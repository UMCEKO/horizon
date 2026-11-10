package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.vertex.BufferBuilder;
import com.mojang.blaze3d.vertex.ByteBufferBuilder;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.renderpearl.api.pipeline.IndexType;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import net.minecraft.client.renderer.DynamicGpuData;
import net.minecraft.client.renderer.RenderPipelines;
import org.joml.Matrix4f;
import org.joml.Vector3f;
import org.joml.Vector4f;

/** A QUADS mesh in the block format, uploaded and drawn through the recording pass with the quad index buffer, decodes to its quads, placed by the transform it was drawn under; pipelines classify by their state. */
public final class RecordingDeviceCheck {
    private static void near(double a, double b, String what) {
        if (Math.abs(a - b) > 1e-3) throw new AssertionError(what + " " + a + " vs " + b);
    }

    public static void main(String[] args) throws Exception {
        var device = new RecordingDevice();
        var vertices = new float[][] {{0, 0, 0}, {1, 0, 0}, {1, 1, 0}, {0, 1, 0}, {0, 0, 1}, {1, 0, 1}, {1, 1, 1}, {0, 1, 1}};
        ByteBuffer mesh;
        try (var builder = new ByteBufferBuilder(1024)) {
            var buffer = new BufferBuilder(builder, PrimitiveTopology.QUADS, DefaultVertexFormat.BLOCK);
            for (int k = 0; k < vertices.length; k++) {
                buffer.addVertex(vertices[k][0], vertices[k][1], vertices[k][2]).setColor(0x80FF4020).setUv(k * 0.25f, 0.5f).setUv2(240, 96);
            }
            try (var data = buffer.buildOrThrow()) {
                mesh = ByteBuffer.allocate(data.vertexBuffer().remaining()).order(ByteOrder.nativeOrder());
                mesh.put(data.vertexBuffer());
                mesh.flip();
            }
        }
        var indices = ByteBuffer.allocate(12 * 2).order(ByteOrder.nativeOrder());
        for (int q = 0; q < 2; q++) for (int i : new int[] {0, 1, 2, 2, 3, 0}) indices.putShort((short) (q * 4 + i));
        indices.flip();
        var vertexBuffer = device.createBuffer(() -> "v", com.mojang.renderpearl.api.buffers.GpuBuffer.USAGE_VERTEX, mesh);
        var indexBuffer = device.createBuffer(() -> "i", com.mojang.renderpearl.api.buffers.GpuBuffer.USAGE_INDEX, indices);
        var transform = new DynamicGpuData.Transform(new Matrix4f().translate(5, 6, 7).scale(2), new Vector4f(1, 0.5f, 1, 1), new Vector3f(1, 0, 0), new Matrix4f());
        var pass = new RecordingRenderPass(device);
        pass.setPipeline(new RecordingPipeline(RenderPipelines.TRANSLUCENT_BLOCK));
        pass.setUniform("DynamicTransforms", new RecordingTransform(device, transform).slice());
        pass.setVertexBuffer(0, vertexBuffer.slice());
        pass.setIndexBuffer(indexBuffer, IndexType.SHORT);
        pass.drawIndexed(12, 1, 0, 0, 0);
        if (device.draws.size() != 1) throw new AssertionError("draws " + device.draws.size());
        var draw = device.draws.get(0);
        var decoded = WorldFrame.decode(draw, 1);
        if (decoded.quads().size() != 2) throw new AssertionError("quads " + decoded.quads().size());
        for (int q = 0; q < 2; q++) {
            var quad = decoded.quads().get(q);
            for (int k = 0; k < 4; k++) {
                for (int axis = 0; axis < 3; axis++) near(quad.xyzuv()[k * 5 + axis], vertices[q * 4 + k][axis], "position " + q + "," + k);
                near(quad.xyzuv()[k * 5 + 3], 0.5, "untextured geometry takes the white texel");
                if (quad.color()[k] != 0x80FF4020) throw new AssertionError("color " + Integer.toHexString(quad.color()[k]));
                if (quad.light()[k] != (240 | 96 << 16)) throw new AssertionError("light " + quad.light()[k]);
            }
        }
        // The world position of a corner is the transform on it, whatever the camera and its rotation are.
        var viewRotation = new Matrix4f().rotationY(0.7f).rotateX(-0.3f);
        double cx = 100.25, cy = 64.5, cz = -30.75;
        var modelView = new Matrix4f(viewRotation).translate((float) -cx, (float) -cy, (float) -cz).translate(105.25f, 70.5f, -23.75f).scale(2);
        var placed = WorldFrame.toWorld(new DynamicGpuData.Transform(modelView, new Vector4f(1, 1, 1, 1), new Vector3f(0.5f, 0, 0), new Matrix4f()), viewRotation, cx, cy, cz);
        var corner = placed.transformPosition(new org.joml.Vector3d(1, 1, 0));
        near(corner.x, 105.25 + 2 * (1 + 0.5), "world x");
        near(corner.y, 70.5 + 2, "world y");
        near(corner.z, -23.75, "world z");

        // The same bytes under another version are another mesh.
        var a = device.draws.get(0);
        indexBuffer.map(0, 4, false, true).close();
        pass.setIndexBuffer(indexBuffer, IndexType.SHORT);
        pass.drawIndexed(12, 1, 0, 0, 0);
        var b = device.draws.get(1);
        if (a.indexVersion() == b.indexVersion()) throw new AssertionError("version");

        // A pipeline is classified by its state, never its name.
        expect(RenderPipelines.LINES, WorldFrame.SUBMIT, WorldFrame.LINES);
        expect(RenderPipelines.SOLID_BLOCK, WorldFrame.SUBMIT, WorldFrame.OPAQUE);
        expect(RenderPipelines.DEBUG_QUADS, WorldFrame.SUBMIT, WorldFrame.TRANSLUCENT);
        expect(RenderPipelines.DEBUG_QUADS, WorldFrame.AFTER_TRANSLUCENT_BLOCKS, WorldFrame.AFTER_TRANSLUCENT);
        expect(RenderPipelines.LIGHTNING, WorldFrame.AFTER_LEVEL, WorldFrame.ADDITIVE);
        expect(RenderPipelines.TEXT_SEE_THROUGH, WorldFrame.SUBMIT, WorldFrame.SEE_THROUGH);
        System.out.println("RecordingDeviceCheck ok");
    }

    private static void expect(com.mojang.renderpearl.api.pipeline.RenderPipeline pipeline, int stage, byte pass) {
        byte got = WorldFrame.classify(pipeline, stage);
        if (got != pass) throw new AssertionError(pipeline + " at " + stage + " -> " + got + ", wanted " + pass);
    }
}
