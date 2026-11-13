package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.vertex.VertexConsumer;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.particle.SingleQuadParticle;
import net.minecraft.client.renderer.state.level.QuadParticleRenderState;
import org.joml.Quaternionf;

/** The corners vanilla's `QuadParticleRenderState.renderRotatedQuad` writes, which the Rust drawing of recorded quads must match. */
public final class ParticleCornersCheck {
    private static final class Capture implements VertexConsumer {
        final List<float[]> corners = new ArrayList<>();

        @Override
        public VertexConsumer addVertex(float x, float y, float z) {
            corners.add(new float[] {x, y, z});
            return this;
        }

        @Override
        public VertexConsumer setColor(int r, int g, int b, int a) {
            return this;
        }

        @Override
        public VertexConsumer setColor(int color) {
            return this;
        }

        @Override
        public VertexConsumer setUv(float u, float v) {
            return this;
        }

        @Override
        public VertexConsumer setUv1(int u, int v) {
            return this;
        }

        @Override
        public VertexConsumer setUv2(int u, int v) {
            return this;
        }

        @Override
        public VertexConsumer setUv3(float u, float v) {
            return this;
        }

        @Override
        public VertexConsumer setNormal(float x, float y, float z) {
            return this;
        }

        @Override
        public VertexConsumer setLineWidth(float width) {
            return this;
        }

        @Override
        public VertexConsumer setLight(int light) {
            return this;
        }
    }

    private static List<float[]> corners(Quaternionf q, float size, float x, float y, float z) {
        var state = new QuadParticleRenderState();
        state.add(SingleQuadParticle.Layer.OPAQUE_TERRAIN, x, y, z, q.x, q.y, q.z, q.w, size, 0, 1, 0, 1, -1, 0);
        var capture = new Capture();
        state.buildLayer(SingleQuadParticle.Layer.OPAQUE_TERRAIN, capture);
        return capture.corners;
    }

    private static void print(String name, List<float[]> corners) {
        var line = new StringBuilder(name);
        for (var c : corners) line.append(String.format(" (%.6f, %.6f, %.6f)", c[0], c[1], c[2]));
        System.out.println(line);
    }

    public static void main(String[] args) {
        var one = corners(new Quaternionf().rotationYXZ(0, 0, 0), 0.5f, 1, 2, 3);
        float[][] want = {{1.5f, 1.5f, 3}, {1.5f, 2.5f, 3}, {0.5f, 2.5f, 3}, {0.5f, 1.5f, 3}};
        if (one.size() != 4) throw new AssertionError("four corners");
        for (int i = 0; i < 4; i++) {
            for (int k = 0; k < 3; k++) {
                if (Math.abs(one.get(i)[k] - want[i][k]) > 1e-5f) throw new AssertionError("case 1 corner " + i);
            }
        }
        print("case1", one);
        var camera = new Quaternionf().rotationYXZ((float) (Math.PI - Math.toRadians(90)), (float) -Math.toRadians(30), 0);
        print("case2", corners(new Quaternionf(camera).rotateZ(0.3f), 0.25f, 0, 1, -2));
        var yaw = new Quaternionf(0, camera.y, 0, camera.w);
        print("case3", corners(yaw.rotateZ(0.3f), 0.25f, 0, 1, -2));
    }
}
