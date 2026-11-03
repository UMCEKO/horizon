package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.PlayerSync;
import net.minecraft.world.food.FoodData;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The acting player's food reaches Horizon's player as each of FoodData's writes changes it. */
@Mixin(FoodData.class)
abstract class FoodSyncMixin {
    @Inject(method = {"add", "addExhaustion", "setFoodLevel", "setSaturation"}, at = @At("RETURN"))
    private void horizon$food(CallbackInfo ci) {
        PlayerSync.food((FoodData) (Object) this);
    }
}
