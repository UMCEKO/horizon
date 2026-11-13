package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.Camera;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.extract.LevelExtractor;
import net.minecraft.client.renderer.state.level.LevelRenderState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;
import org.spongepowered.asm.mixin.gen.Invoker;

/** The world recording runs the extractor's block outline step itself: `setLevel` is cancelled, so it sets the level. */
@Mixin(LevelExtractor.class)
public interface LevelExtractorAccess {
    @Accessor("level")
    void horizon$setLevel(ClientLevel level);

    @Invoker("extractBlockOutline")
    void horizon$extractBlockOutline(Camera camera, LevelRenderState state);
}
