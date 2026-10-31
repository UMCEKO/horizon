package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.GameBridge;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.material.FluidState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(ServerLevel.class)
abstract class ServerLevelMixin extends Level {
    private ServerLevelMixin() {
        super(null, null, null, null, false, false, 0L, 0);
    }

    /** Fluids live in block states, so this is what vanilla's chunk lookup answers. */
    @Override
    public FluidState getFluidState(BlockPos pos) {
        return ((ServerLevel) (Object) this).getBlockState(pos).getFluidState();
    }

    @Inject(method = "getNextEntityId", at = @At("HEAD"), cancellable = true)
    private void horizon$nextEntityId(CallbackInfoReturnable<Integer> result) {
        result.setReturnValue(GameBridge.nextEntityId());
    }

    @Inject(method = "addFreshEntity", at = @At("HEAD"), cancellable = true)
    private void horizon$addFreshEntity(Entity entity, CallbackInfoReturnable<Boolean> result) {
        result.setReturnValue(GameBridge.spawn(entity));
    }

    @Inject(method = "playSeededSound(Lnet/minecraft/world/entity/Entity;DDDLnet/minecraft/core/Holder;Lnet/minecraft/sounds/SoundSource;FFJ)V", at = @At("HEAD"), cancellable = true)
    private void horizon$soundAt(Entity except, double x, double y, double z, Holder<SoundEvent> sound, SoundSource source, float volume, float pitch, long seed, CallbackInfo info) {
        GameBridge.sound(sound.value(), x, y, z, volume, pitch);
        info.cancel();
    }

    @Inject(method = "playSeededSound(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/world/entity/Entity;Lnet/minecraft/core/Holder;Lnet/minecraft/sounds/SoundSource;FFJ)V", at = @At("HEAD"), cancellable = true)
    private void horizon$soundFrom(Entity except, Entity from, Holder<SoundEvent> sound, SoundSource source, float volume, float pitch, long seed, CallbackInfo info) {
        GameBridge.sound(sound.value(), from.getX(), from.getY(), from.getZ(), volume, pitch);
        info.cancel();
    }
}
