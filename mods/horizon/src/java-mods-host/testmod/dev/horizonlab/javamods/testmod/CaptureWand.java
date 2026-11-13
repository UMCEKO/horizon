package dev.horizonlab.javamods.testmod;

import net.minecraft.core.BlockPos;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.RedstoneLampBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.common.util.BlockSnapshot;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.registries.DeferredRegister;

/** A mod item whose `useOn` writes three blocks under NeoForge's capture: stone, or redstone blocks when clicked on gold. */
final class CaptureWand extends Item {
    CaptureWand(Properties properties) {
        super(properties);
    }

    static void register(DeferredRegister.Items items) {
        items.registerItem("capture_wand", CaptureWand::new);
        NeoForge.EVENT_BUS.addListener(CaptureWand::placed);
    }

    /** The three positions the wand writes above `clicked`, east of each other. */
    static BlockPos[] row(BlockPos clicked) {
        BlockPos first = clicked.above();
        return new BlockPos[] {first, first.east(), first.east(2)};
    }

    @Override
    public InteractionResult useOn(UseOnContext context) {
        var level = context.getLevel();
        boolean redstone = level.getBlockState(context.getClickedPos()).is(Blocks.GOLD_BLOCK);
        BlockState state = (redstone ? Blocks.REDSTONE_BLOCK : Blocks.STONE).defaultBlockState();
        for (BlockPos pos : row(context.getClickedPos())) level.setBlock(pos, state, 3);
        return InteractionResult.SUCCESS;
    }

    private static void placed(BlockEvent.EntityMultiPlaceEvent event) {
        if (event.getLevel().isClientSide() || event.getEntity() == null) return;
        if (!(event.getEntity() instanceof net.minecraft.world.entity.player.Player player) || !(player.getMainHandItem().getItem() instanceof CaptureWand)) return;
        var snapshots = event.getReplacedBlockSnapshots();
        var placed = snapshots.get(0).getCurrentState();
        if (placed.is(Blocks.STONE) && !player.isShiftKeyDown()) {
            Checks.check("java_place_is_multi_place", () -> snapshots.size() == 3 && snapshots.stream().allMatch(CaptureWand::airToStone));
        }
        if (placed.is(Blocks.REDSTONE_BLOCK)) {
            BlockPos lamp = snapshots.get(0).getPos().north();
            Checks.check("java_place_defers_updates", () -> {
                var state = event.getLevel().getBlockState(lamp);
                return state.is(Blocks.REDSTONE_LAMP) && !state.getValue(RedstoneLampBlock.LIT);
            });
        }
        if (player.isShiftKeyDown()) event.setCanceled(true);
    }

    private static boolean airToStone(BlockSnapshot snapshot) {
        return snapshot.getState().isAir() && snapshot.getCurrentState().is(Blocks.STONE);
    }
}
