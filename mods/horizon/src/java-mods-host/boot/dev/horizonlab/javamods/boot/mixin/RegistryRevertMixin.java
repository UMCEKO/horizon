package dev.horizonlab.javamods.boot.mixin;

import net.neoforged.neoforge.registries.RegistryManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The client never replaced a registry (it filters the server's own), and the server in this JVM keeps using them: leaving a world must not unbind their tags. */
@Mixin(value = RegistryManager.class, remap = false)
abstract class RegistryRevertMixin {
    @Inject(method = "revertToFrozen", at = @At("HEAD"), cancellable = true, remap = false)
    private static void horizon$keep(CallbackInfo info) {
        info.cancel();
    }
}
