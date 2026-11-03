package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.Camera;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

@Mixin(Camera.class)
public interface CameraAccess {
    @Invoker("alignWithEntity")
    void horizon$align(float partialTicks);
}
