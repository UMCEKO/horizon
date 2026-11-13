package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientRoutes;
import dev.horizonlab.javamods.boot.ClientWorld;
import net.minecraft.client.Minecraft;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The local player snapped by a client route (`syncBlockState`) is the window's player too. */
@Mixin(Entity.class)
abstract class ClientSnapMixin {
    @Inject(method = "absSnapTo(DDD)V", at = @At("HEAD"))
    private void horizon$routedSnap(double x, double y, double z, CallbackInfo info) {
        if (ClientRoutes.active() && (Object) this == Minecraft.getInstance().player) ClientWorld.snapPlayer(x, y, z);
    }
}
