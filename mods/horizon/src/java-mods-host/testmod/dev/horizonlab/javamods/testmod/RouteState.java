package dev.horizonlab.javamods.testmod;

import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.player.Inventory;

/** What the routed `setSelectedSlot` handlers count between calls. */
public final class RouteState {
    private RouteState() {}

    public static boolean canceled;
    public static int tails;
    public static int returns;

    /** The Java client's own inventory runs the same handlers; only the server's is routed. */
    public static boolean server(Inventory inventory) {
        return inventory.player instanceof ServerPlayer;
    }
}
