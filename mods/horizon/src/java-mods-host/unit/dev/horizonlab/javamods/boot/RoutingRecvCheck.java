package dev.horizonlab.javamods.boot;

import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.ChestMenu;
import net.minecraft.world.inventory.InventoryMenu;
import sun.reflect.ReflectionFactory;

/** The receiver a player-filled route hands the method, by the owner's class. */
public final class RoutingRecvCheck {
    private static <T> T bare(Class<T> type) throws Exception {
        var parent = Object.class.getDeclaredConstructor();
        return type.cast(ReflectionFactory.getReflectionFactory().newConstructorForSerialization(type, parent).newInstance());
    }

    public static void main(String[] args) throws Exception {
        var player = bare(ServerPlayer.class);
        var connection = bare(ServerGamePacketListenerImpl.class);
        player.connection = connection;
        var proj = Routing.projection(ServerGamePacketListenerImpl.class);
        if (Routing.project(proj, ServerGamePacketListenerImpl.class, player, "ServerGamePacketListenerImpl#tryPickItem(I)V") != connection) throw new AssertionError("a packet listener's route runs on the connection");
        if (Routing.projection(net.minecraft.server.network.ServerCommonPacketListenerImpl.class) != Routing.Proj.CONNECTION) throw new AssertionError("the common listener too");
        if (Routing.projection(net.minecraft.world.food.FoodData.class) != Routing.Proj.FOOD) throw new AssertionError("food data");
        if (Routing.projection(net.minecraft.world.entity.player.Inventory.class) != Routing.Proj.INVENTORY) throw new AssertionError("inventory");
        if (Routing.projection(ServerPlayer.class) != Routing.Proj.PLAYER) throw new AssertionError("anything else is the player");
        player.containerMenu = bare(InventoryMenu.class);
        var any = Routing.projection(AbstractContainerMenu.class);
        if (Routing.project(any, AbstractContainerMenu.class, player, "k") != player.containerMenu) throw new AssertionError("any menu passes the base class");
        var chest = Routing.projection(ChestMenu.class);
        try {
            Routing.project(chest, ChestMenu.class, player, "net/minecraft/world/inventory/ChestMenu#removed(Lnet/minecraft/world/entity/player/Player;)V");
            throw new AssertionError("a chest route must throw when the open menu is the inventory's");
        } catch (IllegalStateException expected) {
            if (!expected.getMessage().startsWith("route net/minecraft/world/inventory/ChestMenu#removed")) throw new AssertionError(expected.getMessage());
        }
        player.containerMenu = bare(ChestMenu.class);
        if (Routing.project(chest, ChestMenu.class, player, "k") != player.containerMenu) throw new AssertionError("a chest menu passes its own route");
        System.out.println("RoutingRecvCheck ok");
    }
}
