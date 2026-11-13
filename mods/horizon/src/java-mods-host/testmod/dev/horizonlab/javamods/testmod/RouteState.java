package dev.horizonlab.javamods.testmod;

import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.player.Inventory;

/** What the routed `setSelectedSlot` handlers count between calls. */
public final class RouteState {
    private RouteState() {}

    public static boolean canceled;
    public static int tails;
    public static int returns;
    public static boolean initRan;
    public static boolean runRan;
    public static boolean initBeforeRun;
    /** The scoreboard handlers that ran, in order, for team `red`. */
    public static final java.util.List<String> teamLog = new java.util.ArrayList<>();
    /** The players `unpackLootTable` saw for containers with a table, null for none. */
    public static final java.util.List<String> unpackPlayers = new java.util.ArrayList<>();
    /** Server-side `BrushItem.onUseTick` calls so far. */
    public static int brushTicks;

    /** The Java client's own inventory runs the same handlers; only the server's is routed. */
    public static boolean server(Inventory inventory) {
        return inventory.player instanceof ServerPlayer;
    }
}
