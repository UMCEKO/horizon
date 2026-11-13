package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.MotionLog;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Mods' writes to the local player's motion state, as the setters left it, for the window's next step. */
@Mixin(Entity.class)
abstract class MotionWritesMixin {
    @Inject(method = "setDeltaMovement(Lnet/minecraft/world/phys/Vec3;)V", at = @At("RETURN"))
    private void horizon$velocity(Vec3 v, CallbackInfo info) {
        if (MotionLog.watching != (Object) this) return;
        Vec3 now = ((Entity) (Object) this).getDeltaMovement();
        MotionLog.velocity(this, now.x, now.y, now.z);
    }

    @Inject(method = "setPosRaw(DDD)V", at = @At("RETURN"))
    private void horizon$position(double x, double y, double z, CallbackInfo info) {
        if (MotionLog.watching != (Object) this) return;
        MotionLog.position(this, x, y, z);
    }

    @Inject(method = "setYRot(F)V", at = @At("RETURN"))
    private void horizon$yaw(float y, CallbackInfo info) {
        if (MotionLog.watching != (Object) this) return;
        MotionLog.yaw(this, ((Entity) (Object) this).getYRot());
    }

    @Inject(method = "setXRot(F)V", at = @At("RETURN"))
    private void horizon$pitch(float x, CallbackInfo info) {
        if (MotionLog.watching != (Object) this) return;
        MotionLog.pitch(this, ((Entity) (Object) this).getXRot());
    }

    @Inject(method = "setOnGround(Z)V", at = @At("RETURN"))
    private void horizon$ground(boolean ground, CallbackInfo info) {
        if (MotionLog.watching != (Object) this) return;
        MotionLog.ground(this, ground);
    }

    @Inject(method = "resetFallDistance()V", at = @At("RETURN"))
    private void horizon$fallReset(CallbackInfo info) {
        if (MotionLog.watching != (Object) this) return;
        MotionLog.fallReset(this);
    }

    @Inject(method = "setSprinting(Z)V", at = @At("RETURN"))
    private void horizon$sprint(boolean sprinting, CallbackInfo info) {
        if (MotionLog.watching != (Object) this) return;
        MotionLog.sprint(this, sprinting);
    }

    @Inject(method = "setOldPosAndRot()V", at = @At("RETURN"))
    private void horizon$oldPos(CallbackInfo info) {
        if (MotionLog.watching != (Object) this) return;
        MotionLog.oldPos(this);
    }
}
