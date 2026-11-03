package dev.horizonlab.javamods.boot;

import com.mojang.authlib.GameProfile;
import com.mojang.blaze3d.platform.Window;
import io.netty.channel.embedded.EmbeddedChannel;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.CharacterEvent;
import net.minecraft.client.InputType;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.input.MouseButtonInfo;
import net.minecraft.client.multiplayer.ClientCommonPacketListenerImpl;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.multiplayer.ClientPacketListener;
import net.minecraft.client.multiplayer.MultiPlayerGameMode;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.tutorial.Tutorial;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.Connection;
import net.minecraft.network.PacketListener;
import net.minecraft.network.PacketProcessor;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.PacketFlow;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.RunningOnDifferentThreadException;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.ProblemReporter;
import net.minecraft.util.thread.BlockableEventLoop;
import net.minecraft.world.Difficulty;
import net.minecraft.world.TickRateManager;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.EntityBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.dimension.BuiltinDimensionTypes;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.level.storage.WritableLevelData;
import net.neoforged.neoforge.network.registration.NetworkRegistry;

/** The Java client of the window's player: its own level, player and connection as views, the screens mods open, and their drawing for Rust. */
public final class ClientBridge {
    private ClientBridge() {}

    private static UUID player;
    private static net.minecraft.server.level.ServerPlayer serverView;
    private static String dimension;
    private static ClientPacketListener listener;
    private static final Map<Long, BlockEntity> BLOCK_ENTITIES = new HashMap<>();
    private static final Set<Integer> KEYS_DOWN = new HashSet<>();
    private static int opened;
    private static int shapes;
    private static final it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet RESHAPED = new it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet();
    private static int guiWidth = 427, guiHeight = 240, guiScale = 2, windowWidth = 854, windowHeight = 480;

    /** `Minecraft.gameThread`: every call of the client runs on it, whichever of the window's threads asks, as a real client's render thread runs it all. */
    private static Thread renderThread;
    private static java.util.concurrent.ExecutorService render;

    static void install() {
        // In the loader's group, not that of the server thread whose call starts it, so `EffectiveSide` says client.
        ThreadGroup group = Thread.currentThread().getThreadGroup();
        render = java.util.concurrent.Executors.newSingleThreadExecutor(task -> {
            renderThread = new Thread(group, task, "Render thread");
            renderThread.setDaemon(true);
            return renderThread;
        });
        GameBridge.client = ClientBridge::call;
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener(net.neoforged.bus.api.EventPriority.LOWEST, true,
                net.neoforged.neoforge.client.event.InputEvent.MouseScrollingEvent.class, event -> scrollCanceled = event.isCanceled());
        Clients.dynamic = state -> ClientModels.dynamic(state) || BlockRenders.has(state);
    }

    private static byte[] call(String op, byte[] input) {
        if (Thread.currentThread() == renderThread) return run(op, input);
        try {
            return render.submit(() -> run(op, input)).get();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            return new byte[0];
        } catch (java.util.concurrent.ExecutionException e) {
            System.err.println("Horizon: the Java client's " + op + " failed");
            e.getCause().printStackTrace();
            return new byte[0];
        }
    }

    private static byte[] run(String op, byte[] input) {
        try {
            var in = new DataInputStream(new ByteArrayInputStream(input));
            return switch (op) {
                case "client_tick" -> tick(in);
                case "client_frame" -> frame(in);
                case "client_input" -> input(in);
                case "client_scroll" -> scroll(in);
                case "client_font" -> font(in);
                case "client_leave" -> leave();
                case "client_models_probe" -> modelsProbe();
                case "client_models_states" -> ClientModels.states();
                case "client_models_take" -> Minecraft.getInstance().level == null ? new byte[0] : ClientModels.take(Minecraft.getInstance().level);
                case "client_items_external" -> ClientModels.foreignItemIds();
                case "client_items_take" -> BlockRenders.items(in);
                case "client_items_decorations" -> BlockRenders.decorations(in);
                case "client_renders_take" -> Minecraft.getInstance().level == null ? new byte[0] : BlockRenders.take(Minecraft.getInstance().level);
                case "client_shape" -> shape(in);
                case "client_collisions_take" -> collisions();
                default -> throw new IllegalArgumentException(op);
            };
        } catch (Throwable t) {
            System.err.println("Horizon: the Java client's " + op + " failed");
            t.printStackTrace();
            return new byte[0];
        }
    }

    static UUID player() {
        return player;
    }

    public static void send(Packet<?> packet) {
        if (player != null) Clients.toServer(player, packet);
    }

    public static boolean keyDown(int key) {
        return KEYS_DOWN.contains(key);
    }

    private static void set(Class<?> owner, Object target, String field, Object value) throws ReflectiveOperationException {
        Field f = owner.getDeclaredField(field);
        f.setAccessible(true);
        f.set(target, value);
    }

    private static String text(DataInputStream in) throws IOException {
        return new String(in.readNBytes(in.readInt()), java.nio.charset.StandardCharsets.UTF_8);
    }

    /** The window's size as `Window` reports it, for `Screen.init` and the extractor's bounds. */
    private static void size(DataInputStream in) throws ReflectiveOperationException, IOException {
        windowWidth = in.readInt();
        windowHeight = in.readInt();
        guiScale = Math.max(1, in.readInt());
        guiWidth = (int) Math.ceil(windowWidth / (double) guiScale);
        guiHeight = (int) Math.ceil(windowHeight / (double) guiScale);
        Window window = Minecraft.getInstance().getWindow();
        set(Window.class, window, "width", windowWidth);
        set(Window.class, window, "height", windowHeight);
        set(Window.class, window, "framebufferWidth", windowWidth);
        set(Window.class, window, "framebufferHeight", windowHeight);
        set(Window.class, window, "guiScaledWidth", guiWidth);
        set(Window.class, window, "guiScaledHeight", guiHeight);
        set(Window.class, window, "guiScale", guiScale);
    }

    /** Builds the client's views the first time, and a new level when the player changed dimension. */
    private static void join(UUID id, String name, String dim) throws ReflectiveOperationException {
        Minecraft minecraft = Minecraft.getInstance();
        var server = GameBridge.playerView(id);
        // The server's view of the player comes with its first tick; the client's player takes its id, as a login gives it.
        if (server == null) return;
        if (!id.equals(player) || server != serverView) {
            if (player != null) leave();
            player = id;
            serverView = server;
            Clients.join(id);
            set(Minecraft.class, minecraft, "packetProcessor", new PacketProcessor(Thread.currentThread()));
            set(Minecraft.class, minecraft, "gameThread", Thread.currentThread());
            set(Minecraft.class, minecraft, "lastInputType", InputType.NONE);
            set(Minecraft.class, minecraft, "tutorial", new Tutorial(minecraft, minecraft.options));
            var social = GameBridge.construct(net.minecraft.client.gui.screens.social.PlayerSocialManager.class, Object.class.getDeclaredConstructor());
            set(social.getClass(), social, "minecraft", minecraft);
            set(social.getClass(), social, "discoveredNamesToUUID", new HashMap<>());
            set(Minecraft.class, minecraft, "playerSocialManager", social);
            // No signed chat here: no keys, so `enforcesSecureChat` is false as an offline server's.
            set(Minecraft.class, minecraft, "services", new net.minecraft.server.Services(null, com.mojang.authlib.services.ServicesKeySet.EMPTY, null, null, null));
            listener = GameBridge.construct(ClientPacketListener.class, Object.class.getDeclaredConstructor());
            var network = new Connection(PacketFlow.CLIENTBOUND);
            set(Connection.class, network, "channel", new EmbeddedChannel());
            NetworkRegistry.configureMockConnection(network);
            set(ClientCommonPacketListenerImpl.class, listener, "minecraft", minecraft);
            set(ClientCommonPacketListenerImpl.class, listener, "connection", network);
            set(ClientCommonPacketListenerImpl.class, listener, "seenPlayers", new HashMap<>());
            set(ClientCommonPacketListenerImpl.class, listener, "customReportDetails", new HashMap<>());
            set(ClientCommonPacketListenerImpl.class, listener, "connectionType", net.neoforged.neoforge.network.connection.ConnectionType.NEOFORGE);
            set(ClientPacketListener.class, listener, "connectionType", net.neoforged.neoforge.network.connection.ConnectionType.NEOFORGE);
            var registries = GameBridge.registries();
            set(ClientPacketListener.class, listener, "localGameProfile", new GameProfile(id, name));
            set(ClientPacketListener.class, listener, "registryAccess", registries);
            set(ClientPacketListener.class, listener, "enabledFeatures", FeatureFlags.DEFAULT_FLAGS);
            set(ClientPacketListener.class, listener, "commands", new com.mojang.brigadier.CommandDispatcher<>());
            set(ClientPacketListener.class, listener, "playerInfoMap", new HashMap<>());
            set(ClientPacketListener.class, listener, "listedPlayers", new HashSet<>());
            var hashOps = registries.createSerializationContext(net.minecraft.util.HashOps.CRC32C_INSTANCE);
            net.minecraft.network.HashedPatchMap.HashGenerator hashes = component -> component.encodeValue(hashOps).getOrThrow().asInt();
            set(ClientPacketListener.class, listener, "decoratedHashOpsGenerator", hashes);
            Inits.run(listener, minecraft, network);
            set(Minecraft.class, minecraft, "gameMode", new MultiPlayerGameMode(minecraft, listener));
            dimension = null;
        }
        if (!dim.equals(dimension)) {
            if (Minecraft.getInstance().level != null) ChunkViews.forget(Minecraft.getInstance().level);
            dimension = dim;
            BLOCK_ENTITIES.clear();
            STATES.clear();
            ClientLevel level = level(minecraft, dim);
            set(ClientPacketListener.class, listener, "level", level);
            LocalPlayer local = GameBridge.construct(LocalPlayer.class, Player.class.getConstructor(Level.class, GameProfile.class), level, listener.getLocalGameProfile());
            set(LocalPlayer.class, local, "minecraft", minecraft);
            set(LocalPlayer.class, local, "connection", listener);
            set(LocalPlayer.class, local, "stats", new net.minecraft.stats.StatsCounter());
            set(LocalPlayer.class, local, "recipeBook", new net.minecraft.client.ClientRecipeBook());
            Inits.run(local, minecraft, level, listener);
            set(LocalPlayer.class, local, "permissions", net.minecraft.server.permissions.PermissionSet.NO_PERMISSIONS);
            local.input = new net.minecraft.client.player.KeyboardInput(minecraft.options);
            local.setId(server.getId());
            set(Minecraft.class, minecraft, "level", level);
            set(Minecraft.class, minecraft, "player", local);
            level.addEntity(local);
        }
    }

    private static ClientLevel level(Minecraft minecraft, String id) throws ReflectiveOperationException {
        var key = ResourceKey.create(Registries.DIMENSION, Identifier.parse(id));
        ResourceKey<DimensionType> type = key == Level.NETHER ? BuiltinDimensionTypes.NETHER : key == Level.END ? BuiltinDimensionTypes.END : BuiltinDimensionTypes.OVERWORLD;
        Holder<DimensionType> holder = GameBridge.registries().lookupOrThrow(Registries.DIMENSION_TYPE).getOrThrow(type);
        var data = new ClientLevel.ClientLevelData(Difficulty.NORMAL, false, false);
        var parent = Level.class.getDeclaredConstructor(WritableLevelData.class, ResourceKey.class, net.minecraft.core.RegistryAccess.class, Holder.class, boolean.class, boolean.class, long.class, int.class);
        ClientLevel level = GameBridge.construct(ClientLevel.class, parent, data, key, GameBridge.registries(), holder, true, false, 0L, 1000000);
        set(ClientLevel.class, level, "connection", listener);
        set(ClientLevel.class, level, "clientLevelData", data);
        set(ClientLevel.class, level, "tickRateManager", new TickRateManager());
        set(ClientLevel.class, level, "minecraft", minecraft);
        set(ClientLevel.class, level, "players", new java.util.ArrayList<>());
        set(ClientLevel.class, level, "tickingEntities", new net.minecraft.world.level.entity.EntityTickList());
        var callbacks = Class.forName("net.minecraft.client.multiplayer.ClientLevel$EntityCallbacks").getDeclaredConstructor(ClientLevel.class);
        callbacks.setAccessible(true);
        set(ClientLevel.class, level, "entityStorage", new net.minecraft.world.level.entity.TransientEntitySectionManager<>(
                net.minecraft.world.entity.Entity.class, (net.minecraft.world.level.entity.LevelCallback<net.minecraft.world.entity.Entity>) callbacks.newInstance(level)));
        var chunks = GameBridge.construct(net.minecraft.client.multiplayer.ClientChunkCache.class, Object.class.getDeclaredConstructor());
        set(net.minecraft.client.multiplayer.ClientChunkCache.class, chunks, "level", level);
        set(ClientLevel.class, level, "chunkSource", chunks);
        set(ClientLevel.class, level, "modelDataManager", new net.neoforged.neoforge.model.data.ModelDataManager(level));
        set(ClientLevel.class, level, "blockStatePredictionHandler", new net.minecraft.client.multiplayer.prediction.BlockStatePredictionHandler());
        Inits.run(level, listener, data, key, holder);
        return level;
    }

    private static final it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap<BlockState> STATES = new it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap<>();

    /** A block entity of the window's level, as `LevelChunk.getBlockEntity` makes one for a block the server said has one; its data comes with the server's packet. */
    public static BlockEntity blockEntity(ClientLevel level, BlockPos pos) {
        long at = pos.asLong();
        var held = BLOCK_ENTITIES.get(at);
        if (held != null && !held.isRemoved()) return held;
        BlockState state = STATES.get(at);
        if (state == null || !state.hasBlockEntity() || !(state.getBlock() instanceof EntityBlock block)) return null;
        BlockEntity copy = block.newBlockEntity(pos, state);
        if (copy == null) return null;
        copy.setLevel(level);
        copy.clearRemoved();
        BLOCK_ENTITIES.put(at, copy);
        shapes++;
        RESHAPED.add(at);
        if (ClientModels.dynamic(state)) ClientModels.dirty(pos);
        if (BlockRenders.has(state)) BlockRenders.dirty(pos);
        copy.onLoad();
        return copy;
    }

    /** `LevelChunk.setBlockState` on the client, as a server's block update applies: a block entity of the same type stays with the new state, another is removed. */
    public static void setBlock(BlockPos pos, BlockState state) {
        long at = pos.asLong();
        if (state.isAir()) STATES.remove(at);
        else STATES.put(at, state);
        var held = BLOCK_ENTITIES.get(at);
        if (held != null) {
            if (state.hasBlockEntity() && held.getType().isValid(state)) {
                held.setBlockState(state);
            } else {
                held.setRemoved();
                BLOCK_ENTITIES.remove(at);
            }
        }
        shapes++;
        RESHAPED.add(at);
        ClientModels.dirty(pos);
        BlockRenders.dirty(pos);
    }

    /** A block of the window's level changed on the client side: its geometry is baked again if its data decides it. */
    public static void redraw(BlockState state, BlockPos pos) {
        if (ClientModels.dynamic(state)) ClientModels.dirty(pos);
        if (BlockRenders.has(state)) BlockRenders.dirty(pos);
    }

    /** The state the server last sent for a block; air for one it never sent. */
    public static BlockState blockState(ClientLevel level, BlockPos pos) {
        return STATES.getOrDefault(pos.asLong(), net.minecraft.world.level.block.Blocks.AIR.defaultBlockState());
    }

    @SuppressWarnings("unchecked")
    private static void receive() throws ReflectiveOperationException {
        var updated = new ArrayList<BlockPos>();
        for (Packet<?> packet : Clients.takeForClient(player)) {
            if (packet instanceof net.minecraft.network.protocol.game.ClientboundBlockEntityDataPacket update) updated.add(update.getPos());
            try {
                ((Packet<PacketListener>) packet).handle(listener);
            } catch (RunningOnDifferentThreadException e) {
                // Queued on the client's loop; its tasks run below.
            } catch (Throwable t) {
                System.err.println("Horizon: the Java client's " + packet.getClass().getSimpleName() + " failed");
                t.printStackTrace();
            }
        }
        // `Minecraft.tick`: packets that came in off the client's thread wait on its packet processor, then queued tasks run.
        Minecraft.getInstance().packetProcessor().processQueuedPackets();
        var tasks = BlockableEventLoop.class.getDeclaredMethod("runAllTasks");
        tasks.setAccessible(true);
        tasks.invoke(Minecraft.getInstance());
        for (BlockPos pos : updated) {
            shapes++;
            RESHAPED.add(pos.asLong());
            ClientModels.dirty(pos);
            BlockRenders.dirty(pos);
        }
    }

    /** Once a client tick: packets from the server, then the open screen's tick. Answers whether a Java screen is open and how many have opened. */
    private static byte[] tick(DataInputStream in) throws Exception {
        var id = new UUID(in.readLong(), in.readLong());
        String name = text(in);
        String dim = text(in);
        size(in);
        join(id, name, dim);
        // An empty answer: not joined yet, so the window asks again next tick.
        if (player == null) return new byte[0];
        keys(in);
        receive();
        Screen screen = Minecraft.getInstance().gui.screen();
        if (screen != null) screen.tick();
        return status();
    }

    /** `KeyboardHandler.keyPress`'s part for the keys the window's world took since the last tick, then `LocalPlayer.aiStep`'s read of them. */
    private static void keys(DataInputStream in) throws IOException {
        for (int n = in.readInt(); n > 0; n--) {
            var key = com.mojang.blaze3d.platform.InputConstants.getKey(text(in));
            boolean down = in.readBoolean();
            if (down) KEYS_DOWN.add(key.getValue());
            else KEYS_DOWN.remove(key.getValue());
            net.minecraft.client.KeyMapping.set(key, down);
            if (down) net.minecraft.client.KeyMapping.click(key);
        }
        LocalPlayer local = Minecraft.getInstance().player;
        if (local != null) local.input.tick();
        int selected = in.readInt();
        if (local != null) local.getInventory().setSelectedSlot(selected);
    }

    private static boolean scrollCanceled;

    /** The wheel in the world, through the real `MouseHandler.onScroll`; answers whether a mod's listener canceled the hotbar's turn. */
    private static byte[] scroll(DataInputStream in) throws Exception {
        double dx = in.readDouble();
        double dy = in.readDouble();
        Minecraft minecraft = Minecraft.getInstance();
        scrollCanceled = false;
        // Horizon has already applied its wheel options; Java's are the defaults, which leave these as they are.
        if (player != null && minecraft.gui.screen() == null) minecraft.mouseHandler.onScroll(minecraft.getWindow().handle(), dx, dy);
        return new byte[] {(byte) (scrollCanceled ? 1 : 0)};
    }

    private record Sound(String id, float volume, float pitch) {}

    private static final java.util.List<Sound> SOUNDS = new java.util.ArrayList<>();

    /** `SoundManager.play`: Horizon plays it. */
    public static void sound(net.minecraft.client.resources.sounds.SoundInstance instance) {
        float volume = 1, pitch = 1;
        if (instance instanceof net.minecraft.client.resources.sounds.AbstractSoundInstance sound) {
            try {
                var v = net.minecraft.client.resources.sounds.AbstractSoundInstance.class.getDeclaredField("volume");
                var p = net.minecraft.client.resources.sounds.AbstractSoundInstance.class.getDeclaredField("pitch");
                v.setAccessible(true);
                p.setAccessible(true);
                volume = v.getFloat(sound);
                pitch = p.getFloat(sound);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }
        SOUNDS.add(new Sound(instance.getIdentifier().toString(), volume, pitch));
    }

    private static byte[] status() throws IOException {
        Screen screen = Minecraft.getInstance().gui.screen();
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeBoolean(screen != null);
        out.writeInt(opened);
        out.writeBoolean(screen != null && screen.isPauseScreen());
        out.writeInt(SOUNDS.size());
        for (Sound sound : SOUNDS) {
            byte[] id = sound.id.getBytes(java.nio.charset.StandardCharsets.UTF_8);
            out.writeInt(id.length);
            out.write(id);
            out.writeFloat(sound.volume);
            out.writeFloat(sound.pitch);
        }
        SOUNDS.clear();
        out.writeInt(shapes);
        out.writeInt(ClientModels.pending());
        out.writeInt(BlockRenders.pending());
        return bytes.toByteArray();
    }

    /** `Gui.setScreen` without the window's own parts: the mouse, the narrator, a title screen when nothing is left. */
    public static void setScreen(Screen previous, Screen next) {
        if (previous != null) previous.removed();
        if (next != null) {
            opened++;
            next.added();
            next.init(guiWidth, guiHeight);
        }
    }

    /** The open screen's drawing this frame, as Rust draws it. */
    private static byte[] frame(DataInputStream in) throws Exception {
        size(in);
        double mouseX = in.readDouble();
        double mouseY = in.readDouble();
        float partial = in.readFloat();
        receive();
        Minecraft minecraft = Minecraft.getInstance();
        Screen screen = minecraft.gui.screen();
        if (screen == null) return new byte[0];
        if (screen.width != guiWidth || screen.height != guiHeight) screen.resize(guiWidth, guiHeight);
        var recorder = new Recorder();
        var graphics = new GuiGraphicsExtractor(minecraft, recorder, (int) mouseX, (int) mouseY);
        screen.extractRenderStateWithTooltipAndSubtitles(graphics, (int) mouseX, (int) mouseY, partial);
        graphics.extractDeferredElements((int) mouseX, (int) mouseY, partial);
        return recorder.bytes();
    }

    /** Mouse and keys, as `MouseHandler` and `KeyboardHandler` hand them to the open screen. */
    private static byte[] input(DataInputStream in) throws Exception {
        int kind = in.readInt();
        double x = in.readDouble();
        double y = in.readDouble();
        int a = in.readInt();
        int b = in.readInt();
        int modifiers = in.readInt();
        double dx = in.readDouble();
        double dy = in.readDouble();
        String key = text(in);
        // Keys come by name (`key.keyboard.e`), which Java's own table turns into its scancode.
        if (kind == 5 || kind == 6) {
            a = com.mojang.blaze3d.platform.InputConstants.getKey(key).getValue();
            b = a;
        }
        Screen screen = Minecraft.getInstance().gui.screen();
        boolean handled = false;
        if (screen != null) {
            switch (kind) {
                case 0 -> handled = screen.mouseClicked(new MouseButtonEvent(x, y, new MouseButtonInfo(a, modifiers)), b != 0);
                case 1 -> handled = screen.mouseReleased(new MouseButtonEvent(x, y, new MouseButtonInfo(a, modifiers)));
                case 2 -> screen.mouseMoved(x, y);
                case 3 -> handled = screen.mouseDragged(new MouseButtonEvent(x, y, new MouseButtonInfo(a, modifiers)), dx, dy);
                case 4 -> handled = screen.mouseScrolled(x, y, dx, dy);
                case 5 -> {
                    KEYS_DOWN.add(a);
                    handled = screen.keyPressed(new KeyEvent(a, b, modifiers));
                }
                case 6 -> {
                    KEYS_DOWN.remove(a);
                    handled = screen.keyReleased(new KeyEvent(a, b, modifiers));
                }
                default -> handled = screen.charTyped(new CharacterEvent(a));
            }
        }
        receive();
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeBoolean(handled);
        bytes.write(status());
        return bytes.toByteArray();
    }

    /** Dev: bakes the code-built block models and reports how many states have quads. */
    private static byte[] modelsProbe() throws Exception {
        var models = ClientModels.baked();
        int withQuads = 0;
        var random = net.minecraft.util.RandomSource.create(42);
        for (var model : models.values()) {
            var parts = new java.util.ArrayList<net.minecraft.client.renderer.block.dispatch.BlockStateModelPart>();
            model.collectParts(random, parts);
            if (!parts.isEmpty()) withQuads++;
        }
        String text = models.size() + " states, " + withQuads + " with parts";
        System.err.println("Horizon: models probe " + text);
        return text.getBytes(java.nio.charset.StandardCharsets.UTF_8);
    }

    /** The window left its world: no screen, no level, no player, until it joins another. */
    private static byte[] leave() throws ReflectiveOperationException {
        Minecraft minecraft = Minecraft.getInstance();
        Field screen = net.minecraft.client.gui.Gui.class.getDeclaredField("screen");
        screen.setAccessible(true);
        Screen open = (Screen) screen.get(minecraft.gui);
        screen.set(minecraft.gui, null);
        if (open != null) open.removed();
        if (minecraft.level != null) ChunkViews.forget(minecraft.level);
        set(Minecraft.class, minecraft, "player", null);
        set(Minecraft.class, minecraft, "level", null);
        BLOCK_ENTITIES.clear();
        STATES.clear();
        ClientModels.forget();
        BlockRenders.forget();
        if (player != null) Clients.leave(player);
        player = null;
        serverView = null;
        dimension = null;
        return new byte[] {1};
    }

    /** The outline of a block with a block entity, as its client copy shapes it; empty when there is no copy. */
    private static byte[] shape(DataInputStream in) throws IOException {
        var level = Minecraft.getInstance().level;
        if (level == null) return new byte[0];
        var pos = BlockPos.of(in.readLong());
        var entity = blockEntity(level, pos);
        if (entity == null) return new byte[0];
        var boxes = entity.getBlockState().getShape(level, pos, net.minecraft.world.phys.shapes.CollisionContext.empty()).toAabbs();
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeByte(1);
        out.writeInt(boxes.size());
        for (var box : boxes) {
            out.writeDouble(box.minX);
            out.writeDouble(box.minY);
            out.writeDouble(box.minZ);
            out.writeDouble(box.maxX);
            out.writeDouble(box.maxY);
            out.writeDouble(box.maxZ);
        }
        return bytes.toByteArray();
    }

    /** `getCollisionShape(level, pos)` of each block whose client copy changed since the last take, as the window's collisions use it. */
    private static byte[] collisions() throws IOException {
        var level = Minecraft.getInstance().level;
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        var shaped = new java.util.ArrayList<BlockPos>();
        for (long at : RESHAPED) {
            var state = blockState(level, BlockPos.of(at));
            if (level != null && state.hasBlockEntity() && BlockEntities.ours(state)) shaped.add(BlockPos.of(at));
        }
        RESHAPED.clear();
        out.writeInt(shaped.size());
        for (BlockPos pos : shaped) {
            var state = blockState(level, pos);
            var boxes = Clients.boxes(state.getCollisionShape(level, pos, net.minecraft.world.phys.shapes.CollisionContext.empty()));
            out.writeLong(pos.asLong());
            out.writeInt(Flat.raw(state));
            out.writeInt(boxes.length / 6);
            for (double v : boxes) out.writeDouble(v);
        }
        return bytes.toByteArray();
    }

    /** The advance of each character as Horizon's font draws it, so text mods lay out measures as it shows. */
    private static float[] advances = new float[0];

    /** A range of Horizon's font's advances; the one from 0 starts a new table, in this language. */
    private static byte[] font(DataInputStream in) throws Exception {
        String language = text(in);
        int start = in.readInt();
        int count = in.readInt();
        if (start == 0) {
            ClientLoader.language(language);
            advances = new float[0];
        }
        float[] table = java.util.Arrays.copyOf(advances, Math.max(advances.length, start + count));
        for (int i = 0; i < count; i++) table[start + i] = in.readFloat();
        advances = table;
        ClientLoader.advances(table);
        return new byte[] {1};
    }
}
