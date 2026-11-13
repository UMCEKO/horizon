package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.core.BlockPos;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LevelAccessor;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.common.util.BlockSnapshot;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

/** `BlockSnapshot`'s private constructor: a snapshot of the state Rust's capture recorded, which is no longer in the level. */
@Mixin(BlockSnapshot.class)
public interface BlockSnapshotAccess {
    @Invoker("<init>")
    static BlockSnapshot horizon$create(ResourceKey<Level> dim, LevelAccessor level, BlockPos pos, BlockState state, CompoundTag nbt, int flags) {
        throw new AssertionError();
    }
}
