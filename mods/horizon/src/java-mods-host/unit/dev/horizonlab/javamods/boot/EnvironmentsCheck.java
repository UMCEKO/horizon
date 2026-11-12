package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.vertex.BufferBuilder;
import com.mojang.blaze3d.vertex.ByteBufferBuilder;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.api.commands.RenderPass;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import net.minecraft.client.CloudStatus;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.state.level.LevelRenderState;
import net.minecraft.client.renderer.state.level.SkyRenderState;
import net.minecraft.client.renderer.state.level.WeatherRenderState;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.client.CustomCloudsRenderer;
import net.neoforged.neoforge.client.CustomSkyboxRenderer;
import net.neoforged.neoforge.client.CustomWeatherEffectRenderer;
import org.joml.Matrix4f;
import org.joml.Matrix4fc;

/** A skybox renderer's one quad straight up is a camera-anchored SKY piece whatever the view's rotation, and each renderer's answer is a bit of the status. */
public final class EnvironmentsCheck {
    private static void near(double a, double b, String what) {
        if (Math.abs(a - b) > 1e-3) throw new AssertionError(what + " " + a + " vs " + b);
    }

    public static void main(String[] args) throws Exception {
        var sky = new CustomSkyboxRenderer() {
            @Override
            public boolean renderSky(LevelRenderState state, SkyRenderState skyState, Matrix4fc modelView, GpuBufferSlice transform) {
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
                var vertices = WorldFrame.DEVICE.createBuffer(() -> "sky", GpuBuffer.USAGE_VERTEX, mesh);
                var pass = new RecordingRenderPass(WorldFrame.DEVICE);
                pass.setPipeline(new RecordingPipeline(RenderPipelines.DEBUG_QUADS));
                pass.setUniform("DynamicTransforms", transform);
                pass.setVertexBuffer(0, vertices.slice());
                pass.draw(4, 1, 0, 0);
                return true;
            }
        };
        var clouds = new CustomCloudsRenderer() {
            @Override
            public boolean renderClouds(LevelRenderState state, CloudStatus status, Matrix4fc modelView, RenderPass pass) {
                return true;
            }
        };
        var weather = new CustomWeatherEffectRenderer() {
            @Override
            public boolean renderSnowAndRain(LevelRenderState state, WeatherRenderState weatherState, Vec3 cam, RenderPass pass) {
                return true;
            }

            @Override
            public boolean tickRain(net.minecraft.client.multiplayer.ClientLevel level, long ticks, net.minecraft.client.Camera camera) {
                return true;
            }
        };
        var renderers = new Environments.Renderers(sky, clouds, weather);
        var view = new Matrix4f().rotationY(0.7f).rotateX(-0.3f);
        var inputs = new Environments.Inputs(null, null, null, new Vec3(10, 64, -3), view, CloudStatus.FANCY, 0xFFFFFFFF, 192.0f, 12, false);
        var out = new WorldFrame.Recording(new ArrayList<>(), new LinkedHashMap<>());
        var pieces = new ArrayList<WorldFrame.Piece>();
        int bits = Environments.record(renderers, inputs, new WorldFrame.Cam(10, 64, -3, 30, 10, 70, 512), out, pieces);
        if (bits != (Environments.SKY | Environments.CLOUDS | Environments.WEATHER)) throw new AssertionError("bits " + bits);
        if (pieces.size() != 1) throw new AssertionError("pieces " + pieces.size());
        var piece = pieces.get(0);
        if (piece.pass() != WorldFrame.SKY || (piece.flags() & WorldFrame.ANCHORED) == 0) throw new AssertionError("pass " + piece.pass() + " flags " + piece.flags());
        var mesh = out.meshes().get(piece.mesh());
        if (mesh.quads().size() != 1) throw new AssertionError("quads " + mesh.quads().size());
        var quad = mesh.quads().get(0);
        if (quad.color()[0] != 0xFF00FF00) throw new AssertionError("color " + Integer.toHexString(quad.color()[0]));
        double[] sum = new double[3];
        for (int k = 0; k < 4; k++) {
            for (int axis = 0; axis < 3; axis++) {
                double at = piece.origin()[axis] + piece.linear()[axis * 3] * quad.xyzuv()[k * 5] + piece.linear()[axis * 3 + 1] * quad.xyzuv()[k * 5 + 1] + piece.linear()[axis * 3 + 2] * quad.xyzuv()[k * 5 + 2];
                sum[axis] += at / 4;
            }
        }
        near(sum[0], 0, "x");
        near(sum[1], 100, "y");
        near(sum[2], 0, "z");
        if (Environments.tick(renderers, null, null) != Environments.RAIN_TICK) throw new AssertionError("the rain tick");
        if (Environments.tick(new Environments.Renderers(sky, clouds, null), null, null) != 0) throw new AssertionError("a tick without a weather renderer");
        var none = Environments.record(new Environments.Renderers(null, clouds, null), inputs, new WorldFrame.Cam(0, 0, 0, 0, 0, 70, 512), new WorldFrame.Recording(new ArrayList<>(), new LinkedHashMap<>()), new ArrayList<>());
        if (none != Environments.CLOUDS) throw new AssertionError("clouds alone " + none);
        System.out.println("EnvironmentsCheck ok");
    }
}
