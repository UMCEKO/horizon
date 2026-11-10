package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.Camera;
import net.minecraft.client.renderer.culling.Frustum;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;
import org.spongepowered.asm.mixin.gen.Invoker;

@Mixin(Camera.class)
public interface CameraAccess {
    @Invoker("alignWithEntity")
    void horizon$align(float partialTicks);

    @Invoker("setPosition")
    void horizon$position(Vec3 position);

    @Invoker("setRotation")
    void horizon$rotation(float yRot, float xRot);

    @Invoker("setupPerspective")
    void horizon$perspective(float near, float far, float fov, float width, float height);

    @Accessor("cullFrustum")
    void horizon$frustum(Frustum frustum);

    @Accessor("fov")
    void horizon$fov(float fov);

    @Accessor("hudFov")
    void horizon$hudFov(float fov);

    @Accessor("depthFar")
    void horizon$far(float far);

    @Accessor("initialized")
    void horizon$initialized(boolean initialized);
}
