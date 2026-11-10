package dev.horizonlab.javamods.boot;

import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import net.minecraft.network.PacketListener;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.configuration.ClientboundFinishConfigurationPacket;
import net.minecraft.network.protocol.configuration.ClientboundUpdateEnabledFeaturesPacket;
import net.minecraft.network.protocol.configuration.ServerboundSelectKnownPacks;
import net.minecraft.network.protocol.game.ClientboundChangeDifficultyPacket;
import net.minecraft.network.protocol.game.ClientboundGameEventPacket;
import net.minecraft.network.protocol.game.ClientboundInitializeBorderPacket;
import net.minecraft.network.protocol.game.ClientboundLoginPacket;
import net.minecraft.network.protocol.game.ClientboundPlayerAbilitiesPacket;
import net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket;
import net.minecraft.network.protocol.game.ClientboundPlayerPositionPacket;
import net.minecraft.network.protocol.game.ClientboundRespawnPacket;
import net.minecraft.network.protocol.game.ClientboundSetDefaultSpawnPositionPacket;
import net.minecraft.network.protocol.game.ClientboundSetHeldSlotPacket;
import net.minecraft.network.protocol.game.ClientboundUpdateMobEffectPacket;
import net.minecraft.network.protocol.game.ClientboundUpdateRecipesPacket;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.RunningOnDifferentThreadException;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.config.SynchronizeRegistriesTask;
import net.minecraft.world.entity.PositionMoveRotation;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.gamerules.GameRules;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.OnDatapackSyncEvent;
import net.neoforged.neoforge.network.payload.RecipeContentPayload;

/** A player's open Java menu on the server: what their client sent it, then `ServerPlayer.tick`'s part. */
final class Menus {
    private Menus() {}

    @SuppressWarnings("unchecked")
    static void serverTick(ServerPlayer player) {
        for (Packet<?> packet : Clients.takeForServer(player.getUUID())) {
            try {
                ((Packet<PacketListener>) packet).handle(player.connection);
            } catch (RunningOnDifferentThreadException e) {
                // Queued on the server instead; its next tick runs it.
            } catch (Throwable t) {
                System.err.println("Horizon: a Java client's " + packet.getClass().getSimpleName() + " failed on the server");
                t.printStackTrace();
            }
        }
        // `MinecraftServer.tickServer`: what payload handlers queued for the server thread runs now.
        player.level().getServer().packetProcessor().processQueuedPackets();
        player.containerMenu.broadcastChanges();
        if (!player.containerMenu.stillValid(player)) {
            player.closeContainer();
            player.containerMenu = player.inventoryMenu;
        }
    }

    /** `ServerConfigurationPacketListenerImpl.startConfiguration`'s packets for this server's registries: its features, then `SynchronizeRegistriesTask` asks which packs the client knows. */
    static List<Packet<?>> startConfiguration(MinecraftServer server) {
        var packets = new ArrayList<Packet<?>>();
        packets.add(new ClientboundUpdateEnabledFeaturesPacket(FeatureFlags.REGISTRY.toNames(server.getWorldData().enabledFeatures())));
        synchronizeRegistries(server).start(packets::add);
        return packets;
    }

    /** `ServerConfigurationPacketListenerImpl.handleSelectKnownPacks`: the registries and tags, then the end of configuration. */
    static List<Packet<?>> knownPacks(MinecraftServer server, ServerboundSelectKnownPacks packet) {
        var packets = new ArrayList<Packet<?>>();
        synchronizeRegistries(server).handleResponse(packet.knownPacks(), packets::add);
        packets.add(ClientboundFinishConfigurationPacket.INSTANCE);
        return packets;
    }

    private static SynchronizeRegistriesTask synchronizeRegistries(MinecraftServer server) {
        var known = server.getResourceManager().listPacks().flatMap(pack -> pack.location().knownPackInfo().stream()).toList();
        return new SynchronizeRegistriesTask(known, server.registries());
    }

    /** The configuration for a client in another process, which knows no packs: the features, then every registry and tag in full, and the configs NeoForge syncs. */
    static List<byte[]> remoteConfiguration(MinecraftServer server) {
        var packets = new ArrayList<Packet<?>>();
        packets.add(new ClientboundUpdateEnabledFeaturesPacket(FeatureFlags.REGISTRY.toNames(server.getWorldData().enabledFeatures())));
        synchronizeRegistries(server).handleResponse(List.of(), packets::add);
        syncConfigs(packets::add);
        packets.add(ClientboundFinishConfigurationPacket.INSTANCE);
        return packets.stream().map(Wire::configuration).toList();
    }

    /** NeoForge's own configuration task for a remote client: the server configs it syncs, sent through a listener that hands each packet on. */
    private static void syncConfigs(java.util.function.Consumer<Packet<?>> out) {
        var listener = (net.minecraft.network.protocol.configuration.ServerConfigurationPacketListener) java.lang.reflect.Proxy.newProxyInstance(
                Menus.class.getClassLoader(), new Class<?>[] {net.minecraft.network.protocol.configuration.ServerConfigurationPacketListener.class}, (proxy, method, args) -> {
                    switch (method.getName()) {
                        case "send" -> {
                            if (args[0] instanceof Packet<?> packet) out.accept(packet);
                            else out.accept(new net.minecraft.network.protocol.common.ClientboundCustomPayloadPacket((net.minecraft.network.protocol.common.custom.CustomPacketPayload) args[0]));
                            return null;
                        }
                        case "protocol" -> {
                            return net.minecraft.network.ConnectionProtocol.CONFIGURATION;
                        }
                        case "getConnectionType" -> {
                            return net.neoforged.neoforge.network.connection.ConnectionType.NEOFORGE;
                        }
                        case "hasChannel" -> {
                            return true;
                        }
                        case "getConnection" -> {
                            var network = new net.minecraft.network.Connection(net.minecraft.network.protocol.PacketFlow.SERVERBOUND);
                            ServerView.set(net.minecraft.network.Connection.class, network, "channel", new io.netty.channel.embedded.EmbeddedChannel());
                            return network;
                        }
                        default -> {
                            System.err.println("Horizon: NeoForge's config sync called " + method);
                            return method.getReturnType() == boolean.class ? (Object) false : null;
                        }
                    }
                });
        net.neoforged.neoforge.network.ConfigSync.syncAllConfigs(listener);
    }

    /** `PlayerList.placeNewPlayer`'s packets, in its order, without what stays Horizon's: the scoreboard, the join message, boss bars and entity pairing. */
    static void login(ServerPlayer player) {
        var server = player.level().getServer();
        var level = player.level();
        var data = level.getLevelData();
        var rules = level.getGameRules();
        var players = server.getPlayerList();
        var connection = player.connection;
        connection.send(new ClientboundLoginPacket(
                player.getId(),
                data.isHardcore(),
                server.levelKeys(),
                players.getMaxPlayers(),
                players.getViewDistance(),
                players.getSimulationDistance(),
                rules.get(GameRules.REDUCED_DEBUG_INFO),
                !rules.get(GameRules.IMMEDIATE_RESPAWN),
                rules.get(GameRules.LIMITED_CRAFTING),
                player.createCommonSpawnInfo(level),
                server.usesAuthentication(),
                server.enforceSecureProfile()));
        connection.send(new ClientboundChangeDifficultyPacket(data.getDifficulty(), data.isDifficultyLocked()));
        connection.send(new ClientboundPlayerAbilitiesPacket(player.getAbilities()));
        connection.send(new ClientboundSetHeldSlotPacket(player.getInventory().getSelectedSlot()));
        var recipes = server.getRecipeManager();
        connection.send(new ClientboundUpdateRecipesPacket(recipes.getSynchronizedItemProperties(), recipes.getSynchronizedStonecutterRecipes()));
        // NeoForge: the recipe types mods asked to sync follow vanilla's synced parts.
        var sync = NeoForge.EVENT_BUS.post(new OnDatapackSyncEvent(players, player));
        connection.send(RecipeContentPayload.create(sync.getRecipeTypesToSend(), recipes.recipeMap()));
        players.sendPlayerPermissionLevel(player);
        connection.send(new ClientboundPlayerPositionPacket(0, new PositionMoveRotation(player.position(), player.getDeltaMovement(), player.getYRot(), player.getXRot()), Set.of()));
        connection.send(ClientboundPlayerInfoUpdatePacket.createPlayerInitializing(players.getPlayers()));
        levelInfo(player, level);
        for (var effect : player.getActiveEffects()) connection.send(new ClientboundUpdateMobEffectPacket(player.getId(), effect, false));
        PlayerSync.sendAllAttributes(player);
        player.initInventoryMenu();
        for (var entity : new ArrayList<>(BlockEntities.held(level).values())) Clients.joined(level, entity);
    }

    /** `PlayerList.sendLevelInfo`. */
    private static void levelInfo(ServerPlayer player, ServerLevel level) {
        var connection = player.connection;
        var server = level.getServer();
        connection.send(new ClientboundInitializeBorderPacket(level.getWorldBorder()));
        connection.send(server.clockManager().createFullSyncPacket());
        connection.send(new ClientboundSetDefaultSpawnPositionPacket(level.getRespawnData()));
        if (level.isRaining()) {
            connection.send(new ClientboundGameEventPacket(ClientboundGameEventPacket.START_RAINING, 0.0F));
            connection.send(new ClientboundGameEventPacket(ClientboundGameEventPacket.RAIN_LEVEL_CHANGE, level.getRainLevel(1.0F)));
            connection.send(new ClientboundGameEventPacket(ClientboundGameEventPacket.THUNDER_LEVEL_CHANGE, level.getThunderLevel(1.0F)));
        }
        connection.send(new ClientboundGameEventPacket(ClientboundGameEventPacket.LEVEL_CHUNKS_LOAD_START, 0.0F));
        server.tickRateManager().updateJoiningPlayer(player);
    }

    /** `ServerPlayer.teleport`'s packets to a new level: its client builds that level and a new player from them. */
    static void changedLevel(ServerPlayer player, ServerLevel to) {
        var connection = player.connection;
        var data = to.getLevelData();
        connection.send(new ClientboundRespawnPacket(player.createCommonSpawnInfo(to), (byte) 3));
        connection.send(new ClientboundChangeDifficultyPacket(data.getDifficulty(), data.isDifficultyLocked()));
        to.getServer().getPlayerList().sendPlayerPermissionLevel(player);
        connection.send(new ClientboundPlayerPositionPacket(0, new PositionMoveRotation(player.position(), player.getDeltaMovement(), player.getYRot(), player.getXRot()), Set.of()));
        levelInfo(player, to);
        connection.send(new ClientboundPlayerAbilitiesPacket(player.getAbilities()));
        connection.send(new ClientboundSetHeldSlotPacket(player.getInventory().getSelectedSlot()));
        for (var effect : player.getActiveEffects()) connection.send(new ClientboundUpdateMobEffectPacket(player.getId(), effect, false));
        PlayerSync.sendAllAttributes(player);
        for (var entity : new ArrayList<>(BlockEntities.held(to).values())) Clients.joined(to, entity);
    }

    /** `ServerPlayer.setGameMode`'s packets, for a mode Horizon changed. */
    static void gameModeChanged(ServerPlayer player, net.minecraft.world.level.GameType type) {
        player.connection.send(new net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket(
                net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket.Action.UPDATE_GAME_MODE, player));
        player.connection.send(new net.minecraft.network.protocol.game.ClientboundGameEventPacket(
                net.minecraft.network.protocol.game.ClientboundGameEventPacket.CHANGE_GAME_MODE, type.getId()));
    }

    /** The player left: Java's menu closes as a disconnect closes it. */
    static void close(ServerPlayer player) {
        if (player.containerMenu != player.inventoryMenu) player.closeContainer();
    }
}
