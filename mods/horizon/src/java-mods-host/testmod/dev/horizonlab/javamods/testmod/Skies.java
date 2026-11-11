package dev.horizonlab.javamods.testmod;

import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.vertex.BufferBuilder;
import com.mojang.blaze3d.vertex.ByteBufferBuilder;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.api.commands.RenderPass;
import com.mojang.renderpearl.api.commands.RenderPassDescriptor;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import net.minecraft.client.CloudStatus;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.state.level.LevelRenderState;
import net.minecraft.client.renderer.state.level.SkyRenderState;
import net.minecraft.client.renderer.state.level.WeatherRenderState;
import net.minecraft.resources.Identifier;
import net.minecraft.world.phys.Vec3;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.client.CustomCloudsRenderer;
import net.neoforged.neoforge.client.CustomSkyboxRenderer;
import net.neoforged.neoforge.client.CustomWeatherEffectRenderer;
import net.neoforged.neoforge.client.event.RegisterCustomEnvironmentEffectRendererEvent;
import org.joml.Matrix4fc;

/** Custom environment renderers for the `test_sky` biome: a green square straight up, no clouds and no rain. */
final class Skies {
    private Skies() {}

    static final Identifier ID = Identifier.fromNamespaceAndPath("horizon_java_test", "test");

    static void register(IEventBus bus) {
        bus.addListener((RegisterCustomEnvironmentEffectRendererEvent event) -> {
            event.registerSkyboxRenderer(ID, new CustomSkyboxRenderer() {
                @Override
                public boolean renderSky(LevelRenderState state, SkyRenderState sky, Matrix4fc modelView, GpuBufferSlice transform) {
                    draw(transform);
                    return true;
                }
            });
            event.registerCloudRenderer(ID, new CustomCloudsRenderer() {
                @Override
                public boolean renderClouds(LevelRenderState state, CloudStatus status, Matrix4fc modelView, RenderPass pass) {
                    return true;
                }
            });
            event.registerWeatherEffectRenderer(ID, new CustomWeatherEffectRenderer() {
                @Override
                public boolean renderSnowAndRain(LevelRenderState state, WeatherRenderState weather, Vec3 camera, RenderPass pass) {
                    return true;
                }

                @Override
                public boolean tickRain(net.minecraft.client.multiplayer.ClientLevel level, long ticks, net.minecraft.client.Camera camera) {
                    return true;
                }
            });
        });
    }

    private static void draw(GpuBufferSlice transform) {
        ByteBuffer mesh;
        try (var builder = new ByteBufferBuilder(256)) {
            var buffer = new BufferBuilder(builder, PrimitiveTopology.QUADS, DefaultVertexFormat.POSITION_COLOR);
            for (float[] corner : new float[][] {{-1, 100, -1}, {1, 100, -1}, {1, 100, 1}, {-1, 100, 1}}) buffer.addVertex(corner[0], corner[1], corner[2]).setColor(0xFF00FF00);
            try (var data = buffer.buildOrThrow()) {
                mesh = ByteBuffer.allocate(data.vertexBuffer().remaining()).order(ByteOrder.nativeOrder());
                mesh.put(data.vertexBuffer());
                mesh.flip();
            }
        }
        var device = RenderSystem.getDevice();
        var vertices = device.createBuffer(() -> "Test sky", GpuBuffer.USAGE_VERTEX, mesh);
        try (var pass = device.createCommandEncoder().createRenderPass(RenderPassDescriptor.builder(() -> "Test sky").build())) {
            pass.setPipeline(RenderSystem.getCompiledPipeline(RenderPipelines.DEBUG_QUADS));
            pass.setUniform("DynamicTransforms", transform);
            pass.setVertexBuffer(0, vertices.slice());
            pass.draw(4, 1, 0, 0);
        }
    }
}
