package dev.horizonlab.javamods.boot.mixin;

import java.util.Map;
import net.minecraft.client.sounds.SoundManager;
import net.minecraft.client.sounds.WeighedSoundEvents;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Sounds are Horizon's: this client's sound manager knows no events of its own. */
@Mixin(SoundManager.class)
abstract class SoundsMixin {
    @Shadow
    @Final
    private Map<Identifier, WeighedSoundEvents> registry;

    /** Sounds are Horizon's, so this view loads none: an event lookup finds nothing rather than failing. */
    @Inject(method = "getSoundEvent", at = @At("HEAD"), cancellable = true)
    private void horizon$event(Identifier id, CallbackInfoReturnable<WeighedSoundEvents> result) {
        if (registry == null) result.setReturnValue(null);
    }
}
