package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.CaptureBridge;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** NeoForge's post-event loop calls `onPlace` per captured write: a vanilla block's runs native, a mod's keeps its Java body. */
@Mixin(BlockBehaviour.BlockStateBase.class)
abstract class StateOnPlaceMixin {
    @Inject(method = "onPlace", at = @At("HEAD"), cancellable = true)
    private void horizon$onPlace(Level level, BlockPos pos, BlockState old, boolean moved, CallbackInfo ci) {
        if (CaptureBridge.onPlaced(level, pos, (BlockState) (Object) this, old, moved)) ci.cancel();
    }
}
