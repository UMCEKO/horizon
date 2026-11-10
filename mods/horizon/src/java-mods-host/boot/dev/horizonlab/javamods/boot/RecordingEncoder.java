package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.platform.NativeImage;
import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.api.buffers.TransientMemory;
import com.mojang.renderpearl.api.commands.CommandEncoder;
import com.mojang.renderpearl.api.commands.GpuFence;
import com.mojang.renderpearl.api.commands.GpuQueryPool;
import com.mojang.renderpearl.api.commands.RenderPass;
import com.mojang.renderpearl.api.commands.RenderPassDescriptor;
import com.mojang.renderpearl.api.textures.GpuTexture;
import java.nio.ByteBuffer;
import org.joml.Vector4fc;

/** Buffer writes change the bytes; render passes record their draws; everything else a GPU would do with pixels is left out. */
public final class RecordingEncoder implements CommandEncoder {
    private final RecordingDevice device;

    RecordingEncoder(RecordingDevice device) {
        this.device = device;
    }

    @Override
    public void submit() {}

    @Override
    public TransientMemory transientMemory() {
        throw new UnsupportedOperationException("the world recording has no transient memory");
    }

    @Override
    public RenderPass createRenderPass(RenderPassDescriptor descriptor) {
        return new RecordingRenderPass(device);
    }

    @Override
    public void clearColorTexture(GpuTexture colorTexture, Vector4fc clearColor) {}

    @Override
    public void clearColorAndDepthTextures(GpuTexture colorTexture, Vector4fc clearColor, GpuTexture depthTexture, double clearDepth) {}

    @Override
    public void clearColorAndDepthTextures(GpuTexture colorTexture, Vector4fc clearColor, GpuTexture depthTexture, double clearDepth, int regionX, int regionY, int regionWidth, int regionHeight, int mipLevel) {}

    @Override
    public void clearDepthTexture(GpuTexture depthTexture, double clearDepth) {}

    @Override
    public void clearStencilTexture(GpuTexture stencilTexture, int clearValue) {}

    @Override
    public void writeToBuffer(GpuBufferSlice destination, ByteBuffer data) {
        if (destination.buffer() instanceof RecordingBuffer buffer) buffer.write(destination.offset(), data);
    }

    @Override
    public void copyToBuffer(GpuBufferSlice source, GpuBufferSlice target) {
        if (source.buffer() instanceof RecordingBuffer from && target.buffer() instanceof RecordingBuffer to) to.copyFrom(from, source.offset(), target.offset(), Math.min(source.length(), target.length()));
    }

    @Override
    public void writeToTexture(GpuTexture destination, NativeImage source) {}

    @Override
    public void writeToTexture(GpuTexture destination, NativeImage source, int mipLevel, int depthOrLayer, int destX, int destY) {}

    @Override
    public void writeToTexture(GpuTexture destination, ByteBuffer source, int mipLevel, int depthOrLayer, int destX, int destY, int width, int height) {}

    @Override
    public void copyBufferToTexture(GpuBufferSlice source, int sourceX, int sourceY, int sourceWidth, int sourceHeight, GpuTexture destination, int destinationX, int destinationY, int copyWidth, int copyHeight, int mipLevel, int arrayLayer) {}

    @Override
    public void copyTextureToBuffer(GpuTexture source, GpuBuffer destination, long offset, Runnable callback, int mipLevel) {}

    @Override
    public void copyTextureToBuffer(GpuTexture source, GpuBuffer destination, long offset, Runnable callback, int mipLevel, int x, int y, int width, int height) {}

    @Override
    public void copyTextureToTexture(GpuTexture source, GpuTexture destination, int mipLevel, int destX, int destY, int sourceX, int sourceY, int width, int height) {}

    @Override
    public GpuFence createFence() {
        return new GpuFence() {
            @Override
            public void close() {}

            @Override
            public boolean awaitCompletion(long timeoutNS) {
                return true;
            }
        };
    }

    @Override
    public void writeTimestamp(GpuQueryPool pool, int index) {}
}
