package dev.horizonlab.javamods.boot;

import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.api.commands.GpuQueryPool;
import com.mojang.renderpearl.api.commands.RenderPass;
import com.mojang.renderpearl.api.pipeline.CompiledRenderPipeline;
import com.mojang.renderpearl.api.pipeline.IndexType;
import com.mojang.renderpearl.api.textures.GpuSampler;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.IntBuffer;
import java.util.Collection;
import java.util.function.Supplier;
import net.minecraft.client.renderer.DynamicGpuData;
import org.lwjgl.PointerBuffer;

/** A render pass that keeps the state it is given and appends a `RecordedDraw` for each draw call. */
public final class RecordingRenderPass implements RenderPass, RenderPass.UniformUploader {
    private final RecordingDevice device;
    private RecordingPipeline pipeline;
    private GpuBufferSlice vertex;
    private RecordingBuffer index;
    private IndexType indexType = IndexType.SHORT;
    private net.minecraft.resources.Identifier texture;
    private boolean textured;
    private DynamicGpuData.Transform transform;

    RecordingRenderPass(RecordingDevice device) {
        this.device = device;
    }

    @Override
    public void pushDebugGroup(Supplier<String> label) {}

    @Override
    public void popDebugGroup() {}

    @Override
    public void writeTimestamp(GpuQueryPool pool, int index) {}

    @Override
    public void setPipeline(CompiledRenderPipeline pipeline) {
        this.pipeline = pipeline instanceof RecordingPipeline recording ? recording : null;
    }

    @Override
    public void setUniform(String name, GpuTextureView textureView, GpuSampler sampler) {
        if (!name.equals("Sampler0")) return;
        textured = textureView != null;
        texture = textureView instanceof Sprites.View view ? view.id() : null;
    }

    @Override
    public void setUniform(String name, GpuBuffer value) {
        setUniform(name, value == null ? null : value.slice());
    }

    @Override
    public void setUniform(String name, GpuBufferSlice value) {
        if (name.equals("DynamicTransforms") && value != null && value.buffer() instanceof RecordingTransform holder) transform = holder.transform();
    }

    @Override
    public void pushConstants(ByteBuffer value) {}

    @Override
    public void setViewport(int x, int y, int width, int height) {}

    @Override
    public void enableScissor(int x, int y, int width, int height) {}

    @Override
    public void disableScissor() {}

    @Override
    public void setVertexBuffer(int slot, GpuBufferSlice vertexBuffer) {
        if (slot == 0) vertex = vertexBuffer;
    }

    @Override
    public void setIndexBuffer(GpuBuffer indexBuffer, IndexType indexType) {
        index = indexBuffer instanceof RecordingBuffer buffer ? buffer : null;
        this.indexType = indexType;
    }

    private void record(boolean indexed, int first, int count, int base) {
        if (pipeline == null || vertex == null || !(vertex.buffer() instanceof RecordingBuffer vertices) || count <= 0) return;
        if (indexed && index == null) return;
        var uses = transform != null ? transform : RecordingTransform.IDENTITY;
        device.draw(new RecordingDevice.RecordedDraw(pipeline, vertices.id(), vertices.version(), vertices.snapshot(), vertex.offset(), indexed ? index.id() : 0, indexed ? index.version() : 0,
                indexed ? index.snapshot() : null, indexType, first, count, base, texture, textured, uses, WorldFrame.stage));
    }

    @Override
    public void drawIndexed(int indexCount, int instanceCount, int firstIndex, int vertexOffset, int firstInstance) {
        record(true, firstIndex, indexCount, vertexOffset);
    }

    @Override
    public void multiDrawIndexed(IntBuffer drawParameters, int instanceCount, int firstInstance, int drawCount) {
        int at = drawParameters.position();
        for (int i = 0; i < drawCount; i++) record(true, drawParameters.get(at + i * 3 + 1), drawParameters.get(at + i * 3), drawParameters.get(at + i * 3 + 2));
    }

    @Override
    public void multiDrawIndexed(PointerBuffer firstIndexOffsets, IntBuffer indexCounts, IntBuffer vertexOffsets, int drawCount) {
        for (int i = 0; i < drawCount; i++) record(true, (int) (firstIndexOffsets.get(firstIndexOffsets.position() + i) / indexType.bytes), indexCounts.get(indexCounts.position() + i), vertexOffsets.get(vertexOffsets.position() + i));
    }

    private static IntBuffer commands(GpuBufferSlice slice) {
        if (!(slice.buffer() instanceof RecordingBuffer buffer)) return IntBuffer.allocate(0);
        return ByteBuffer.wrap(buffer.snapshot(), (int) slice.offset(), (int) slice.length()).order(ByteOrder.nativeOrder()).asIntBuffer();
    }

    @Override
    public void drawIndexedIndirect(GpuBufferSlice commands, int drawCount) {
        var c = commands(commands);
        for (int i = 0; i < drawCount; i++) record(true, c.get(i * 5 + 2), c.get(i * 5), c.get(i * 5 + 3));
    }

    @Override
    public <T> void drawMultipleIndexed(Collection<RenderPass.Draw<T>> draws, GpuBuffer defaultIndexBuffer, IndexType defaultIndexType, Collection<String> dynamicUniforms, T uniformArgument) {
        for (var draw : draws) {
            if (draw.uniformUploaderConsumer() != null) draw.uniformUploaderConsumer().accept(uniformArgument, this);
            setVertexBuffer(draw.slot(), draw.vertexBuffer().slice());
            setIndexBuffer(draw.indexBuffer() == null ? defaultIndexBuffer : draw.indexBuffer(), draw.indexType() == null ? defaultIndexType : draw.indexType());
            record(true, draw.firstIndex(), draw.indexCount(), draw.baseVertex());
        }
    }

    @Override
    public void draw(int vertexCount, int instanceCount, int firstVertex, int firstInstance) {
        record(false, firstVertex, vertexCount, 0);
    }

    @Override
    public void multiDraw(IntBuffer drawParameters, int instanceCount, int firstInstance, int drawCount) {
        int at = drawParameters.position();
        for (int i = 0; i < drawCount; i++) record(false, drawParameters.get(at + i * 2), drawParameters.get(at + i * 2 + 1), 0);
    }

    @Override
    public void multiDraw(IntBuffer firstVertices, IntBuffer vertexCounts, int drawCount) {
        for (int i = 0; i < drawCount; i++) record(false, firstVertices.get(firstVertices.position() + i), vertexCounts.get(vertexCounts.position() + i), 0);
    }

    @Override
    public void drawIndirect(GpuBufferSlice commands, int drawCount) {
        var c = commands(commands);
        for (int i = 0; i < drawCount; i++) record(false, c.get(i * 4 + 2), c.get(i * 4), 0);
    }

    @Override
    public void close() {}
}
