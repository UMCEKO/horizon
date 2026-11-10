package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ModKeyMappings;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Options;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Vanilla's key mappings are all `Options` has when it hands the mods their registration event. */
@Mixin(Options.class)
abstract class KeyMappingsMixin {
    @Shadow
    public KeyMapping[] keyMappings;

    @Inject(method = "<init>", at = @At(value = "INVOKE", target = "Lnet/neoforged/neoforge/client/ClientHooks;onRegisterKeyMappings(Lnet/minecraft/client/Options;Ljava/util/List;)V"))
    private void horizon$vanillaKeys(Minecraft minecraft, java.io.File folder, CallbackInfo info) {
        ModKeyMappings.snapshot(keyMappings);
    }
}
