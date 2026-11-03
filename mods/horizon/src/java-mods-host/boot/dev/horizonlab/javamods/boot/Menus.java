package dev.horizonlab.javamods.boot;

import net.minecraft.network.PacketListener;
import net.minecraft.network.protocol.Packet;
import net.minecraft.server.RunningOnDifferentThreadException;
import net.minecraft.server.level.ServerPlayer;

/** A player's open Java menu on the server: what their client sent it, then `ServerPlayer.tick`'s part. */
final class Menus {
    private Menus() {}

    @SuppressWarnings("unchecked")
    static void serverTick(ServerPlayer player) {
        if (Clients.joining(player.getUUID())) login(player);
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

    /** What `PlayerList.placeNewPlayer` sends a client about recipes: vanilla's synced parts, then the types mods asked NeoForge to sync. */
    private static void login(ServerPlayer player) {
        var server = player.level().getServer();
        var recipes = server.getRecipeManager();
        player.connection.send(new net.minecraft.network.protocol.game.ClientboundUpdateRecipesPacket(recipes.getSynchronizedItemProperties(), recipes.getSynchronizedStonecutterRecipes()));
        var sync = net.neoforged.neoforge.common.NeoForge.EVENT_BUS.post(new net.neoforged.neoforge.event.OnDatapackSyncEvent(server.getPlayerList(), player));
        player.connection.send(net.neoforged.neoforge.network.payload.RecipeContentPayload.create(sync.getRecipeTypesToSend(), recipes.recipeMap()));
        var level = (net.minecraft.server.level.ServerLevel) player.level();
        for (var entity : new java.util.ArrayList<>(BlockEntities.held(level).values())) Clients.joined(level, entity);
        player.connection.send(new net.minecraft.network.protocol.game.ClientboundSetHeldSlotPacket(player.getInventory().getSelectedSlot()));
        server.getPlayerList().sendPlayerPermissionLevel(player);
        // Chat is Horizon's, so no chat session (INITIALIZE_CHAT) goes with the player's entry.
        player.connection.send(new net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket(java.util.EnumSet.of(
                net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket.Action.ADD_PLAYER,
                net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket.Action.UPDATE_GAME_MODE,
                net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket.Action.UPDATE_LISTED,
                net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket.Action.UPDATE_LATENCY,
                net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket.Action.UPDATE_DISPLAY_NAME,
                net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket.Action.UPDATE_HAT,
                net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket.Action.UPDATE_LIST_ORDER), java.util.List.of(player)));
        player.initInventoryMenu();
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
