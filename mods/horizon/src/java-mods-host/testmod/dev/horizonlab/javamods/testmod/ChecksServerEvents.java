package dev.horizonlab.javamods.testmod;

import java.util.List;
import java.util.Map;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.RedstoneLampBlock;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.event.level.ExplosionEvent;

/** The server census: what the test mod listens to is listened, a subclass through its supertype's listener, and Horizon's own listeners never count. */
final class ChecksServerEvents {
    private ChecksServerEvents() {}

    /** Where a neighbour notify is canceled, and where one's sides and reads are kept; null outside `run`. */
    private static volatile BlockPos cancelAt, watchAt;
    private static volatile int watchedSides = -1;
    private static volatile boolean watchedReadMatches;
    private static volatile String writeThrew = "";

    /** Listeners that do nothing but the neighbour notify's: the census only reads that they are there. */
    static void listen() {
        NeoForge.EVENT_BUS.addListener((BlockEvent.EntityPlaceEvent event) -> {});
        NeoForge.EVENT_BUS.addListener((ExplosionEvent.Start event) -> {});
        NeoForge.EVENT_BUS.addListener(ChecksServerEvents::notified);
    }

    private static void notified(BlockEvent.NeighborNotifyEvent event) {
        if (event.getPos().equals(cancelAt)) {
            event.setCanceled(true);
            try {
                ((ServerLevel) event.getLevel()).setBlock(event.getPos().above(), Blocks.STONE.defaultBlockState(), 3);
                writeThrew = "no throw";
            } catch (IllegalStateException e) {
                writeThrew = String.valueOf(e.getMessage());
            }
        } else if (event.getPos().equals(watchAt)) {
            watchedSides = event.getNotifiedSides().size();
            watchedReadMatches = event.getLevel().getBlockState(event.getPos()).equals(event.getState());
        }
    }

    static void run(UseOnContext context, BlockPos area) {
        Checks.check("census_sees_base_listener", () -> census().containsKey("BlockEvent.EntityPlaceEvent") && census().containsKey("BlockEvent.EntityMultiPlaceEvent"));
        Checks.check("census_sees_direct_listener", () -> census().getOrDefault("ExplosionEvent.Start", List.of()).stream().anyMatch(owner -> owner.startsWith("dev.horizonlab.javamods.testmod.")));
        Checks.check("census_skips_horizon", () -> census().values().stream().flatMap(List::stream).noneMatch(owner -> owner.startsWith("dev.horizonlab.javamods.boot.")));
        var level = (ServerLevel) context.getLevel();
        BlockPos canceled = area.above(2), heard = area.above(2).south(2);
        level.setBlock(canceled.east(), Blocks.REDSTONE_LAMP.defaultBlockState(), 3);
        level.setBlock(heard.east(), Blocks.REDSTONE_LAMP.defaultBlockState(), 3);
        cancelAt = canceled;
        watchAt = heard;
        try {
            level.setBlock(canceled, Blocks.REDSTONE_BLOCK.defaultBlockState(), 3);
            level.setBlock(heard, Blocks.REDSTONE_BLOCK.defaultBlockState(), 3);
        } finally {
            cancelAt = null;
            watchAt = null;
        }
        Checks.check("notify_cancel_keeps_lamp_off", () -> !level.getBlockState(canceled.east()).getValue(RedstoneLampBlock.LIT) && level.getBlockState(heard.east()).getValue(RedstoneLampBlock.LIT));
        Checks.check("notify_sides_all_six", () -> watchedSides == 6);
        Checks.check("notify_reads_the_cascades_level", () -> watchedReadMatches);
        Checks.check("notify_write_throws_inside_block_update", () -> writeThrew.equals("Horizon: level_set_block isn't served inside a block update (BlockEvent.NeighborNotifyEvent)") && level.getBlockState(canceled.above()).isAir());
    }

    @SuppressWarnings("unchecked")
    private static Map<String, List<String>> census() {
        try {
            var type = Class.forName("dev.horizonlab.javamods.boot.EventPosts", true, ClassLoader.getSystemClassLoader());
            return (Map<String, List<String>>) type.getMethod("lastCensus").invoke(null);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }
}
