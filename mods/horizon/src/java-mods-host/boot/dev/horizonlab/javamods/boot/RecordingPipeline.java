package dev.horizonlab.javamods.boot;

import com.mojang.renderpearl.api.pipeline.CompiledRenderPipeline;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;

/** A pipeline "compiled" as itself: the recording reads its state. */
public record RecordingPipeline(RenderPipeline pipeline) implements CompiledRenderPipeline {
    @Override
    public boolean isClosed() {
        return false;
    }

    @Override
    public void close() {}
}
