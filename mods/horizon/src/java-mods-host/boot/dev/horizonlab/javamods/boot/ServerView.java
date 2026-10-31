package dev.horizonlab.javamods.boot;

import com.mojang.serialization.Lifecycle;
import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import net.minecraft.core.LayeredRegistryAccess;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.RegistryLayer;
import net.minecraft.server.ReloadableServerResources;
import net.minecraft.server.dedicated.DedicatedPlayerList;
import net.minecraft.server.dedicated.DedicatedServer;
import net.minecraft.server.level.ChunkMap;
import net.minecraft.server.level.ServerChunkCache;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.server.packs.resources.CloseableResourceManager;
import net.minecraft.server.players.PlayerList;
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
        set(net.minecraft.world.entity.Entity.class, player, "onGround", onGround);
    }

    private static void set(Class<?> owner, Object target, String field, Object value) throws ReflectiveOperationException {
        Field f = owner.getDeclaredField(field);
        f.setAccessible(true);
        f.set(target, value);
    }

    private static Object get(Class<?> owner, Object target, String field) throws ReflectiveOperationException {
        Field f = owner.getDeclaredField(field);
        f.setAccessible(true);
        return f.get(target);
    }

    static MinecraftServer create(CloseableResourceManager manager, ReloadableServerResources resources, LayeredRegistryAccess<RegistryLayer> layers) throws ReflectiveOperationException {
        var loop = ReentrantBlockableEventLoop.class.getDeclaredConstructor(String.class, boolean.class);
        DedicatedServer server = GameBridge.construct(DedicatedServer.class, loop, "Horizon", false);
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
        PlayerList players = GameBridge.construct(DedicatedPlayerList.class, Object.class.getDeclaredConstructor());
        set(PlayerList.class, players, "server", server);
        set(PlayerList.class, players, "players", new ArrayList<ServerPlayer>());
        set(PlayerList.class, players, "playersByUUID", new HashMap<UUID, ServerPlayer>());
        set(MinecraftServer.class, server, "playerList", players);
        return server;
    }

    @SuppressWarnings("unchecked")
    static void attach(MinecraftServer server, ServerLevel level) throws ReflectiveOperationException {
        set(ServerLevel.class, level, "server", server);
        set(ServerLevel.class, level, "players", new ArrayList<ServerPlayer>());
        set(ServerLevel.class, level, "dragonParts", new it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap<>());
        set(ServerLevel.class, level, "serverLevelData", server.getWorldData().overworldData());
        var chunks = GameBridge.construct(ServerChunkCache.class, Object.class.getDeclaredConstructor());
        set(ServerChunkCache.class, chunks, "chunkMap", GameBridge.construct(ChunkMap.class, Object.class.getDeclaredConstructor()));
        set(ServerLevel.class, level, "chunkSource", chunks);
        set(ServerLevel.class, level, "environmentAttributes", EnvironmentAttributeSystem.builder().addDefaultLayers(level).build());
        ((Map<ResourceKey<Level>, ServerLevel>) get(MinecraftServer.class, server, "levels")).put(level.dimension(), level);
    }

    /** A player's server and a connection whose packets go nowhere yet; Horizon tells clients what it knows itself. */
    @SuppressWarnings("unchecked")
    static void attach(MinecraftServer server, ServerPlayer player) throws ReflectiveOperationException {
        set(ServerPlayer.class, player, "server", server);
        ((List<ServerPlayer>) get(ServerLevel.class, player.level(), "players")).add(player);
        var connection = GameBridge.construct(ServerGamePacketListenerImpl.class, Object.class.getDeclaredConstructor());
        set(ServerGamePacketListenerImpl.class, connection, "player", player);
        player.connection = connection;
        PlayerList list = server.getPlayerList();
        ((List<ServerPlayer>) get(PlayerList.class, list, "players")).add(player);
        ((Map<UUID, ServerPlayer>) get(PlayerList.class, list, "playersByUUID")).put(player.getUUID(), player);
    }
}
