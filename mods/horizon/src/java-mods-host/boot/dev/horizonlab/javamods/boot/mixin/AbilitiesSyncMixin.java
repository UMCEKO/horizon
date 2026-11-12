package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.PlayerSync;
import net.minecraft.server.level.ServerPlayer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A mod's `onUpdateAbilities` on the acting player reaches Horizon's player. */
@Mixin(ServerPlayer.class)
abstract class AbilitiesSyncMixin {
    @Inject(method = "onUpdateAbilities", at = @At("HEAD"))
    private void horizon$abilities(CallbackInfo ci) {
        PlayerSync.abilities(this);
    }
}
