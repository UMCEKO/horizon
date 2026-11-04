package dev.horizonlab.javamods.testmod;

import com.mojang.blaze3d.vertex.PoseStack;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.rendertype.RenderTypes;
import net.neoforged.neoforge.client.event.SubmitCustomGeometryEvent;
import net.neoforged.neoforge.common.NeoForge;

/** What a mod draws in the level's frame: a red translucent box and a white wire box at a fixed place, while the wand is held. */
final class ClientWorldBox {
    private ClientWorldBox() {}

    static final double X = 8, Y = 100, Z = 8;

    static void register() {
        NeoForge.EVENT_BUS.addListener((SubmitCustomGeometryEvent event) -> {
            var player = Minecraft.getInstance().player;
            if (player == null || !(player.getMainHandItem().getItem() instanceof TestMod.Wand)) return;
            var camera = event.getLevelRenderState().cameraRenderState.pos;
            var poses = event.getPoseStack();
            poses.pushPose();
            poses.translate(X - camera.x, Y - camera.y, Z - camera.z);
            event.getSubmitNodeCollector().submitCustomGeometry(poses, RenderTypes.debugQuads(), (pose, consumer) -> faces(pose, consumer));
            event.getSubmitNodeCollector().submitCustomGeometry(poses, RenderTypes.lines(), (pose, consumer) -> edges(pose, consumer));
            poses.popPose();
        });
    }

    private static final float[][] CORNERS = {{0, 0, 0}, {1, 0, 0}, {1, 1, 0}, {0, 1, 0}, {0, 0, 1}, {1, 0, 1}, {1, 1, 1}, {0, 1, 1}};
    private static final int[][] FACES = {{0, 1, 2, 3}, {5, 4, 7, 6}, {4, 0, 3, 7}, {1, 5, 6, 2}, {3, 2, 6, 7}, {4, 5, 1, 0}};
    private static final int[][] EDGES = {{0, 1}, {1, 2}, {2, 3}, {3, 0}, {4, 5}, {5, 6}, {6, 7}, {7, 4}, {0, 4}, {1, 5}, {2, 6}, {3, 7}};

    private static void faces(PoseStack.Pose pose, com.mojang.blaze3d.vertex.VertexConsumer consumer) {
        for (int[] face : FACES) for (int corner : face) consumer.addVertex(pose, CORNERS[corner][0], CORNERS[corner][1], CORNERS[corner][2]).setColor(0x80FF0000);
    }

    private static void edges(PoseStack.Pose pose, com.mojang.blaze3d.vertex.VertexConsumer consumer) {
        for (int[] edge : EDGES) {
            float[] a = CORNERS[edge[0]], b = CORNERS[edge[1]];
            float nx = b[0] - a[0], ny = b[1] - a[1], nz = b[2] - a[2];
            consumer.addVertex(pose, a[0], a[1], a[2]).setColor(-1).setNormal(pose, nx, ny, nz).setLineWidth(2.0F);
            consumer.addVertex(pose, b[0], b[1], b[2]).setColor(-1).setNormal(pose, nx, ny, nz).setLineWidth(2.0F);
        }
    }
}
