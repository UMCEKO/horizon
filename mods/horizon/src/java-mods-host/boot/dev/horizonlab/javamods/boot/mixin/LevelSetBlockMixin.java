package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.Clients;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** `Level.markAndNotifyBlock`: a server block set with `UPDATE_CLIENTS` is sent to clients, which Horizon's binding of `setBlock` skips. */
@Mixin(Level.class)
abstract class LevelSetBlockMixin {
    @Inject(method = "setBlock(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/block/state/BlockState;II)Z", at = @At("RETURN"))
    private void horizon$set(BlockPos pos, BlockState state, int flags, int limit, CallbackInfoReturnable<Boolean> result) {
        if ((Object) this instanceof ServerLevel level && result.getReturnValueZ() && (flags & 2) != 0) Clients.blockChanged(level, pos);
    }
}
