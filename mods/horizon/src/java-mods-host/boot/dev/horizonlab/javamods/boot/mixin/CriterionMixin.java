package dev.horizonlab.javamods.boot.mixin;

import java.util.function.Predicate;
import net.minecraft.advancements.triggers.SimpleCriterionTrigger;
import net.minecraft.server.level.ServerPlayer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Horizon's own advancements hear vanilla's events; a view has no advancement data to test against. */
@Mixin(SimpleCriterionTrigger.class)
abstract class CriterionMixin<T> {
    @Inject(method = "trigger", at = @At("HEAD"), cancellable = true)
    private void horizon$trigger(ServerPlayer player, Predicate<T> predicate, CallbackInfo info) {
        info.cancel();
    }
}
