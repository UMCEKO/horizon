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
        if (player.containerMenu == player.inventoryMenu) return;
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
    }

    /** The player left: Java's menu closes as a disconnect closes it. */
    static void close(ServerPlayer player) {
        if (player.containerMenu != player.inventoryMenu) player.closeContainer();
    }
}
