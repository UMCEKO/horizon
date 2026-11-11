package dev.horizonlab.javamods.boot;

import net.minecraft.core.BlockPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityTicker;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockState;

/** `LevelChunk.BoundTickingBlockEntity`: a block entity with the ticker `updateBlockEntityTicker` bound for one state, null included. */
final class BoundTicker<T extends BlockEntity> {
    private static final org.slf4j.Logger LOGGER = com.mojang.logging.LogUtils.getLogger();

    final T entity;
    private BlockState bound;
    private BlockEntityTicker<T> ticker;
    private boolean loggedInvalidBlockState;

    BoundTicker(T entity) {
        this.entity = entity;
    }

    /** `LevelChunk.updateBlockEntityTicker`: the state's block is asked once, until the next bind. */
    @SuppressWarnings("unchecked")
    void bind(Level level, BlockState state) {
        bound = state;
        ticker = state.getTicker(level, (BlockEntityType<T>) entity.getType());
    }

    BlockState bound() {
        return bound;
    }

    /** `BoundTickingBlockEntity.tick` with the state at the entity's position; true when a ticker ran. */
    boolean run(Level level, BlockPos pos, BlockState state) {
        if (ticker == null) return false;
        if (!entity.getType().isValid(state)) {
            if (!loggedInvalidBlockState) {
                loggedInvalidBlockState = true;
                LOGGER.warn("Block entity {} @ {} state {} invalid for ticking:", entity.typeHolder().getRegisteredName(), pos, state);
            }
            return false;
        }
        ticker.tick(level, pos, state, entity);
        loggedInvalidBlockState = false;
        return true;
    }
}
