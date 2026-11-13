package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.CaptureBridge;
import dev.horizonlab.javamods.boot.Clients;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.common.util.BlockSnapshot;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** NeoForge's capture steps of `Level.setBlock`, which Horizon's bridge of the method would skip: a mod item's placement records its snapshots. */
@Mixin(Level.class)
abstract class LevelCaptureMixin {
    @Inject(method = "setBlock(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/block/state/BlockState;II)Z", at = @At("HEAD"), cancellable = true)
    private void horizon$capture(BlockPos pos, BlockState state, int flags, int limit, CallbackInfoReturnable<Boolean> cir) {
        Level self = (Level) (Object) this;
        if (!(self instanceof ServerLevel level) || !self.captureBlockSnapshots || !CaptureBridge.placeListened()) return;
        var snap = BlockSnapshot.create(self.dimension(), self, pos.immutable(), flags);
        self.capturedBlockSnapshots.add(snap);
        boolean changed = CaptureBridge.setBlock(level, pos, state, flags);
        if (!changed) {
            self.capturedBlockSnapshots.remove(snap);
        } else {
            CaptureBridge.captured(pos);
            // The RETURN injection that tells Java clients is skipped by this early return.
            if ((flags & 2) != 0) Clients.blockChanged(level, pos);
        }
        cir.setReturnValue(changed);
    }
}
