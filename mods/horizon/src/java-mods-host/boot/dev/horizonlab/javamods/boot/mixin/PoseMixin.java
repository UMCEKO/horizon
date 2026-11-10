package dev.horizonlab.javamods.boot.mixin;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.math.Axis;
import dev.horizonlab.javamods.boot.BlockRenders;
import org.joml.Matrix4f;
import org.joml.Matrix4fc;
import org.joml.Quaternionf;
import org.joml.Quaternionfc;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** While a block entity renderer is recorded, each pose keeps the operations that built it. */
@Mixin(PoseStack.Pose.class)
abstract class PoseMixin {
    @Inject(method = "translate(FFF)Lorg/joml/Matrix4f;", at = @At("HEAD"))
    private void horizon$translate(float x, float y, float z, CallbackInfoReturnable<Matrix4f> info) {
        BlockRenders.op(this, 'T', x, y, z);
    }

    @Inject(method = "scale(FFF)V", at = @At("HEAD"))
    private void horizon$scale(float x, float y, float z, CallbackInfo info) {
        BlockRenders.op(this, 'S', x, y, z);
    }

    @Inject(method = "rotate(Lorg/joml/Quaternionfc;)V", at = @At("HEAD"))
    private void horizon$rotate(Quaternionfc by, CallbackInfo info) {
        BlockRenders.rotate(this, new Quaternionf(by));
    }

    @Inject(method = "rotate(Lcom/mojang/math/Axis;F)V", at = @At("HEAD"))
    private void horizon$rotateAxis(Axis axis, float angle, CallbackInfo info) {
        BlockRenders.rotate(this, axis.rotation(angle));
    }

    @Inject(method = "rotateDegrees(Lcom/mojang/math/Axis;F)V", at = @At("HEAD"))
    private void horizon$rotateDegrees(Axis axis, float angle, CallbackInfo info) {
        BlockRenders.rotate(this, axis.rotationDegrees(angle));
    }

    @Inject(method = "rotateAround(Lorg/joml/Quaternionfc;FFF)V", at = @At("HEAD"))
    private void horizon$rotateAround(Quaternionfc by, float x, float y, float z, CallbackInfo info) {
        BlockRenders.op(this, 'T', x, y, z);
        BlockRenders.rotate(this, new Quaternionf(by));
        BlockRenders.op(this, 'T', -x, -y, -z);
    }

    @Inject(method = "setIdentity()V", at = @At("HEAD"))
    private void horizon$identity(CallbackInfo info) {
        BlockRenders.reset(this);
    }

    @Inject(method = "mulPose(Lorg/joml/Matrix4fc;)V", at = @At("HEAD"))
    private void horizon$mul(Matrix4fc matrix, CallbackInfo info) {
        BlockRenders.op(this, 'M', matrix.get(new float[16]));
    }

    @Inject(method = "set(Lcom/mojang/blaze3d/vertex/PoseStack$Pose;)V", at = @At("HEAD"))
    private void horizon$set(PoseStack.Pose from, CallbackInfo info) {
        BlockRenders.copied(this, from);
    }
}
