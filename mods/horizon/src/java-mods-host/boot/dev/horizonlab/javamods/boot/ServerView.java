package dev.horizonlab.javamods.boot;

import com.mojang.serialization.Lifecycle;
import net.neoforged.fml.loading.FMLEnvironment;
import io.netty.channel.embedded.EmbeddedChannel;
import com.mojang.authlib.GameProfileRepository;
import java.lang.reflect.Field;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import net.minecraft.core.LayeredRegistryAccess;
import net.minecraft.resources.ResourceKey;
import net.minecraft.network.Connection;
import net.minecraft.network.protocol.PacketFlow;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.RegistryLayer;
import net.minecraft.server.ReloadableServerResources;
import net.minecraft.server.ServerScoreboard;
import net.minecraft.server.ServerTickRateManager;
import net.minecraft.server.Services;
import net.minecraft.server.players.CachedUserNameToIdResolver;
import net.minecraft.world.level.levelgen.WorldDimensions;
import net.minecraft.world.level.levelgen.WorldGenSettings;
import net.minecraft.world.level.levelgen.WorldOptions;
import net.minecraft.world.level.storage.SavedDataStorage;
import com.mojang.authlib.services.ServicesKeySet;
import net.minecraft.server.level.ChunkMap;
import net.minecraft.server.level.ServerChunkCache;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.ServerCommonPacketListenerImpl;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.server.packs.resources.CloseableResourceManager;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.players.IpBanList;
import net.minecraft.server.players.PlayerList;
import net.minecraft.server.players.ServerOpList;
import net.minecraft.server.players.UserBanList;
import net.minecraft.server.players.UserWhiteList;
import net.minecraft.util.thread.BlockableEventLoop;
import net.minecraft.util.thread.ReentrantBlockableEventLoop;
import net.minecraft.world.attribute.EnvironmentAttributeSystem;
import net.minecraft.world.clock.PackedClockStates;
import net.minecraft.world.clock.ServerClockManager;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.level.storage.PrimaryLevelData;

/** The real server classes, built without a world: real data packs behind them, and the levels and players as views. */
final class ServerView {
    private ServerView() {}

    static void setHealth(ServerPlayer player, float health) throws ReflectiveOperationException {
        var data = player.getEntityData();
        var id = (net.minecraft.network.syncher.EntityDataAccessor<Float>) get(net.minecraft.world.entity.LivingEntity.class, null, "DATA_HEALTH_ID");
        data.set(id, health);
    }

    static void setFood(ServerPlayer player, int food, float saturation) throws ReflectiveOperationException {
        var data = player.getFoodData();
        set(net.minecraft.world.food.FoodData.class, data, "foodLevel", food);
        set(net.minecraft.world.food.FoodData.class, data, "saturationLevel", saturation);
    }

    static void setMotion(ServerPlayer player, net.minecraft.world.phys.Vec3 motion, boolean onGround) throws ReflectiveOperationException {
        set(net.minecraft.world.entity.Entity.class, player, "deltaMovement", motion);
        player.setKnownMovement(motion);
        set(net.minecraft.world.entity.Entity.class, player, "onGround", onGround);
    }

    static void set(Class<?> owner, Object target, String field, Object value) throws ReflectiveOperationException {
        Field f = owner.getDeclaredField(field);
        f.setAccessible(true);
        f.set(target, value);
    }

    private static Object get(Class<?> owner, Object target, String field) throws ReflectiveOperationException {
        Field f = owner.getDeclaredField(field);
        f.setAccessible(true);
        return f.get(target);
    }

    /** By name, as a class of the other side doesn't exist on this one. */
    private static <T> Class<? extends T> viewClass(String name, Class<T> base) throws ClassNotFoundException {
        return Class.forName(name, true, ServerView.class.getClassLoader()).asSubclass(base);
    }

    static MinecraftServer create(CloseableResourceManager manager, ReloadableServerResources resources, LayeredRegistryAccess<RegistryLayer> layers) throws ReflectiveOperationException {
        var loop = ReentrantBlockableEventLoop.class.getDeclaredConstructor(String.class, boolean.class);
        boolean client = FMLEnvironment.getDist().isClient();
        MinecraftServer server = GameBridge.construct(viewClass(client ? "net.minecraft.client.server.IntegratedServer" : "net.minecraft.server.dedicated.DedicatedServer", MinecraftServer.class), loop, "Horizon", false);
        set(MinecraftServer.class, server, "registries", layers);
        set(MinecraftServer.class, server, "levels", new java.util.LinkedHashMap<ResourceKey<Level>, ServerLevel>());
        Class<?> holder = Class.forName("net.minecraft.server.MinecraftServer$ReloadableResources");
        var record = holder.getDeclaredConstructor(CloseableResourceManager.class, ReloadableServerResources.class);
        record.setAccessible(true);
        set(MinecraftServer.class, server, "resources", record.newInstance(manager, resources));
        set(MinecraftServer.class, server, "serverThread", Thread.currentThread());
        var settings = new LevelSettings("Horizon", GameType.SURVIVAL, LevelSettings.DifficultySettings.DEFAULT, true, WorldDataConfiguration.DEFAULT);
        var data = new PrimaryLevelData(settings, PrimaryLevelData.SpecialWorldProperty.NONE, Lifecycle.stable());
        set(MinecraftServer.class, server, "worldData", data);
        var clocks = ServerClockManager.class.getDeclaredConstructor(PackedClockStates.class);
        clocks.setAccessible(true);
        var clockManager = clocks.newInstance(PackedClockStates.EMPTY);
        set(MinecraftServer.class, server, "clockManager", clockManager);
        clockManager.init(server);
        set(MinecraftServer.class, server, "gameRules", new GameRules(WorldDataConfiguration.DEFAULT.enabledFeatures()));
        // `MinecraftServer`'s field initializer: `Entity.getTeam` (a wither's constructor names itself) reads it.
        set(MinecraftServer.class, server, "scoreboard", new ServerScoreboard(server));
        PlayerList players = GameBridge.construct(viewClass(client ? "net.minecraft.client.server.IntegratedPlayerList" : "net.minecraft.server.dedicated.DedicatedPlayerList", PlayerList.class), Object.class.getDeclaredConstructor());
        set(PlayerList.class, players, "server", server);
        var playing = new ArrayList<ServerPlayer>();
        set(PlayerList.class, players, "players", playing);
        set(PlayerList.class, players, "playersView", java.util.Collections.unmodifiableList(playing));
        set(PlayerList.class, players, "playersByUUID", new HashMap<UUID, ServerPlayer>());
        set(PlayerList.class, players, "stats", new HashMap<>());
        set(PlayerList.class, players, "advancements", new HashMap<>());
        set(MinecraftServer.class, server, "playerList", players);
        // The dedicated server's limits (players, distances) are its properties file's, which the game's own server reads from the same folder.
        if (!client) set(server.getClass(), server, "settings", viewClass("net.minecraft.server.dedicated.DedicatedServerSettings", Object.class).getConstructor(Path.class).newInstance(Path.of("server.properties")));
        set(MinecraftServer.class, server, "tickRateManager", new ServerTickRateManager(server));
        if (players.getPlayers() == null) throw new IllegalStateException("the player list has no players");
        if (client) ClientLoader.attach(server);
        Inits.run(server, layers, manager);
        Inits.run(players, server, layers);
        return server;
    }

    @SuppressWarnings("unchecked")
    static void attach(MinecraftServer server, ServerLevel level) throws ReflectiveOperationException {
        set(ServerLevel.class, level, "server", server);
        set(ServerLevel.class, level, "players", new ArrayList<ServerPlayer>());
        set(ServerLevel.class, level, "dragonParts", new it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap<>());
        set(ServerLevel.class, level, "capListenerHolder", new net.neoforged.neoforge.capabilities.CapabilityListenerHolder());
        set(ServerLevel.class, level, "serverLevelData", server.getWorldData().overworldData());
        var chunks = GameBridge.construct(ServerChunkCache.class, Object.class.getDeclaredConstructor());
        set(ServerChunkCache.class, chunks, "chunkMap", GameBridge.construct(ChunkMap.class, Object.class.getDeclaredConstructor()));
        set(ServerChunkCache.class, chunks, "level", level);
        set(ServerLevel.class, level, "chunkSource", chunks);
        set(ChunkMap.class, chunks.chunkMap, "entityMap", new it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap<>());
        set(ChunkMap.class, chunks.chunkMap, "playerMap", new net.minecraft.server.level.PlayerMap());
        set(ChunkMap.class, chunks.chunkMap, "level", level);
        Inits.run(chunks.chunkMap, level);
        Inits.run(chunks, level);
        set(ServerLevel.class, level, "environmentAttributes", EnvironmentAttributeSystem.builder().addDefaultLayers(level).build());
        ((Map<ResourceKey<Level>, ServerLevel>) get(MinecraftServer.class, server, "levels")).put(level.dimension(), level);
    }

    /** Horizon resolves names itself; a mod asking this cache for a stranger's name gets nothing rather than a network call. */
    private static final class OfflineNames implements GameProfileRepository {
        @Override
        public void findProfilesByNames(String[] names, com.mojang.authlib.ProfileLookupCallback callback) {
            for (String name : names) callback.onProfileLookupFailed(name, new com.mojang.authlib.exceptions.AuthenticationUnavailableException("offline"));
        }

        @Override
        public java.util.Optional<com.mojang.authlib.services.response.NameAndId> findProfileByName(String name) {
            return java.util.Optional.empty();
        }
    }

    /** Vanilla's `ProfileResolver.Cached` without the session service: the player list, then the name cache. */
    private static final class OfflineProfiles implements net.minecraft.server.players.ProfileResolver {
        private final MinecraftServer server;
        private final net.minecraft.server.players.UserNameToIdResolver cache;

        OfflineProfiles(MinecraftServer server, net.minecraft.server.players.UserNameToIdResolver cache) {
            this.server = server;
            this.cache = cache;
        }

        @Override
        public java.util.Optional<com.mojang.authlib.GameProfile> fetchByName(String name) {
            if (!net.minecraft.util.StringUtil.isValidPlayerName(name)) return java.util.Optional.empty();
            var player = server.getPlayerList().getPlayerByName(name);
            if (player != null) return java.util.Optional.of(player.getGameProfile());
            return cache.get(name).map(known -> new com.mojang.authlib.GameProfile(known.id(), known.name()));
        }

        @Override
        public java.util.Optional<com.mojang.authlib.GameProfile> fetchById(java.util.UUID id) {
            var player = server.getPlayerList().getPlayer(id);
            if (player != null) return java.util.Optional.of(player.getGameProfile());
            return cache.get(id).map(known -> new com.mojang.authlib.GameProfile(known.id(), known.name()));
        }
    }

    /** What a server reads from its world folder, once Horizon has opened one: mods' saved data lands beside vanilla's. */
    static void openWorld(MinecraftServer server, Iterable<ServerLevel> levels, Path folder, long seed, WorldDimensions dimensions) throws ReflectiveOperationException {
        set(MinecraftServer.class, server, "serverThread", Thread.currentThread());
        set(MinecraftServer.class, server, "packetProcessor", new net.minecraft.network.PacketProcessor(Thread.currentThread()));
        var registries = server.registryAccess();
        var fixer = net.minecraft.util.datafix.DataFixers.getDataFixer();
        set(MinecraftServer.class, server, "fixerUpper", fixer);
        ServerLevel overworld = null;
        for (ServerLevel level : levels) if (level.dimension() == Level.OVERWORLD) overworld = level;
        // NeoForge's storages know their level, as mods' saved data reads it; the server's is the overworld's.
        set(MinecraftServer.class, server, "savedDataStorage", new SavedDataStorage(overworld, folder.resolve("data"), fixer, registries));
        set(MinecraftServer.class, server, "worldGenSettings", new WorldGenSettings(new WorldOptions(seed, true, false), dimensions));
        for (ServerLevel level : levels) {
            // `MinecraftServer.createLevels`: every level zooms biomes with the world seed's hash.
            set(Level.class, level, "biomeManager", new net.minecraft.world.level.biome.BiomeManager(level, net.minecraft.world.level.biome.BiomeManager.obfuscateSeed(seed)));
            var id = level.dimension().identifier();
            var dataFolder = folder.resolve("dimensions").resolve(id.getNamespace()).resolve(id.getPath()).resolve("data");
            set(ServerChunkCache.class, level.getChunkSource(), "savedDataStorage", new SavedDataStorage(level, dataFolder, fixer, registries));
        }
        GameProfileRepository names = new OfflineNames();
        var cache = new CachedUserNameToIdResolver(names, folder.resolve("usercache.json").toFile());
        set(MinecraftServer.class, server, "services", new Services(null, ServicesKeySet.EMPTY, names, cache, new OfflineProfiles(server, cache)));
        // Empty in Java: Horizon's own lists decide who is banned and who is an operator.
        var notices = new EmptyNotificationService();
        PlayerList players = server.getPlayerList();
        set(PlayerList.class, players, "ops", new ServerOpList(folder.resolve("horizon_java_ops.json").toFile(), notices));
        set(PlayerList.class, players, "whitelist", new UserWhiteList(folder.resolve("horizon_java_whitelist.json").toFile(), notices));
        set(PlayerList.class, players, "bans", new UserBanList(folder.resolve("horizon_java_bans.json").toFile(), notices));
        set(PlayerList.class, players, "ipBans", new IpBanList(folder.resolve("horizon_java_ip_bans.json").toFile(), notices));
    }

    @SuppressWarnings("unchecked")
    static void closeWorld(MinecraftServer server) {
        try {
            ((Map<ResourceKey<Level>, ServerLevel>) get(MinecraftServer.class, server, "levels")).clear();
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    static void saveWorld(MinecraftServer server, Iterable<ServerLevel> levels) {
        if (get0(server) != null) server.getDataStorage().saveAndJoin();
        for (ServerLevel level : levels) {
            if (level.getChunkSource().getDataStorage() != null) level.getChunkSource().getDataStorage().saveAndJoin();
        }
    }

    private static Object get0(MinecraftServer server) {
        try {
            return get(MinecraftServer.class, server, "savedDataStorage");
        } catch (ReflectiveOperationException e) {
            return null;
        }
    }

    /** One server tick: its count, and the tasks mods queued with `server.execute`. */
    static void tick(MinecraftServer server) throws ReflectiveOperationException {
        var count = MinecraftServer.class.getDeclaredField("tickCount");
        count.setAccessible(true);
        count.setInt(server, count.getInt(server) + 1);
        var run = net.minecraft.util.thread.BlockableEventLoop.class.getDeclaredMethod("runAllTasks");
        run.setAccessible(true);
        run.invoke(server);
    }

    @SuppressWarnings("unchecked")
    static void detach(MinecraftServer server, ServerPlayer player) throws ReflectiveOperationException {
        ((List<ServerPlayer>) get(ServerLevel.class, player.level(), "players")).remove(player);
        // `ChunkMap.updatePlayerStatus(player, false)`: no longer among the players its chunks are tracked by.
        ((net.minecraft.server.level.PlayerMap) get(ChunkMap.class, player.level().getChunkSource().chunkMap, "playerMap")).removePlayer(player);
        PlayerList list = server.getPlayerList();
        ((List<ServerPlayer>) get(PlayerList.class, list, "players")).remove(player);
        ((Map<UUID, ServerPlayer>) get(PlayerList.class, list, "playersByUUID")).remove(player.getUUID());
    }

    /** `ChunkMap.updatePlayerStatus` on both levels of a player's change of level. */
    @SuppressWarnings("unchecked")
    static void moved(ServerPlayer player, ServerLevel to) throws ReflectiveOperationException {
        var from = player.level();
        ((List<ServerPlayer>) get(ServerLevel.class, from, "players")).remove(player);
        ((net.minecraft.server.level.PlayerMap) get(ChunkMap.class, from.getChunkSource().chunkMap, "playerMap")).removePlayer(player);
        player.setServerLevel(to);
        ((List<ServerPlayer>) get(ServerLevel.class, to, "players")).add(player);
        ((net.minecraft.server.level.PlayerMap) get(ChunkMap.class, to.getChunkSource().chunkMap, "playerMap")).addPlayer(player, false);
        Menus.changedLevel(player, to);
    }

    /** The instance of `outer`'s anonymous class implementing `type`, as its field initializer makes it. */
    static Object anonymous(Class<?> outer, Object instance, Class<?> type) throws ReflectiveOperationException {
        for (int n = 1; n < 64; n++) {
            Class<?> candidate;
            try {
                candidate = Class.forName(outer.getName() + "$" + n, false, outer.getClassLoader());
            } catch (ClassNotFoundException e) {
                continue;
            }
            if (!type.isAssignableFrom(candidate)) continue;
            var make = candidate.getDeclaredConstructor(outer);
            make.setAccessible(true);
            return make.newInstance(instance);
        }
        throw new NoSuchFieldException(outer.getName() + " has no anonymous " + type.getName());
    }

    /** A player's server and a connection whose packets go nowhere yet; Horizon tells clients what it knows itself. */
    @SuppressWarnings("unchecked")
    static void attach(MinecraftServer server, ServerPlayer player) throws ReflectiveOperationException {
        set(ServerPlayer.class, player, "server", server);
        ((List<ServerPlayer>) get(ServerLevel.class, player.level(), "players")).add(player);
        // `ChunkMap.updatePlayerStatus(player, true)`: the level's chunks are tracked by this player, as mods ask before sending.
        ((net.minecraft.server.level.PlayerMap) get(ChunkMap.class, player.level().getChunkSource().chunkMap, "playerMap")).addPlayer(player, false);
        var connection = GameBridge.construct(ServerGamePacketListenerImpl.class, Object.class.getDeclaredConstructor());
        set(ServerGamePacketListenerImpl.class, connection, "player", player);
        var network = new Connection(PacketFlow.SERVERBOUND);
        set(Connection.class, network, "channel", new EmbeddedChannel());
        set(ServerCommonPacketListenerImpl.class, connection, "connection", network);
        set(ServerCommonPacketListenerImpl.class, connection, "server", server);
        player.connection = connection;
        net.neoforged.neoforge.network.registration.NetworkRegistry.configureMockConnection(network);
        // Configuration syncs the configs a client gets; the Java client shares this JVM's, so none are owed.
        var owed = net.neoforged.neoforge.network.ConfigSync.class.getDeclaredField("configsToSync");
        owed.setAccessible(true);
        synchronized (get(net.neoforged.neoforge.network.ConfigSync.class, null, "lock")) {
            ((Map<Connection, Map<String, byte[]>>) owed.get(null)).put(network, new HashMap<>());
        }
        for (String field : List.of("containerSynchronizer", "containerListener", "commandSource")) {
            Field f = ServerPlayer.class.getDeclaredField(field);
            f.setAccessible(true);
            f.set(player, anonymous(ServerPlayer.class, player, f.getType()));
        }
        PlayerList list = server.getPlayerList();
        ((List<ServerPlayer>) get(PlayerList.class, list, "players")).add(player);
        ((Map<UUID, ServerPlayer>) get(PlayerList.class, list, "playersByUUID")).put(player.getUUID(), player);
    }
}
