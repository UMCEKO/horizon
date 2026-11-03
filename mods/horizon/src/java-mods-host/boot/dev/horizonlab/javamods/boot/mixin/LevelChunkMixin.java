package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.BlockEntities;
import dev.horizonlab.javamods.boot.Clients;
import java.util.Map;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.material.FluidState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Every chunk in Java is a view (`ChunkViews`): its blocks and block entities are the level's. */
@Mixin(LevelChunk.class)
abstract class LevelChunkMixin {
    private net.minecraft.world.level.Level horizon$level() {
        return ((LevelChunk) (Object) this).getLevel();
    }

    @Inject(method = "getBlockState(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/block/state/BlockState;", at = @At("HEAD"), cancellable = true)
    private void horizon$state(BlockPos pos, CallbackInfoReturnable<BlockState> result) {
        result.setReturnValue(horizon$level().getBlockState(pos));
    }

    @Inject(method = "getFluidState(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/material/FluidState;", at = @At("HEAD"), cancellable = true)
    private void horizon$fluid(BlockPos pos, CallbackInfoReturnable<FluidState> result) {
        result.setReturnValue(horizon$level().getFluidState(pos));
    }

    @Inject(method = "getFluidState(III)Lnet/minecraft/world/level/material/FluidState;", at = @At("HEAD"), cancellable = true)
    private void horizon$fluidAt(int x, int y, int z, CallbackInfoReturnable<FluidState> result) {
        result.setReturnValue(horizon$level().getFluidState(new BlockPos(x, y, z)));
    }

    @Inject(method = "setBlockState", at = @At("HEAD"), cancellable = true)
    private void horizon$setState(BlockPos pos, BlockState state, int flags, CallbackInfoReturnable<BlockState> result) {
        BlockState old = horizon$level().getBlockState(pos);
        result.setReturnValue(horizon$level().setBlock(pos, state, flags) ? old : null);
    }

    @Inject(method = "getBlockEntity(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/block/entity/BlockEntity;", at = @At("HEAD"), cancellable = true)
    private void horizon$entity(BlockPos pos, CallbackInfoReturnable<BlockEntity> result) {
        result.setReturnValue(horizon$level().getBlockEntity(pos));
    }

    @Inject(method = "getBlockEntity(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/chunk/LevelChunk$EntityCreationType;)Lnet/minecraft/world/level/block/entity/BlockEntity;", at = @At("HEAD"), cancellable = true)
    private void horizon$entityMaybe(BlockPos pos, LevelChunk.EntityCreationType creation, CallbackInfoReturnable<BlockEntity> result) {
        result.setReturnValue(horizon$level().getBlockEntity(pos));
    }

    @Inject(method = "setBlockEntity", at = @At("HEAD"), cancellable = true)
    private void horizon$setEntity(BlockEntity entity, CallbackInfo info) {
        if (horizon$level() instanceof ServerLevel level) BlockEntities.set(level, entity);
        info.cancel();
    }

    @Inject(method = "removeBlockEntity", at = @At("HEAD"), cancellable = true)
    private void horizon$removeEntity(BlockPos pos, CallbackInfo info) {
        if (horizon$level() instanceof ServerLevel level) BlockEntities.remove(level, pos);
        info.cancel();
    }

    @Inject(method = "getBlockEntities", at = @At("HEAD"), cancellable = true)
    private void horizon$entities(CallbackInfoReturnable<Map<BlockPos, BlockEntity>> result) {
        var chunk = ((LevelChunk) (Object) this).getPos();
        result.setReturnValue(horizon$level() instanceof ServerLevel level ? BlockEntities.inChunk(level, chunk) : Clients.clientEntities.apply(chunk));
    }
}
