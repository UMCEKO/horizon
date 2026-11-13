package dev.horizonlab.javamods.boot;

import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;

/** NeoForge's snapshot capture around a mod item's `useOn`: the captured writes and the deferred `onPlace`s go to Rust. */
public final class CaptureBridge {
    private CaptureBridge() {}

    /** Positions written under capture during this `GameBridge.use`, whose `onPlace` NeoForge's post-event loop still owes. */
    static final LongOpenHashSet CAPTURED = new LongOpenHashSet();

    /** Whether a mod hears `EntityPlaceEvent` or `EntityMultiPlaceEvent`: only then is a capture worth recording. */
    public static boolean placeListened() {
        var census = EventPosts.lastCensus();
        return census.containsKey("BlockEvent.EntityPlaceEvent") || census.containsKey("BlockEvent.EntityMultiPlaceEvent");
    }

    /** Remembers a captured write that happened, for `onPlaced`. */
    public static void captured(BlockPos pos) {
        CAPTURED.add(pos.asLong());
    }

    /** `BlockState.onPlace` from NeoForge's post-event loop on a captured vanilla block: true when native ran it instead. */
    public static boolean onPlaced(Level level, BlockPos pos, BlockState state, BlockState old, boolean moved) {
        if (!(level instanceof ServerLevel server) || !CAPTURED.remove(pos.asLong()) || BootFacts.ownsBlock(state.getBlock())) return false;
        onPlace(server, pos, state, old, moved);
        return true;
    }

    /** Bridged: one write as `Level.setBlock` makes under capture, with no `onPlace` and no updates. */
    public static boolean setBlock(ServerLevel level, BlockPos pos, BlockState state, int flags) {
        throw new IllegalStateException("not bridged");
    }

    /** Bridged: native's `BlockState.onPlace` of `state`, which replaced `old`. */
    private static void onPlace(ServerLevel level, BlockPos pos, BlockState state, BlockState old, boolean moved) {
        throw new IllegalStateException("not bridged");
    }
}
