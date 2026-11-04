package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import net.minecraft.client.renderer.rendertype.RenderTypes;

/** A box submitted at a fixed place records one translucent piece of six quads and one of twelve segments there, and the same bytes from a camera one block away. */
public final class WorldFrameSubmitCheck {
    private static final float[][] CORNERS = {{0, 0, 0}, {1, 0, 0}, {1, 1, 0}, {0, 1, 0}, {0, 0, 1}, {1, 0, 1}, {1, 1, 1}, {0, 1, 1}};
    private static final int[][] FACES = {{0, 1, 2, 3}, {5, 4, 7, 6}, {4, 0, 3, 7}, {1, 5, 6, 2}, {3, 2, 6, 7}, {4, 5, 1, 0}};
    private static final int[][] EDGES = {{0, 1}, {1, 2}, {2, 3}, {3, 0}, {4, 5}, {5, 6}, {6, 7}, {7, 4}, {0, 4}, {1, 5}, {2, 6}, {3, 7}};

    private static WorldFrame.Recording record(WorldFrame.Cam cam) {
        var capture = new SubmitCapture(true);
        var poses = new PoseStack();
        poses.translate(8 - cam.x(), 100 - cam.y(), 8 - cam.z());
        capture.submitCustomGeometry(poses, RenderTypes.debugQuads(), (pose, consumer) -> {
            for (int[] face : FACES) for (int c : face) consumer.addVertex(pose, CORNERS[c][0], CORNERS[c][1], CORNERS[c][2]).setColor(0x80FF0000);
        });
        capture.submitCustomGeometry(poses, RenderTypes.lines(), (pose, consumer) -> {
            for (int[] edge : EDGES) {
                for (int end : edge) {
                    VertexConsumer v = consumer.addVertex(pose, CORNERS[end][0], CORNERS[end][1], CORNERS[end][2]).setColor(-1);
                    v.setNormal(pose, 1, 0, 0).setLineWidth(2.0F);
                }
            }
        });
        var out = new WorldFrame.Recording(new ArrayList<>(), new LinkedHashMap<>());
        var pieces = new ArrayList<WorldFrame.Piece>();
        WorldFrame.submitted(capture, cam, out, pieces);
        return new WorldFrame.Recording(pieces, out.meshes());
    }

    public static void main(String[] args) throws Exception {
        var a = record(new WorldFrame.Cam(3.5, 101.25, 20.75, 10, 20, 70, 512));
        if (a.pieces().size() != 2) throw new AssertionError("pieces " + a.pieces().size());
        var boxes = a.pieces().get(0);
        var wires = a.pieces().get(1);
        if (boxes.pass() != WorldFrame.TRANSLUCENT || a.meshes().get(boxes.mesh()).quads().size() != 6) throw new AssertionError("the box");
        if (wires.pass() != WorldFrame.LINES || a.meshes().get(wires.mesh()).lines().size() != 12) throw new AssertionError("the wire box " + a.meshes().get(wires.mesh()).lines().size());
        if (boxes.origin()[0] != 8 || boxes.origin()[1] != 100 || boxes.origin()[2] != 8) throw new AssertionError("the place");
        var b = record(new WorldFrame.Cam(4.5, 101.25, 20.75, 10, 20, 70, 512));
        if (!java.util.Arrays.equals(WorldFrame.bytes(a.pieces()), WorldFrame.bytes(b.pieces()))) throw new AssertionError("a camera one block away records other bytes");
        System.out.println("WorldFrameSubmitCheck ok");
    }
}
