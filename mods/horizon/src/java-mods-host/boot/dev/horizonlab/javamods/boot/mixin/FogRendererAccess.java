package dev.horizonlab.javamods.boot.mixin;

import java.util.List;
import net.minecraft.client.renderer.fog.FogRenderer;
import net.minecraft.client.renderer.fog.environment.FogEnvironment;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(FogRenderer.class)
public interface FogRendererAccess {
    @Accessor("FOG_ENVIRONMENTS")
    static List<FogEnvironment> horizon$environments() {
        throw new AssertionError();
    }
}
