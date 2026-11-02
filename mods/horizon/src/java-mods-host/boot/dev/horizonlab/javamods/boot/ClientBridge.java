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
    private static int guiWidth = 427, guiHeight = 240, guiScale = 2, windowWidth = 854, windowHeight = 480;

    static void install() {
        GameBridge.client = ClientBridge::call;
        Clients.dynamic = ClientModels::dynamic;
    }

    private static byte[] call(String op, byte[] input) {
        try {
            var in = new DataInputStream(new ByteArrayInputStream(input));
            return switch (op) {
                case "client_tick" -> tick(in);
                case "client_frame" -> frame(in);
                case "client_input" -> input(in);
                case "client_font" -> font(in);
                case "client_leave" -> leave();
                case "client_models_probe" -> modelsProbe();
                case "client_models_states" -> ClientModels.states();
                case "client_models_take" -> Minecraft.getInstance().level == null ? new byte[0] : ClientModels.take(Minecraft.getInstance().level);
                case "client_shape" -> shape(in);
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
            ClientLevel level = level(minecraft, dim);
            set(ClientPacketListener.class, listener, "level", level);
            LocalPlayer local = GameBridge.construct(LocalPlayer.class, Player.class.getConstructor(Level.class, GameProfile.class), level, listener.getLocalGameProfile());
            set(LocalPlayer.class, local, "minecraft", minecraft);
            set(LocalPlayer.class, local, "connection", listener);
            set(LocalPlayer.class, local, "stats", new net.minecraft.stats.StatsCounter());
            set(LocalPlayer.class, local, "recipeBook", new net.minecraft.client.ClientRecipeBook());
            Inits.run(local, minecraft, level, listener);
            local.setId(server.getId());
            set(LocalPlayer.class, local, "permissions", server.permissions());
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
        Inits.run(level, listener, data, key, holder);
        return level;
    }

    /** A block entity of the window's level: a client copy of the server's, made from its update tag as a chunk's arrival would. */
    public static BlockEntity blockEntity(ClientLevel level, BlockPos pos) {
        long at = pos.asLong();
        var held = BLOCK_ENTITIES.get(at);
        ServerLevel server = GameBridge.levelOrNull(level.dimension().identifier().toString());
        BlockEntity source = server == null ? null : BlockEntities.held(server).get(at);
        boolean gone = source == null || source.isRemoved();
        if (held != null && !held.isRemoved() && !gone && held.getType() == source.getType()) return held;
        if (held != null) {
            BLOCK_ENTITIES.remove(at);
            shapes++;
            ClientModels.dirty(pos);
        }
        if (gone) return null;
        BlockState state = source.getBlockState();
        if (!(state.getBlock() instanceof EntityBlock block)) return null;
        BlockEntity copy = block.newBlockEntity(pos, state);
        if (copy == null) return null;
        copy.setLevel(level);
        var registries = level.registryAccess();
        copy.handleUpdateTag(TagValueInput.create(ProblemReporter.DISCARDING, registries, source.getUpdateTag(registries)));
        copy.clearRemoved();
        BLOCK_ENTITIES.put(at, copy);
        shapes++;
        if (ClientModels.dynamic(state)) ClientModels.dirty(pos);
        copy.onLoad();
        return copy;
    }

    /** The state of a block the window's level has a copy of; air elsewhere, until the client level reads Horizon's own world. */
    public static BlockState blockState(ClientLevel level, BlockPos pos) {
        var entity = blockEntity(level, pos);
        return entity != null ? entity.getBlockState() : net.minecraft.world.level.block.Blocks.AIR.defaultBlockState();
    }

    @SuppressWarnings("unchecked")
    private static void receive() throws ReflectiveOperationException {
        for (Packet<?> packet : Clients.takeForClient(player)) {
            try {
                ((Packet<PacketListener>) packet).handle(listener);
                if (packet instanceof net.minecraft.network.protocol.game.ClientboundBlockEntityDataPacket update) {
                    shapes++;
                    ClientModels.dirty(update.getPos());
                }
            } catch (RunningOnDifferentThreadException e) {
                // Queued on the client's loop; its tasks run below.
            } catch (Throwable t) {
                System.err.println("Horizon: the Java client's " + packet.getClass().getSimpleName() + " failed");
                t.printStackTrace();
            }
        }
        var tasks = BlockableEventLoop.class.getDeclaredMethod("runAllTasks");
        tasks.setAccessible(true);
        tasks.invoke(Minecraft.getInstance());
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
        receive();
        Screen screen = Minecraft.getInstance().gui.screen();
        if (screen != null) screen.tick();
        return status();
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
        ClientModels.forget();
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

    /** The advance of each character as Horizon's font draws it, so text mods lay out measures as it shows. */
    private static byte[] font(DataInputStream in) throws Exception {
        ClientLoader.language(text(in));
        int count = in.readInt();
        float[] advances = new float[count];
        for (int i = 0; i < count; i++) advances[i] = in.readFloat();
        ClientLoader.advances(advances);
        return new byte[] {1};
    }
}
