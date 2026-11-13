package dev.horizonlab.javamods.testmod.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import dev.horizonlab.javamods.testmod.Checks;
import dev.horizonlab.javamods.testmod.UseState;
import net.minecraft.world.item.BucketItem;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.HitResult;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/** A body route: `BucketItem.use` looks at the type of its ray's hit, which the mod records. */
@Mixin(BucketItem.class)
public abstract class UseBucketMixin {
    @WrapOperation(method = "use", at = @At(value = "INVOKE", target = "Lnet/minecraft/world/phys/BlockHitResult;getType()Lnet/minecraft/world/phys/HitResult$Type;", ordinal = 0))
    private HitResult.Type horizon$type(BlockHitResult hit, Operation<HitResult.Type> original) {
        var type = original.call(hit);
        UseState.bucketRays++;
        UseState.lastBucketType = type;
        Checks.check("use_bucket_ray_type_recorded", () -> UseState.lastBucketType != null);
        return type;
    }
}
