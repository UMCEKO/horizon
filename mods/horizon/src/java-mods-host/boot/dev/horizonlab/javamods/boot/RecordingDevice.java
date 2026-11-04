package dev.horizonlab.javamods.boot;

import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.commands.CommandEncoder;
import com.mojang.renderpearl.api.commands.GpuQueryPool;
import com.mojang.renderpearl.api.device.DeviceFeatures;
import com.mojang.renderpearl.api.device.DeviceInfo;
import com.mojang.renderpearl.api.device.DeviceLimits;
import com.mojang.renderpearl.api.device.DeviceType;
import com.mojang.renderpearl.api.device.GpuDevice;
import com.mojang.renderpearl.api.device.GpuSurface;
import com.mojang.renderpearl.api.device.HintsAndWorkarounds;
import com.mojang.renderpearl.api.pipeline.CompiledRenderPipeline;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import com.mojang.renderpearl.api.pipeline.ShaderSource;
import com.mojang.renderpearl.api.textures.AddressMode;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuSampler;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import java.util.OptionalDouble;
import java.util.Set;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executor;
import java.util.function.BooleanSupplier;
import java.util.function.Supplier;
import net.minecraft.client.renderer.DynamicGpuData;

/** A GPU that keeps buffers' bytes and the draws made from them, for a world frame to turn into geometry: it draws nothing. */
public final class RecordingDevice implements GpuDevice {
    private long nextId = 1;
    /** Draws made on this device's passes since the last `take`. */
    final List<RecordedDraw> draws = new ArrayList<>();
    /** Ids of buffers closed since the last `take`. */
    final List<Long> closed = new ArrayList<>();
    private final CommandEncoder encoder = new RecordingEncoder(this);

    /** One draw call with the state it was made under. */
    record RecordedDraw(RecordingPipeline pipeline, long vertexId, int vertexVersion, byte[] vertex, long vertexOffset, long indexId, int indexVersion, byte[] index,
            com.mojang.renderpearl.api.pipeline.IndexType indexType, int first, int count, int base, net.minecraft.resources.Identifier texture, boolean textured,
            DynamicGpuData.Transform transform, int stage) {}

    long nextId() {
        return nextId++;
    }

    void closed(long id) {
        closed.add(id);
    }

    void draw(RecordedDraw draw) {
        draws.add(draw);
    }

    @Override
    public GpuSurface createSurface(long windowHandle, BooleanSupplier isIconified) {
        throw new UnsupportedOperationException("the world recording has no window");
    }

    @Override
    public CommandEncoder createCommandEncoder() {
        return encoder;
    }

    @Override
    public GpuSampler createSampler(AddressMode addressModeU, AddressMode addressModeV, FilterMode minFilter, FilterMode magFilter, int maxAnisotropy, OptionalDouble maxLod) {
        return new Sampler(addressModeU, addressModeV, minFilter, magFilter, maxAnisotropy, maxLod);
    }

    @Override
    public GpuTexture createTexture(Supplier<String> label, int usage, GpuFormat format, int width, int height, int depthOrLayers, int mipLevels) {
        return new Texture(label == null ? "" : label.get(), usage, format, width, height, depthOrLayers, mipLevels);
    }

    @Override
    public GpuTexture createTexture(String label, int usage, GpuFormat format, int width, int height, int depthOrLayers, int mipLevels) {
        return new Texture(label == null ? "" : label, usage, format, width, height, depthOrLayers, mipLevels);
    }

    @Override
    public GpuTextureView createTextureView(GpuTexture texture) {
        return texture instanceof GpuTextureView view ? view : new Texture(texture.getLabel(), texture.usage(), texture.getFormat(), texture.getWidth(0), texture.getHeight(0), texture.getDepthOrLayers(), texture.getMipLevels());
    }

    @Override
    public GpuTextureView createTextureView(GpuTexture texture, int baseMipLevel, int mipLevels) {
        return createTextureView(texture);
    }

    @Override
    public GpuBuffer createBuffer(Supplier<String> label, int usage, long size) {
        return new RecordingBuffer(this, nextId(), usage, new byte[(int) size]);
    }

    @Override
    public GpuBuffer createBuffer(Supplier<String> label, int usage, ByteBuffer data) {
        var bytes = new byte[data.remaining()];
        data.duplicate().get(bytes);
        return new RecordingBuffer(this, nextId(), usage, bytes);
    }

    @Override
    public List<String> getLastDebugMessages() {
        return List.of();
    }

    @Override
    public boolean isDebuggingEnabled() {
        return false;
    }

    @Override
    public CompletableFuture<CompiledRenderPipeline.Pending> compilePipeline(RenderPipeline pipeline, ShaderSource shaderSource, Executor executor) {
        return CompletableFuture.completedFuture(() -> new RecordingPipeline(pipeline));
    }

    @Override
    public void close() {}

    @Override
    public GpuQueryPool createTimestampQueryPool(int size) {
        return new GpuQueryPool() {
            @Override
            public int size() {
                return size;
            }

            @Override
            public java.util.OptionalLong getValue(int index) {
                return java.util.OptionalLong.empty();
            }

            @Override
            public java.util.OptionalLong[] getValues(int index, int count) {
                var empty = new java.util.OptionalLong[count];
                java.util.Arrays.fill(empty, java.util.OptionalLong.empty());
                return empty;
            }

            @Override
            public void close() {}
        };
    }

    private static final DeviceInfo INFO = new DeviceInfo("Horizon world recording", "Horizon", "", true, "recording", 1.0F,
            new DeviceLimits(1, 256, 32768, 1L << 30, 1024, 8, 1024), new DeviceFeatures(false, false, false, false, false, false, false, false), Set.of(),
            new HintsAndWorkarounds(false, false, false, false), DeviceType.OTHER);

    @Override
    public DeviceInfo getDeviceInfo() {
        return INFO;
    }

    /** A texture that stands for no file: a mod's own, which only the GPU could show. */
    private record Texture(String label, int usage, GpuFormat format, int width, int height, int depth, int mips) implements GpuTexture, GpuTextureView {
        @Override
        public int getWidth(int mipLevel) {
            return width;
        }

        @Override
        public int getHeight(int mipLevel) {
            return height;
        }

        @Override
        public int getDepthOrLayers() {
            return depth;
        }

        @Override
        public int getMipLevels() {
            return mips;
        }

        @Override
        public GpuFormat getFormat() {
            return format;
        }

        @Override
        public String getLabel() {
            return label;
        }

        @Override
        public boolean isClosed() {
            return false;
        }

        @Override
        public GpuTexture texture() {
            return this;
        }

        @Override
        public int baseMipLevel() {
            return 0;
        }

        @Override
        public int mipLevels() {
            return mips;
        }

        @Override
        public void close() {}
    }

    private record Sampler(AddressMode u, AddressMode v, FilterMode min, FilterMode mag, int anisotropy, OptionalDouble lod) implements GpuSampler {
        @Override
        public AddressMode getAddressModeU() {
            return u;
        }

        @Override
        public AddressMode getAddressModeV() {
            return v;
        }

        @Override
        public FilterMode getMinFilter() {
            return min;
        }

        @Override
        public FilterMode getMagFilter() {
            return mag;
        }

        @Override
        public int getMaxAnisotropy() {
            return anisotropy;
        }

        @Override
        public OptionalDouble getMaxLod() {
            return lod;
        }

        @Override
        public boolean isClosed() {
            return false;
        }

        @Override
        public void close() {}
    }
}
