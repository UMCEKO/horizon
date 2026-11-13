package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.MotionLog;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.entity.player.Abilities;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** `LocalPlayer.onUpdateAbilities` from a mod: the flight it set reaches the window's step, which sends it as vanilla does here. */
@Mixin(LocalPlayer.class)
abstract class MotionAbilitiesMixin {
    @Inject(method = "onUpdateAbilities()V", at = @At("HEAD"))
    private void horizon$abilities(CallbackInfo info) {
        if (MotionLog.watching != (Object) this) return;
        Abilities a = ((LocalPlayer) (Object) this).getAbilities();
        MotionLog.abilities(this, a.flying, a.mayfly, a.getFlyingSpeed());
    }
}
