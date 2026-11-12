package dev.horizonlab.javamods.testmod;

import java.util.List;
import java.util.Map;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.piston.PistonBaseBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.event.level.PistonEvent;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.event.level.ExplosionEvent;

/** The server census: what the test mod listens to is listened, a subclass through its supertype's listener, and Horizon's own listeners never count. */
final class ChecksServerEvents {
    private ChecksServerEvents() {}

    /** Listeners that do nothing: the census only reads that they are there. */
    static void listen() {
        NeoForge.EVENT_BUS.addListener((BlockEvent.EntityPlaceEvent event) -> {});
        NeoForge.EVENT_BUS.addListener((ExplosionEvent.Start event) -> {});
        NeoForge.EVENT_BUS.addListener((PistonEvent.Pre event) -> {
            if (event.getLevel() instanceof Level level && !level.isClientSide() && event.getPos().equals(marked)) event.setCanceled(true);
        });
        NeoForge.EVENT_BUS.addListener((PistonEvent.Post event) -> {
            if (event.getLevel() instanceof Level level && !level.isClientSide() && event.getPos().equals(moving)) postedFront = level.getBlockState(event.getPos().relative(event.getDirection()));
        });
    }

    /** The piston `PistonEvent.Pre` cancels, and the one whose `Post` records what stood in front of it. */
    private static BlockPos marked, moving;
    private static BlockState postedFront;

    /** A piston at the marked point with stone in front, and a second one at `moving`, each powered from here. */
    private static void pistons(Level level, BlockPos area) {
        marked = area.offset(0, 0, 4);
        moving = area.offset(0, 0, 8);
        for (BlockPos at : new BlockPos[] {marked, moving}) {
            level.setBlock(at, Blocks.PISTON.defaultBlockState().setValue(PistonBaseBlock.FACING, Direction.EAST), 3);
            level.setBlock(at.east(), Blocks.STONE.defaultBlockState(), 3);
            level.setBlock(at.above(), Blocks.REDSTONE_BLOCK.defaultBlockState(), 3);
        }
    }

    /** After the tick: the canceled piston stayed retracted with its stone, and the other had moved by `Post`. */
    static void later(Level level) {
        if (marked == null) return;
        Checks.check("piston_pre_cancel_keeps_it_retracted", () -> !level.getBlockState(marked).getValue(PistonBaseBlock.EXTENDED) && level.getBlockState(marked.east()).is(Blocks.STONE));
        Checks.check("piston_post_fires_after_move", () -> postedFront != null && (postedFront.is(Blocks.MOVING_PISTON) || postedFront.is(Blocks.PISTON_HEAD)));
    }

    static void run(UseOnContext context, BlockPos area) {
        pistons(context.getLevel(), area.offset(0, 0, 40));
        Checks.check("census_sees_base_listener", () -> census().containsKey("BlockEvent.EntityPlaceEvent") && census().containsKey("BlockEvent.EntityMultiPlaceEvent"));
        Checks.check("census_sees_direct_listener", () -> census().getOrDefault("ExplosionEvent.Start", List.of()).stream().anyMatch(owner -> owner.startsWith("dev.horizonlab.javamods.testmod.")));
        Checks.check("census_skips_horizon", () -> census().values().stream().flatMap(List::stream).noneMatch(owner -> owner.startsWith("dev.horizonlab.javamods.boot.")));
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
