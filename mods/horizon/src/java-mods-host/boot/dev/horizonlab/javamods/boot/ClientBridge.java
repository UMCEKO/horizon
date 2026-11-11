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
import net.minecraft.client.multiplayer.ClientRegistryLayer;
import net.minecraft.client.multiplayer.KnownPacksManager;
import net.minecraft.client.multiplayer.CommonListenerCookie;
import net.minecraft.client.multiplayer.LevelLoadTracker;
import net.minecraft.client.multiplayer.RegistryDataCollector;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.telemetry.TelemetryEventSender;
import net.minecraft.client.telemetry.WorldSessionTelemetryManager;
import net.minecraft.client.tutorial.Tutorial;
import net.minecraft.core.BlockPos;
import net.minecraft.core.RegistryAccess;
import net.minecraft.network.Connection;
import net.minecraft.network.PacketListener;
import net.minecraft.network.PacketProcessor;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.PacketFlow;
import net.minecraft.network.protocol.common.ClientboundUpdateTagsPacket;
import net.minecraft.network.protocol.configuration.ClientboundFinishConfigurationPacket;
import net.minecraft.network.protocol.configuration.ClientboundRegistryDataPacket;
import net.minecraft.network.protocol.configuration.ClientboundSelectKnownPacks;
import net.minecraft.network.protocol.configuration.ServerboundSelectKnownPacks;
import net.minecraft.network.protocol.configuration.ClientboundUpdateEnabledFeaturesPacket;
import net.minecraft.network.protocol.game.ServerboundAcceptTeleportationPacket;
import net.minecraft.network.protocol.game.ServerboundClientTickEndPacket;
import net.minecraft.network.protocol.game.ServerboundMovePlayerPacket;
import net.minecraft.network.protocol.game.ServerboundMoveVehiclePacket;
import net.minecraft.network.protocol.game.ServerboundPlayerCommandPacket;
import net.minecraft.network.protocol.game.ServerboundPlayerInputPacket;
import net.minecraft.network.protocol.game.ServerboundPlayerLoadedPacket;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.RunningOnDifferentThreadException;
import net.minecraft.server.ServerLinks;
import net.minecraft.server.packs.resources.ResourceProvider;
import net.minecraft.util.thread.BlockableEventLoop;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.block.EntityBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityTicker;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.entity.TickingBlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.network.connection.ConnectionType;
import net.neoforged.neoforge.network.registration.NetworkRegistry;

/** The Java client of the window's player: its own level, player and connection as views, the screens mods open, and their drawing for Rust. */
public final class ClientBridge {
    private ClientBridge() {}

    private static UUID player;
    private static ClientPacketListener listener;
    private static final Map<Long, BlockEntity> BLOCK_ENTITIES = new HashMap<>();
    private static final Set<Integer> KEYS_DOWN = new HashSet<>();
    private static int opened;
    private static Screen lastScreen;
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
        for (String name : OPS.keySet()) Crossing.client(name);
        WorldFrame.decide();
        NativeSounds.decide();
        Shells.install();
        HudLayers.install(net.neoforged.neoforge.common.NeoForge.EVENT_BUS);
        CreativeTabs.install();
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener(net.neoforged.bus.api.EventPriority.LOWEST, true,
                net.neoforged.neoforge.client.event.InputEvent.MouseScrollingEvent.class, event -> scrollCanceled = event.isCanceled());
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener(net.neoforged.bus.api.EventPriority.LOWEST, true,
                net.neoforged.neoforge.client.event.InputEvent.InteractionKeyMappingTriggered.class, ClientBridge::clicked);
        Clients.dynamic = state -> ClientModels.dynamic(state) || BlockRenders.has(state);
        Clients.clientEntities = chunk -> {
            var here = new HashMap<BlockPos, BlockEntity>();
            for (var entity : BLOCK_ENTITIES.values()) if (net.minecraft.world.level.ChunkPos.containing(entity.getBlockPos()).equals(chunk)) here.put(entity.getBlockPos(), entity);
            return here;
        };
        try {
            render.submit(() -> {
                claimThread();
                return null;
            }).get();
        } catch (InterruptedException | java.util.concurrent.ExecutionException e) {
            throw new IllegalStateException(e);
        }
    }

    /** What `Minecraft`'s constructor takes from the thread that builds it, once. */
    private static void claimThread() throws ReflectiveOperationException {
        Minecraft minecraft = Minecraft.getInstance();
        com.mojang.blaze3d.systems.RenderSystem.initRenderThread();
        set(Minecraft.class, minecraft, "packetProcessor", new PacketProcessor(Thread.currentThread()));
        set(Minecraft.class, minecraft, "gameThread", Thread.currentThread());
        set(Minecraft.class, minecraft, "lastInputType", InputType.NONE);
        set(Minecraft.class, minecraft, "tutorial", new Tutorial(minecraft, minecraft.options));
        var social = GameBridge.construct(net.minecraft.client.gui.screens.social.PlayerSocialManager.class, Object.class.getDeclaredConstructor());
        set(social.getClass(), social, "minecraft", minecraft);
        set(social.getClass(), social, "discoveredNamesToUUID", new HashMap<>());
        set(Minecraft.class, minecraft, "playerSocialManager", social);
        // No signed chat here: no keys, so `enforcesSecureChat` is false as an offline server's.
        set(Minecraft.class, minecraft, "services", new net.minecraft.server.Services(null, com.mojang.authlib.services.ServicesKeySet.EMPTY, null, null, ClientLoader.profiles(minecraft)));
        set(Minecraft.class, minecraft, "isLocalServer", true);
        set(Minecraft.class, minecraft, "userApiService", com.mojang.authlib.minecraft.UserApiService.OFFLINE);
        set(Minecraft.class, minecraft, "reportingContext", net.minecraft.client.multiplayer.chat.report.ReportingContext.create(net.minecraft.client.multiplayer.chat.report.ReportEnvironment.local(), com.mojang.authlib.minecraft.UserApiService.OFFLINE));
        set(Minecraft.class, minecraft, "userPropertiesFuture", java.util.concurrent.CompletableFuture.completedFuture(com.mojang.authlib.minecraft.UserApiService.OFFLINE_PROPERTIES));
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

    private interface Op {
        byte[] run(DataInputStream in) throws Throwable;
    }

    /** Every op of the window's calls, in the order Rust reads them. */
    private static final java.util.Map<String, Op> OPS = new java.util.LinkedHashMap<>();

    static {
        OPS.put("client_connect", in -> connect(in));
        OPS.put("client_connect_remote", in -> RemoteClient.connect(in));
        OPS.put("client_wire_take", in -> RemoteClient.take());
        OPS.put("client_remote_frame", in -> RemoteClient.configure(in));
        OPS.put("client_click", in -> click(in));
        OPS.put("client_tick", in -> tick(in));
        OPS.put("client_frame", in -> frame(in));
        OPS.put("client_hud_frame", in -> hudFrame(in));
        OPS.put("client_world_frame", in -> worldFrame(in));
        OPS.put("client_input", in -> input(in));
        OPS.put("client_scroll", in -> scroll(in));
        OPS.put("client_font", in -> font(in));
        OPS.put("client_leave", in -> leave());
        OPS.put("client_models_probe", in -> modelsProbe());
        OPS.put("client_resources", in -> ClientResources.export());
        OPS.put("client_baked", in -> ClientResources.baked(java.nio.file.Path.of(new String(in.readAllBytes(), java.nio.charset.StandardCharsets.UTF_8))));
        OPS.put("client_keys", in -> ModKeyMappings.export());
        OPS.put("client_keys_apply", in -> ModKeyMappings.apply(in));
        OPS.put("client_models_take", in -> Minecraft.getInstance().level == null ? new byte[0] : ClientModels.take(Minecraft.getInstance().level));
        OPS.put("client_items_take", in -> BlockRenders.items(in));
        OPS.put("client_items_decorations", in -> BlockRenders.decorations(in));
        OPS.put("client_renders_take", in -> Minecraft.getInstance().level == null ? new byte[0] : BlockRenders.take(Minecraft.getInstance().level));
        OPS.put("client_shape", in -> shape(in));
        OPS.put("client_destroy_progress", in -> destroyProgress(in));
        OPS.put("client_item_text", in -> ItemText.take(in));
        OPS.put("client_collisions_take", in -> collisions());
        OPS.put("client_creative_tabs", in -> CreativeTabs.take());
        OPS.put("client_sound_listened", in -> NativeSounds.listenedReply());
        OPS.put("client_play_sound", in -> NativeSounds.ask(in));
        OPS.put("client_shell_frame", in -> shellFrame(in));
        OPS.put("client_shell_input", in -> shellInput(in));
    }

    private static byte[] run(String op, byte[] input) {
        try {
            var in = new DataInputStream(new ByteArrayInputStream(input));
            Op found = OPS.get(op);
            if (found == null) throw new IllegalArgumentException(op);
            return found.run(in);
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
        if (player == null || sentByWindow(packet)) return;
        if (RemoteClient.active()) RemoteClient.outbound(Wire.toServer(packet));
        else Clients.toServer(player, packet);
    }

    /** The window moves this player and acts for it, and Horizon's own client tells the server, so these would say it twice. */
    private static boolean sentByWindow(Packet<?> packet) {
        return packet instanceof ServerboundMovePlayerPacket || packet instanceof ServerboundPlayerInputPacket || packet instanceof ServerboundPlayerCommandPacket
                || packet instanceof ServerboundClientTickEndPacket || packet instanceof ServerboundAcceptTeleportationPacket || packet instanceof ServerboundPlayerLoadedPacket
                || packet instanceof ServerboundMoveVehiclePacket || packet instanceof net.minecraft.network.protocol.game.ServerboundUseItemOnPacket
                || packet instanceof net.minecraft.network.protocol.game.ServerboundUseItemPacket || packet instanceof net.minecraft.network.protocol.game.ServerboundPlayerActionPacket
                || packet instanceof net.minecraft.network.protocol.game.ServerboundInteractPacket || packet instanceof net.minecraft.network.protocol.game.ServerboundAttackPacket
                || packet instanceof net.minecraft.network.protocol.game.ServerboundPunchPacket || packet instanceof net.minecraft.network.protocol.game.ServerboundPickItemFromBlockPacket
                || packet instanceof net.minecraft.network.protocol.game.ServerboundPickItemFromEntityPacket || packet instanceof net.minecraft.network.protocol.game.ServerboundSetCarriedItemPacket
                || packet instanceof net.minecraft.network.protocol.game.ServerboundSpectatorActionPacket;
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

    /** `ClientHandshakePacketListenerImpl.handleLoginFinished`, then configuration as `ClientConfigurationPacketListenerImpl` runs it over a memory connection: the play phase's listener, with registries from what the server sent. */
    private static byte[] connect(DataInputStream in) throws Exception {
        var id = new UUID(in.readLong(), in.readLong());
        String name = text(in);
        if (player != null) leave();
        Minecraft minecraft = Minecraft.getInstance();
        // `Minecraft.doWorldLoad`: the client knows the integrated server it joins.
        ClientLoader.attach(GameBridge.server());
        set(Minecraft.class, minecraft, "isLocalServer", true);
        var server = Clients.configuration();
        var collector = new RegistryDataCollector();
        var features = FeatureFlags.DEFAULT_FLAGS;
        KnownPacksManager knownPacks = null;
        var inbound = new java.util.ArrayDeque<Packet<?>>(server.start().get());
        // `ClientConfigurationPacketListenerImpl`'s handlers, until the server finishes configuration.
        while (true) {
            Packet<?> packet = inbound.poll();
            if (packet == null) throw new IllegalStateException("the server's configuration ended without ClientboundFinishConfigurationPacket");
            if (packet instanceof ClientboundFinishConfigurationPacket) break;
            switch (packet) {
                case ClientboundUpdateEnabledFeaturesPacket enabled -> features = FeatureFlags.REGISTRY.fromNames(enabled.features());
                case ClientboundSelectKnownPacks select -> {
                    knownPacks = new KnownPacksManager();
                    inbound.addAll(server.knownPacks().apply(new ServerboundSelectKnownPacks(knownPacks.trySelectingPacks(select.knownPacks()))));
                }
                case ClientboundRegistryDataPacket data -> collector.appendContents(data.registry(), data.entries());
                case ClientboundUpdateTagsPacket tags -> collector.appendTags(tags.tags());
                default -> throw new IllegalStateException("configuration packet " + packet);
            }
        }
        // `handleConfigurationFinished` over a memory connection.
        RegistryAccess.Frozen registries;
        var received = ClientRegistryLayer.createRegistryAccess().compositeAccess();
        if (knownPacks == null) {
            registries = collector.collectGameRegistries(ResourceProvider.EMPTY, received, true);
        } else {
            try (var packs = knownPacks.createResourceManager()) {
                registries = collector.collectGameRegistries(packs, received, true);
            }
        }
        var local = minecraft.getSingleplayerServer();
        if (local != null) {
            var filter = net.minecraft.client.multiplayer.ClientConfigurationPacketListenerImpl.class.getDeclaredMethod("filterRegistries", RegistryAccess.Frozen.class, java.util.stream.Stream.class);
            filter.setAccessible(true);
            registries = (RegistryAccess.Frozen) filter.invoke(null, local.registryAccess(), registries.listRegistryKeys());
        }
        begin(id, name, registries, features, null);
        Clients.join(id);
        return new byte[] {1};
    }

    /** The play phase's listener over a mock connection, with the registries configuration gave. */
    static void begin(UUID id, String name, RegistryAccess.Frozen registries, net.minecraft.world.flag.FeatureFlagSet features, net.minecraft.client.multiplayer.ServerData server) throws Exception {
        Minecraft minecraft = Minecraft.getInstance();
        var network = new Connection(PacketFlow.CLIENTBOUND);
        set(Connection.class, network, "channel", new EmbeddedChannel());
        NetworkRegistry.configureMockConnection(network);
        var telemetry = new WorldSessionTelemetryManager(TelemetryEventSender.DISABLED, false, null, null, UUID.randomUUID());
        var cookie = new CommonListenerCookie(new LevelLoadTracker(), new GameProfile(id, name), telemetry, registries, features, null, server, null, new HashMap<>(), null, Map.of(), ServerLinks.EMPTY, Map.of(), false, ConnectionType.NEOFORGE);
        listener = new ClientPacketListener(minecraft, network, cookie);
        set(Connection.class, network, "packetListener", listener);
        Clients.registries = registries;
        player = id;
    }

    /** The level changed, as `Minecraft.setLevel`: what this client knew of the last one is gone, as a real client's chunks go. */
    public static void levelChanged() {
        ClientLevel previous = Minecraft.getInstance().level;
        if (previous != null) ChunkViews.forget(previous);
        BLOCK_ENTITIES.clear();
        TICKERS.clear();
        STATES.clear();
        CHUNKS.clear();
        HELD.clear();
    }

    /** `LevelChunk.updateBlockEntityTicker`: one ticker a block entity keeps, its `getTicker` asked again when its state changes. */
    private static final class Ticker implements TickingBlockEntity {
        private final BlockEntity entity;
        private BlockEntityTicker<BlockEntity> ticker;

        Ticker(BlockEntity entity) {
            this.entity = entity;
            rebind();
        }

        @SuppressWarnings({"unchecked", "rawtypes"})
        void rebind() {
            BlockState state = entity.getBlockState();
            ticker = state.getTicker(entity.getLevel(), (BlockEntityType) entity.getType());
        }

        @Override
        public void tick() {
            if (ticker == null || entity.isRemoved() || !entity.hasLevel()) return;
            BlockPos pos = entity.getBlockPos();
            BlockState state = entity.getLevel().getBlockState(pos);
            if (entity.getType().isValid(state)) ticker.tick(entity.getLevel(), pos, state, entity);
        }

        @Override
        public boolean isRemoved() {
            return entity.isRemoved();
        }

        @Override
        public BlockPos getPos() {
            return entity.getBlockPos();
        }

        @Override
        public String getType() {
            return String.valueOf(net.minecraft.core.registries.BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(entity.getType()));
        }
    }

    private static final Map<Long, Ticker> TICKERS = new HashMap<>();

    private static final it.unimi.dsi.fastutil.longs.LongOpenHashSet CHUNKS = new it.unimi.dsi.fastutil.longs.LongOpenHashSet();

    /** `ClientChunkCache.hasChunk`: the columns it has been given and not yet dropped. */
    static boolean hasChunk(int x, int z) {
        return CHUNKS.contains(net.minecraft.world.level.ChunkPos.pack(x, z));
    }

    private static final it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap<BlockState> STATES = new it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap<>();

    /** A block entity of the window's level, as `LevelChunk.getBlockEntity` makes one for a block the server said has one; its data comes with the server's packet. */
    public static BlockEntity blockEntity(ClientLevel level, BlockPos pos) {
        long at = pos.asLong();
        var held = BLOCK_ENTITIES.get(at);
        if (held != null && !held.isRemoved()) return held;
        BlockState state = hasChunk(pos.getX() >> 4, pos.getZ() >> 4) ? blockState(level, pos) : null;
        if (state == null || !state.hasBlockEntity() || !(state.getBlock() instanceof EntityBlock block)) return null;
        BlockEntity copy = block.newBlockEntity(pos, state);
        if (copy == null) return null;
        copy.setLevel(level);
        copy.clearRemoved();
        BLOCK_ENTITIES.put(at, copy);
        var ticker = new Ticker(copy);
        TICKERS.put(at, ticker);
        level.addBlockEntityTicker(ticker);
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
                TICKERS.get(at).rebind();
            } else {
                held.setRemoved();
                BLOCK_ENTITIES.remove(at);
                TICKERS.remove(at);
            }
        }
        shapes++;
        RESHAPED.add(at);
        ClientModels.dirty(pos);
        BlockRenders.dirty(pos);
        // `LevelChunk.setBlockState` makes a new block's block entity as it sets the block.
        ClientLevel level = Minecraft.getInstance().level;
        if (level != null) blockEntity(level, pos);
    }

    /** A block of the window's level changed on the client side: its geometry is baked again if its data decides it. */
    public static void redraw(BlockState state, BlockPos pos) {
        if (ClientModels.dynamic(state)) ClientModels.dirty(pos);
        if (BlockRenders.has(state)) BlockRenders.dirty(pos);
    }

    /** The state the server sent this client in a block update, or the client set itself; else what the window's chunk holds. */
    public static BlockState blockState(ClientLevel level, BlockPos pos) {
        BlockState held = STATES.get(pos.asLong());
        return held != null ? held : ClientWorld.state(pos);
    }

    private static final Map<Long, ArrayList<net.minecraft.network.protocol.game.ClientboundBlockEntityDataPacket>> HELD = new HashMap<>();

    @SuppressWarnings("unchecked")
    private static void receive() throws ReflectiveOperationException {
        var updated = new ArrayList<BlockPos>();
        for (Packet<?> packet : Clients.takeForClient(player)) {
            if (packet instanceof net.minecraft.network.protocol.game.ClientboundBlockEntityDataPacket update) {
                var at = update.getPos();
                // A chunk's block entities travel with its data: until the window has the chunk, this client has no block entity to give theirs to.
                if (!hasChunk(at.getX() >> 4, at.getZ() >> 4)) {
                    HELD.computeIfAbsent(net.minecraft.world.level.ChunkPos.pack(at.getX() >> 4, at.getZ() >> 4), key -> new ArrayList<>()).add(update);
                    continue;
                }
                updated.add(at);
            }
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

    /** `Minecraft.runTick`: the window's input since the last tick, what the server sent, then `Minecraft.tick` for each tick the window owes. Answers whether a Java screen is open and how many have opened; empty with no connection. */
    private static byte[] tick(DataInputStream in) throws Exception {
        int ticks = in.readInt();
        float partial = in.readFloat();
        if (in.readBoolean()) size(in);
        Minecraft minecraft = Minecraft.getInstance();
        events(in);
        if (in.readBoolean()) moved = new Motion(in.readDouble(), in.readDouble(), in.readDouble(), in.readFloat(), in.readFloat(), in.readDouble(), in.readDouble(), in.readDouble(), in.readBoolean());
        ClientSounds.finished(in);
        if (player == null) return new byte[0];
        turnPlayer(minecraft);
        receive();
        chunkEvents();
        cameraAndPick(minecraft);
        // `runTick`: the game time advances, then the window's own count of ticks and its progress into the next replace the timer's.
        advance(minecraft, partial);
        for (int n = 0; n < ticks; n++) {
            Animate.next();
            minecraft.tick();
        }
        windowTicks++;
        timer(minecraft).updatePauseState(minecraft.isPaused());
        timer(minecraft).updateFrozenState(!((dev.horizonlab.javamods.boot.mixin.MinecraftActions) minecraft).horizon$runningNormally());
        if (minecraft.level != null) BlockRenders.probe(minecraft.level, BLOCK_ENTITIES.values());
        cameraAndPick(minecraft);
        byte[] hud = hud();
        byte[] world = WorldFrame.take(WorldFrame.camera(), partial, timer(minecraft), windowWidth, windowHeight, false);
        recordShell(minecraft, minecraft.getDeltaTracker().getGameTimeDeltaPartialTick(false));
        return status(hud, world);
    }

    private static byte[] lastHud;

    /** The frame after a tick, in `runTick`'s order: the camera aligns with the player (mods aim from it), then the crosshair target is picked. `Camera.update` also builds the culling frustum on the GPU device, so only its alignment step runs. */
    private static void cameraAndPick(Minecraft minecraft) {
        if (minecraft.player == null || minecraft.level == null) return;
        var camera = minecraft.gameRenderer.mainCamera();
        if (camera.entity() == null) camera.setEntity(minecraft.player);
        ((dev.horizonlab.javamods.boot.mixin.CameraAccess) camera).horizon$align(1.0F);
        ((dev.horizonlab.javamods.boot.mixin.MinecraftActions) minecraft).horizon$pick(1.0F);
    }

    private static final boolean HUD_DEBUG = System.getenv("HORIZON_JAVA_DEBUG") != null;
    private static String lastHudDebug;

    private static net.minecraft.client.DeltaTracker.Timer timer(Minecraft minecraft) {
        return (net.minecraft.client.DeltaTracker.Timer) minecraft.getDeltaTracker();
    }

    /** `advanceGameTime` for the window's own tick count, then the window's progress into the next tick as the residual `getGameTimeDeltaPartialTick` reports. */
    private static void advance(Minecraft minecraft, float partial) {
        var timer = timer(minecraft);
        timer.advanceGameTime(net.minecraft.util.Util.getMillis());
        ((dev.horizonlab.javamods.boot.mixin.DeltaTrackerAccess) timer).horizon$residual(partial);
    }

    /** The recording of the mods' HUD layers with the timer's frame timing. */
    private static byte[] record(Minecraft minecraft, net.minecraft.client.DeltaTracker delta) throws IOException {
        var recorder = new Recorder();
        var hud = minecraft.gui.hud;
        HudLayers.begin(recorder, () -> hud.leftHeight, () -> hud.rightHeight);
        try {
            if (minecraft.player != null && minecraft.level != null) {
                int x = (int) minecraft.mouseHandler.getScaledXPos(minecraft.getWindow());
                int y = (int) minecraft.mouseHandler.getScaledYPos(minecraft.getWindow());
                var graphics = new GuiGraphicsExtractor(minecraft, recorder, x, y);
                hud.extractRenderState(graphics, delta);
                graphics.extractDeferredElements(x, y, delta.getGameTimeDeltaPartialTick(false));
            }
        } catch (Throwable t) {
            HudLayers.finish();
            throw t;
        }
        return HudLayers.finish();
    }

    /** True while the HUD is changing: the window also records frames with their own timing, between the tick's recordings. */
    private static boolean animating;
    private static byte[] lastRecorded;
    private static int ticksSinceFrame;

    /** A recording of either source: differing from the one before it means the HUD is changing, two alike end it. */
    private static void noted(byte[] bytes) {
        boolean was = animating;
        boolean identical = java.util.Arrays.equals(bytes, lastRecorded);
        if (!identical) animating = true;
        else if (HudAnimation.ends(true, timer(Minecraft.getInstance()).getRealtimeDeltaTicks())) animating = false;
        lastRecorded = bytes;
        if (animating && !was) ticksSinceFrame = 0;
    }

    /** `Hud.extractRenderState` for one window frame while the HUD animates: Jade's box size follows the frame's delta, so the window records it as vanilla would draw it. Answers whether it still animates, then the recording if it differs from the last. */
    private static byte[] hudFrame(DataInputStream in) throws Exception {
        size(in);
        float partial = in.readFloat();
        Minecraft minecraft = Minecraft.getInstance();
        advance(minecraft, partial);
        timer(minecraft).advanceRealTime(net.minecraft.util.Util.getMillis());
        byte[] bytes = record(minecraft, timer(minecraft));
        ticksSinceFrame = 0;
        noted(bytes);
        boolean changed = !java.util.Arrays.equals(bytes, lastHud);
        if (changed) lastHud = bytes;
        var out = new ByteArrayOutputStream();
        var data = new DataOutputStream(out);
        data.writeBoolean(animating);
        data.writeBoolean(changed);
        if (changed) {
            data.writeInt(bytes.length);
            data.write(bytes);
        }
        return out.toByteArray();
    }

    /** The world's recording for one window frame while it changes, as `hudFrame` records the HUD: the window's camera and progress into the next tick. Answers whether it still animates, then the recording if it differs from the last. */
    private static byte[] worldFrame(DataInputStream in) throws Exception {
        size(in);
        float partial = in.readFloat();
        var cam = new WorldFrame.Cam(in.readDouble(), in.readDouble(), in.readDouble(), in.readFloat(), in.readFloat(), in.readFloat(), in.readFloat());
        Minecraft minecraft = Minecraft.getInstance();
        WorldFrame.camera(cam);
        advance(minecraft, partial);
        timer(minecraft).advanceRealTime(net.minecraft.util.Util.getMillis());
        long started = System.nanoTime();
        byte[] blob = WorldFrame.take(cam, partial, timer(minecraft), windowWidth, windowHeight, true);
        if (HUD_DEBUG) System.err.println("Horizon: world frame " + (System.nanoTime() - started) / 1000 + " us, " + (blob == null ? "unchanged" : blob.length + " bytes") + ", animating " + WorldFrame.animating);
        var out = new ByteArrayOutputStream();
        var data = new DataOutputStream(out);
        data.writeBoolean(WorldFrame.animating);
        data.writeBoolean(blob != null);
        if (blob != null) {
            data.writeInt(blob.length);
            data.write(blob);
        }
        return out.toByteArray();
    }

    /** The columns the window has given the Java client, for the sections a world event walks. */
    static it.unimi.dsi.fastutil.longs.LongList columns() {
        return new it.unimi.dsi.fastutil.longs.LongArrayList(CHUNKS);
    }

    /** `Hud.extractRenderState` once a tick, as the frame after it would see it: only the mods' layers draw (`HudLayersMixin`), then `RenderGuiEvent.Post`. Null when it equals what the window already has. */
    private static byte[] hud() {
        Minecraft minecraft = Minecraft.getInstance();
        try {
            timer(minecraft).advanceRealTime(net.minecraft.util.Util.getMillis());
            byte[] bytes = record(minecraft, timer(minecraft));
            if (HUD_DEBUG) {
                String seen = String.valueOf(minecraft.hitResult == null ? null : minecraft.hitResult.getType()) + " " + bytes.length;
                if (!seen.equals(lastHudDebug)) System.err.println("Horizon: HUD target " + seen + (minecraft.hitResult instanceof net.minecraft.world.phys.BlockHitResult b ? " " + b.getBlockPos() : ""));
                lastHudDebug = seen;
            }
            noted(bytes);
            // No frame recording for two ticks: the window isn't serving them, and the tick's own carry the HUD.
            if (animating && ++ticksSinceFrame > 2) animating = false;
            if (java.util.Arrays.equals(bytes, lastHud)) return null;
            lastHud = bytes;
            if (HUD_DEBUG && lookAt >= 0) {
                System.err.println("Horizon: HUD changed " + (windowTicks - lookAt) + " tick(s) after the window's look changed");
                lookAt = -1;
            }
            return bytes;
        } catch (Throwable t) {
            System.err.println("Horizon: a mod's HUD layer failed");
            t.printStackTrace();
            return null;
        }
    }

    private static int windowTicks;
    private static int lookAt = -1;
    private static float lookYaw = Float.NaN, lookPitch = Float.NaN;

    /** `MouseHandler.handleAccumulatedMovement`'s `turnPlayer`, per frame in vanilla: the window's look reaches the player before the camera and pick that precede the tick, with the previous rotation moved along so it doesn't interpolate. */
    private static void turnPlayer(Minecraft minecraft) {
        Motion motion = moved;
        LocalPlayer local = minecraft.player;
        if (motion == null || local == null) return;
        float dy = motion.yaw - local.getYRot();
        float dx = motion.pitch - local.getXRot();
        local.setYRot(motion.yaw);
        local.setXRot(motion.pitch);
        local.yRotO += dy;
        local.xRotO += dx;
        if (HUD_DEBUG && (motion.yaw != lookYaw || motion.pitch != lookPitch)) {
            lookYaw = motion.yaw;
            lookPitch = motion.pitch;
            lookAt = windowTicks;
        }
    }

    private record Motion(double x, double y, double z, float yaw, float pitch, double dx, double dy, double dz, boolean ground) {}

    /** The native movement's last result this client hasn't applied yet. */
    private static Motion moved;

    /** `LocalPlayer.move`: the window's movement code is the motion authority, so the player takes where it moved; its look came in `turnPlayer`. */
    public static void move(LocalPlayer local) {
        Motion motion = moved;
        if (motion == null) return;
        moved = null;
        local.setPos(motion.x, motion.y, motion.z);
        local.setDeltaMovement(motion.dx, motion.dy, motion.dz);
        local.setOnGround(motion.ground);
    }

    private static final byte KEY = 0;
    private static final byte KEY_STATE = 1;
    private static final byte CHAR = 2;
    private static final byte BUTTON = 3;
    private static final byte MOVE = 4;
    private static final byte SCROLL = 5;
    private static final byte SCREEN = 6;
    private static final byte FOCUS = 7;
    private static final byte DROP = 8;
    private static final byte SWAP_HANDS = 9;
    private static final byte HOTBAR = 10;
    private static final byte CHUNK = 11;
    private static final byte WIRE = 12;
    private static final byte CAMERA = 13;
    private static final byte ANIMATE = 14;
    private static final byte PARTICLES = 15;
    private static final byte SHELL_LAYOUT = 16;

    private static double cursorX, cursorY;
    private static BlockPos cameraBlock;

    /** `SDLEventHandler`'s `KeyEvent`: the scancode, then the SDL keycode that `shortcutKey` reads. */
    static KeyEvent keyEvent(String name, int modifiers, int keycode) {
        return new KeyEvent(com.mojang.blaze3d.platform.InputConstants.getKey(name).getValue(), keycode, modifiers);
    }

    /** The window's input in its order, each through the callback the window's library gives a real client: `KeyboardHandler.keyPress` and `charTyped`, `MouseHandler.onButton`, `onMove` and `onScroll`. */
    private static void events(DataInputStream in) throws Exception {
        Minecraft minecraft = Minecraft.getInstance();
        long window = minecraft.getWindow().handle();
        for (int n = in.readInt(); n > 0; n--) {
            switch (in.readByte()) {
                case KEY -> {
                    String name = text(in);
                    int action = in.readInt();
                    int modifiers = in.readInt();
                    KeyEvent event = keyEvent(name, modifiers, in.readInt());
                    held(event.key(), action != 0);
                    minecraft.keyboardHandler.keyPress(window, action, event);
                }
                case KEY_STATE -> held(com.mojang.blaze3d.platform.InputConstants.getKey(text(in)).getValue(), in.readBoolean());
                case CHAR -> minecraft.keyboardHandler.charTyped(window, new CharacterEvent(in.readInt()));
                case BUTTON -> {
                    int button = in.readInt();
                    int action = in.readInt();
                    minecraft.mouseHandler.onButton(window, new MouseButtonInfo(button, in.readInt()), action);
                }
                case MOVE -> {
                    double x = in.readDouble(), y = in.readDouble();
                    minecraft.mouseHandler.onMove(window, x, y, x - cursorX, y - cursorY);
                    cursorX = x;
                    cursorY = y;
                }
                case SCROLL -> {
                    double dx = in.readDouble(), dy = in.readDouble();
                    minecraft.mouseHandler.onScroll(window, dx, dy);
                }
                case SCREEN -> nativeScreen(in.readBoolean(), in.readBoolean(), in.readByte());
                case SHELL_LAYOUT -> Shells.layout(new int[] {in.readInt(), in.readInt(), in.readInt(), in.readInt(), in.readInt()});
                case FOCUS -> set(Window.class, minecraft.getWindow(), "focused", in.readBoolean());
                case DROP -> {
                    boolean all = in.readBoolean();
                    if (minecraft.player != null && !minecraft.player.isSpectator()) minecraft.gameMode.dropItem(minecraft.player, all);
                }
                case SWAP_HANDS -> {
                    if (minecraft.player != null && !minecraft.player.isSpectator()) minecraft.getConnection().send(new net.minecraft.network.protocol.game.ServerboundPlayerActionPacket(
                            net.minecraft.network.protocol.game.ServerboundPlayerActionPacket.Action.SWAP_ITEM_WITH_OFFHAND, BlockPos.ZERO, net.minecraft.core.Direction.DOWN));
                }
                case ANIMATE -> Animate.group(in);
                case PARTICLES -> minecraft.options.particles().set(net.minecraft.server.level.ParticleStatus.values()[Math.max(0, Math.min(2, in.readInt()))]);
                case CAMERA -> {
                    var cam = new WorldFrame.Cam(in.readDouble(), in.readDouble(), in.readDouble(), in.readFloat(), in.readFloat(), in.readFloat(), in.readFloat());
                    WorldFrame.camera(cam);
                    int type = Math.max(0, Math.min(2, in.readByte()));
                    if (type != minecraft.options.getCameraType().ordinal()) minecraft.options.setCameraType(net.minecraft.client.CameraType.values()[type]);
                    var block = BlockPos.containing(cam.x(), cam.y(), cam.z());
                    if (!block.equals(cameraBlock)) {
                        cameraBlock = block;
                        BlockRenders.camera(cam.x(), cam.y(), cam.z());
                    }
                }
                case WIRE -> RemoteClient.inbound(in.readNBytes(in.readInt()));
                case CHUNK -> CHUNK_EVENTS.add(new int[] {in.readInt(), in.readInt(), in.readBoolean() ? 1 : 0});
                case HOTBAR -> {
                    int slot = in.readInt();
                    if (minecraft.player != null) minecraft.player.getInventory().setSelectedSlot(slot);
                }
                default -> throw new IllegalStateException("input event");
            }
        }
    }

    private static final java.util.ArrayList<int[]> CHUNK_EVENTS = new java.util.ArrayList<>();
    private static final java.util.ArrayList<int[]> TOLD = new java.util.ArrayList<>();

    /** The window's chunk arrivals and drops, applied after the packets of the tick so a level they belong to exists, as chunk packets follow the login. */
    private static void chunkEvents() {
        if (Minecraft.getInstance().level == null) return;
        for (int[] event : CHUNK_EVENTS) {
            if (event[2] == 1) {
                chunkArrived(event[0], event[1]);
                TOLD.add(new int[] {event[0], event[1]});
            } else chunkLeft(event[0], event[1]);
        }
        CHUNK_EVENTS.clear();
    }

    /** `ClientChunkCache.replaceWithPacketData`'s part after the data: the column's entities tick, and `ChunkEvent.Load` fires. */
    private static void chunkArrived(int x, int z) {
        ClientLevel level = Minecraft.getInstance().level;
        if (level == null) return;
        CHUNKS.add(net.minecraft.world.level.ChunkPos.pack(x, z));
        // The chunk's block entities come with it, for the blocks the server has told this client of.
        for (long at : STATES.keySet().toLongArray()) {
            var pos = BlockPos.of(at);
            if ((pos.getX() >> 4) == x && (pos.getZ() >> 4) == z) blockEntity(level, pos);
        }
        var pos = new net.minecraft.world.level.ChunkPos(x, z);
        var held = HELD.remove(net.minecraft.world.level.ChunkPos.pack(x, z));
        if (held != null) {
            for (var update : held) {
                update.handle(listener);
                shapes++;
                RESHAPED.add(update.getPos().asLong());
                ClientModels.dirty(update.getPos());
                BlockRenders.dirty(update.getPos());
            }
        }
        level.onChunkLoaded(pos);
        var chunk = ChunkViews.get(level, x, z);
        if (chunk != null) net.neoforged.neoforge.common.NeoForge.EVENT_BUS.post(new net.neoforged.neoforge.event.level.ChunkEvent.Load(chunk, false));
    }

    /** `ClientChunkCache.drop`: `ChunkEvent.Unload`, then `ClientLevel.unload`, and the column's block entity copies go with it. */
    private static void chunkLeft(int x, int z) {
        ClientLevel level = Minecraft.getInstance().level;
        if (level == null) return;
        var chunk = ChunkViews.get(level, x, z);
        if (chunk != null) {
            net.neoforged.neoforge.common.NeoForge.EVENT_BUS.post(new net.neoforged.neoforge.event.level.ChunkEvent.Unload(chunk));
            level.unload(chunk);
        }
        CHUNKS.remove(net.minecraft.world.level.ChunkPos.pack(x, z));
        HELD.remove(net.minecraft.world.level.ChunkPos.pack(x, z));
        var pos = new net.minecraft.world.level.ChunkPos(x, z);
        BLOCK_ENTITIES.entrySet().removeIf(entry -> {
            if (!net.minecraft.world.level.ChunkPos.containing(BlockPos.of(entry.getKey())).equals(pos)) return false;
            entry.getValue().setRemoved();
            TICKERS.remove(entry.getKey());
            return true;
        });
        STATES.keySet().removeIf(at -> net.minecraft.world.level.ChunkPos.containing(BlockPos.of(at)).equals(pos));
        ChunkViews.forget(level, x, z);
    }

    /** What the keyboard's state reports for `InputConstants.isKeyDown`, whichever screen took the key. */
    private static void held(int key, boolean down) {
        if (down) KEYS_DOWN.add(key);
        else KEYS_DOWN.remove(key);
    }

    /** What the Java client holds while one of the window's own screens is open: `Minecraft.screen` is not null, so vanilla's routing keeps `KeyMapping`s quiet as it does under chat. */
    private static final class NativeScreen extends Screen {
        private final boolean pauses;

        NativeScreen(boolean pauses) {
            super(net.minecraft.network.chat.Component.empty());
            this.pauses = pauses;
        }

        @Override
        public boolean isPauseScreen() {
            return pauses;
        }

        @Override
        public boolean shouldCloseOnEsc() {
            return false;
        }
    }

    /** The screen a Java mod could see: the window's own are stand-ins, not its screens. */
    private static Screen javaScreen() {
        Screen screen = Minecraft.getInstance().gui.screen();
        return screen instanceof NativeScreen || Shells.is(screen) ? null : screen;
    }

    /** A screen of the window's own opened over the world or closed, as `Gui.setScreen` does for one of vanilla's. */
    private static void nativeScreen(boolean open, boolean pauses, byte shell) throws ReflectiveOperationException {
        Minecraft minecraft = Minecraft.getInstance();
        if (open) {
            if (shell != 0) Shells.open(shell, minecraft);
            // A shell a mod's `Opening` listener replaced leaves that screen alone.
            boolean replaced = shell != 0 && minecraft.gui.screen() != null && !(minecraft.gui.screen() instanceof NativeScreen);
            if (Shells.current == null && !replaced) minecraft.gui.setScreen(new NativeScreen(pauses));
        } else {
            Shells.close(minecraft);
            if (minecraft.gui.screen() instanceof NativeScreen) minecraft.gui.setScreen(null);
        }
        // `runTick` sets it from the open screen: the window's pause screen pauses this client.
        set(Minecraft.class, minecraft, "pause", open && pauses);
    }

    private static boolean clicking;

    static boolean clicking() {
        return clicking;
    }
    private static int clickCanceled;
    private static int blockUse;

    /** `MultiPlayerGameMode.useItemOn`'s answer for the main hand during a click: 0 none, 1 passed, 2 success, 3 consumed. */
    public static void blockUsed(net.minecraft.world.InteractionHand hand, net.minecraft.world.InteractionResult result) {
        if (!clicking || hand != net.minecraft.world.InteractionHand.MAIN_HAND || blockUse != 0) return;
        blockUse = result instanceof net.minecraft.world.InteractionResult.Success ? 2 : result.consumesAction() ? 3 : 1;
    }

    /** The first canceled `InteractionKeyMappingTriggered` of a click: 1, or 2 and 3 for a main or off hand to swing. */
    private static void clicked(net.neoforged.neoforge.client.event.InputEvent.InteractionKeyMappingTriggered event) {
        if (!clicking || clickCanceled != 0 || !event.isCanceled()) return;
        clickCanceled = !event.shouldSwingHand() ? 1 : event.getHand() == net.minecraft.world.InteractionHand.MAIN_HAND ? 2 : 3;
    }

    /** `startAttack`, `startUseItem` or `pickBlockOrEntity` for a click the window is about to act on, with the window's hit as `hitResult`: answers whether a mod canceled it, and which hand swings then. */
    private static byte[] click(DataInputStream in) throws Exception {
        int kind = in.readInt();
        Minecraft minecraft = Minecraft.getInstance();
        if (player == null || minecraft.player == null || minecraft.level == null) return new byte[] {4}; // not run: the window acts alone
        net.minecraft.world.phys.HitResult hit;
        if (in.readBoolean()) {
            var pos = BlockPos.of(in.readLong());
            var face = net.minecraft.core.Direction.from3DDataValue(in.readInt());
            hit = new net.minecraft.world.phys.BlockHitResult(new net.minecraft.world.phys.Vec3(in.readDouble(), in.readDouble(), in.readDouble()), face, pos, false);
        } else {
            hit = net.minecraft.world.phys.BlockHitResult.miss(minecraft.player.getEyePosition(), net.minecraft.core.Direction.getApproximateNearest(minecraft.player.getLookAngle()), minecraft.player.blockPosition());
        }
        var previous = minecraft.hitResult;
        var actions = (dev.horizonlab.javamods.boot.mixin.MinecraftActions) minecraft;
        minecraft.hitResult = hit;
        minecraft.crosshairPickEntity = null;
        clicking = true;
        clickCanceled = 0;
        blockUse = 0;
        try {
            switch (kind) {
                case 0 -> actions.horizon$startAttack();
                case 1 -> actions.horizon$startUseItem();
                default -> actions.horizon$pickBlockOrEntity();
            }
        } finally {
            clicking = false;
            minecraft.hitResult = previous;
        }
        return new byte[] {(byte) clickCanceled, (byte) blockUse};
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

    private static byte[] status(byte[] hud, byte[] world) throws IOException {
        Screen screen = javaScreen();
        if (screen != null && screen != lastScreen) opened++;
        lastScreen = screen;
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeBoolean(screen != null);
        out.writeInt(opened);
        out.writeBoolean(screen != null && screen.isPauseScreen());
        // The counts lead so the window reads them without parsing the sound events after.
        out.writeInt(ClientModels.pending());
        out.writeInt(BlockRenders.pending());
        out.writeInt(TOLD.size());
        for (int[] column : TOLD) {
            out.writeInt(column[0]);
            out.writeInt(column[1]);
        }
        TOLD.clear();
        ClientSounds.take(out);
        ClientParticles.take(out);
        out.writeInt(shapes);
        out.writeBoolean(hud != null);
        if (hud != null) {
            out.writeInt(hud.length);
            out.write(hud);
        }
        out.writeBoolean(animating);
        out.writeBoolean(world != null);
        if (world != null) {
            out.writeInt(world.length);
            out.write(world);
        }
        out.writeBoolean(WorldFrame.animating);
        out.writeBoolean(CreativeTabs.ready());
        out.writeInt(Shells.is(Minecraft.getInstance().gui.screen()) ? 1 : 0);
        Shells.write(out, shellSegments, shellChanged, true);
        out.writeBoolean(Shells.animating);
        shellSegments = null;
        shellChanged = false;
        return bytes.toByteArray();
    }

    private static byte[][] shellSegments;
    private static boolean shellChanged;

    /** The open shell's recording for the status: a mod that listens for it draws as vanilla's frame would, once a tick and after each input. */
    private static void recordShell(Minecraft minecraft, float partial) throws ReflectiveOperationException {
        shellSegments = null;
        shellChanged = false;
        if (!Shells.is(minecraft.gui.screen())) return;
        timer(minecraft).advanceRealTime(net.minecraft.util.Util.getMillis());
        if (minecraft.gui.screen().width != guiWidth || minecraft.gui.screen().height != guiHeight) minecraft.gui.screen().resize(guiWidth, guiHeight);
        var segments = Shells.record(minecraft, (int) minecraft.mouseHandler.getScaledXPos(minecraft.getWindow()), (int) minecraft.mouseHandler.getScaledYPos(minecraft.getWindow()), partial);
        if (segments == null) return;
        shellChanged = Shells.noteRecorded(segments, Shells.routed, timer(minecraft).getRealtimeDeltaTicks());
        shellSegments = segments;
    }

    /** The shell's recording for one window frame while it changes: the window's size, progress into the tick, cursor and layout. Replies `ShellReply`; 3 and no recording when the shell is not the screen. */
    private static byte[] shellFrame(DataInputStream in) throws Exception {
        size(in);
        float partial = in.readFloat();
        double x = in.readDouble(), y = in.readDouble();
        Shells.layout(new int[] {in.readInt(), in.readInt(), in.readInt(), in.readInt(), in.readInt()});
        Minecraft minecraft = Minecraft.getInstance();
        if (!Shells.is(minecraft.gui.screen())) return Shells.reply(Shells.NONE, null, false);
        receive();
        advance(minecraft, partial);
        minecraft.mouseHandler.onMove(minecraft.getWindow().handle(), x, y, x - cursorX, y - cursorY);
        cursorX = x;
        cursorY = y;
        minecraft.mouseHandler.handleAccumulatedMovement();
        recordShell(minecraft, partial);
        return Shells.reply(Shells.NONE, shellSegments, shellChanged);
    }

    /** Input over the shell, through the real handlers: the owner is who took it, then the recording the input changed. */
    private static byte[] shellInput(DataInputStream in) throws Exception {
        size(in);
        Shells.layout(new int[] {in.readInt(), in.readInt(), in.readInt(), in.readInt(), in.readInt()});
        Shells.nativeWidget = in.readBoolean();
        Shells.nativeHandles = in.readBoolean();
        Shells.owner = Shells.EVENT;
        events(in);
        receive();
        chunkEvents();
        Minecraft minecraft = Minecraft.getInstance();
        recordShell(minecraft, minecraft.getDeltaTracker().getGameTimeDeltaPartialTick(false));
        return Shells.reply(Shells.owner, shellSegments, shellChanged);
    }

    /** The open screen's drawing this frame, as Rust draws it. */
    private static byte[] frame(DataInputStream in) throws Exception {
        size(in);
        double mouseX = in.readDouble();
        double mouseY = in.readDouble();
        float partial = in.readFloat();
        receive();
        Minecraft minecraft = Minecraft.getInstance();
        // `runTick`, each frame: the mouse's moves since the last reach the open screen, with its drags.
        minecraft.mouseHandler.handleAccumulatedMovement();
        Screen screen = javaScreen();
        if (screen == null) return new byte[0];
        if (screen.width != guiWidth || screen.height != guiHeight) screen.resize(guiWidth, guiHeight);
        var recorder = new Recorder();
        var graphics = new GuiGraphicsExtractor(minecraft, recorder, (int) mouseX, (int) mouseY);
        net.neoforged.neoforge.client.ClientHooks.extractScreen(screen, ((dev.horizonlab.javamods.boot.mixin.GuiScreensAccessor) minecraft.gui).horizon$screenLayers(), graphics, (int) mouseX, (int) mouseY, partial);
        return recorder.bytes();
    }

    /** Input while a Java screen is open, handed over as it comes so the screen answers in step with the window. */
    private static byte[] input(DataInputStream in) throws Exception {
        size(in);
        events(in);
        receive();
        chunkEvents();
        recordShell(Minecraft.getInstance(), Minecraft.getInstance().getDeltaTracker().getGameTimeDeltaPartialTick(false));
        return status(null, null);
    }

    /** Dev: how many of the code-built block models the reload baked have parts. */
    private static byte[] modelsProbe() throws Exception {
        var models = ClientModels.dynamicModels();
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
    static void localServer(boolean local) throws ReflectiveOperationException {
        set(Minecraft.class, Minecraft.getInstance(), "isLocalServer", local);
    }

    static void leaveFor() throws ReflectiveOperationException {
        leave();
    }

    private static byte[] leave() throws ReflectiveOperationException {
        Minecraft minecraft = Minecraft.getInstance();
        RemoteClient.left();
        levelChanged();
        CHUNK_EVENTS.clear();
        TOLD.clear();
        // The integrated server's stop is Horizon's, which runs the server's own hooks: the client only leaves it, with the saving screen and then no screen, as the title screen is the window's.
        minecraft.disconnect(new net.minecraft.client.gui.screens.GenericMessageScreen(net.minecraft.client.gui.Gui.SAVING_LEVEL), true);
        minecraft.gui.setScreen(null);
        ClientModels.forget();
        BlockRenders.forget();
        WorldFrame.forget();
        cameraBlock = null;
        if (player != null) Clients.leave(player);
        player = null;
        listener = null;
        moved = null;
        lastScreen = null;
        lastHud = null;
        lastRecorded = null;
        animating = false;
        ClientSounds.forget();
        ClientParticles.forget();
        Animate.forget();
        Clients.registries = null;
        return new byte[] {1};
    }

    /** `MultiPlayerGameMode`'s `getDestroyProgress` on the client's player, for the window's break animation and prediction; empty before the player exists. */
    private static byte[] destroyProgress(DataInputStream in) throws IOException {
        var minecraft = Minecraft.getInstance();
        var level = minecraft.level;
        var player = minecraft.player;
        if (level == null || player == null) return new byte[0];
        var pos = BlockPos.of(in.readLong());
        var bytes = new ByteArrayOutputStream();
        new DataOutputStream(bytes).writeFloat(blockState(level, pos).getDestroyProgress(player, level, pos));
        return bytes.toByteArray();
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

    /** Horizon's font's advances, sent whole as the client starts, in the player's language. */
    private static byte[] font(DataInputStream in) throws Exception {
        ClientLoader.language(text(in));
        float[] table = new float[in.readInt()];
        for (int i = 0; i < table.length; i++) table[i] = in.readFloat();
        ClientLoader.advances(table);
        return new byte[] {1};
    }
}
