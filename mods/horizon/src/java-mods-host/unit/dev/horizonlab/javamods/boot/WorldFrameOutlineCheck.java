package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import net.minecraft.client.renderer.rendertype.RenderTypes;
import net.minecraft.client.renderer.state.level.BlockOutlineRenderState;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.Shapes;
import net.neoforged.neoforge.client.CustomBlockOutlineRenderer;
import net.neoforged.neoforge.client.event.ExtractBlockOutlineRenderStateEvent;
import net.neoforged.neoforge.client.event.ExtractLevelRenderStateEvent;
import net.neoforged.neoforge.client.event.SubmitCustomGeometryEvent;
import net.neoforged.neoforge.common.NeoForge;
import org.joml.Matrix4f;

/** The block outline's steps, as the patched jar's bytecode orders them: extraction before `ExtractLevelRenderStateEvent`, the renderers before `SubmitCustomGeometryEvent` and only when the outline renders; every renderer runs; the answer, the hit's wire form and the outline tag on submitted pieces. */
public final class WorldFrameOutlineCheck {
    private static final List<String> SEEN = new ArrayList<>();

    private static List<String> order(boolean renderOutline) {
        SEEN.clear();
        var frame = new WorldFrame.Frame(null, null, null, null, null, null, null, null, null, new Matrix4f(), List.of(), new RecordingRenderPass(new RecordingDevice()), false, false, renderOutline);
        WorldFrame.fire(frame, (what, event) -> NeoForge.EVENT_BUS.post(event), () -> {}, new WorldFrame.Outline() {
            @Override
            public void extract(WorldFrame.Frame f) {
                SEEN.add("Outline");
            }

            @Override
            public boolean submit(WorldFrame.Frame f) {
                SEEN.add("OutlineSubmit");
                return false;
            }
        });
        return new ArrayList<>(SEEN);
    }

    private static CustomBlockOutlineRenderer renderer(boolean answer, int[] calls, int at) {
        return (state, collector, pose, level) -> {
            calls[at]++;
            return answer;
        };
    }

    private static BlockOutlineRenderState state(List<CustomBlockOutlineRenderer> renderers) {
        return new BlockOutlineRenderState(BlockPos.ZERO, false, false, Shapes.block(), renderers);
    }

    private static WorldFrame.Hit roundTrip(WorldFrame.Hit hit) throws Exception {
        var bytes = new ByteArrayOutputStream();
        WorldFrame.writeHit(new DataOutputStream(bytes), hit);
        return WorldFrame.readHit(new DataInputStream(new ByteArrayInputStream(bytes.toByteArray())));
    }

    private static void line(SubmitCapture capture, int x) {
        var poses = new PoseStack();
        poses.translate(x, 64, 0);
        capture.submitCustomGeometry(poses, RenderTypes.lines(), (pose, consumer) -> {
            for (float[] end : new float[][] {{0, 0, 0}, {1, 0, 0}}) {
                VertexConsumer v = consumer.addVertex(pose, end[0], end[1], end[2]).setColor(-1);
                v.setNormal(pose, 1, 0, 0).setLineWidth(2.0F);
            }
        });
    }

    private static void expect(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) throws Exception {
        NeoForge.EVENT_BUS.addListener(ExtractLevelRenderStateEvent.class, event -> SEEN.add("Extract"));
        NeoForge.EVENT_BUS.addListener(SubmitCustomGeometryEvent.class, event -> SEEN.add("Submit"));
        NeoForge.EVENT_BUS.start();
        var rendered = order(true);
        expect(rendered.subList(0, 4).equals(List.of("Outline", "Extract", "OutlineSubmit", "Submit")), "order with the outline rendered " + rendered);
        var hidden = order(false);
        expect(hidden.subList(0, 3).equals(List.of("Outline", "Extract", "Submit")), "order without it " + hidden);

        int[] calls = new int[3];
        var three = state(List.of(renderer(false, calls, 0), renderer(true, calls, 1), renderer(false, calls, 2)));
        expect(WorldFrame.customOutline(three, null, null, null), "a true among three replaces");
        expect(calls[0] == 1 && calls[1] == 1 && calls[2] == 1, "every renderer ran once " + java.util.Arrays.toString(calls));
        expect(!WorldFrame.customOutline(state(List.of(renderer(false, new int[1], 0), renderer(false, new int[1], 0))), null, null, null), "two falses keep vanilla");
        expect(!WorldFrame.customOutline(state(List.of()), null, null, null), "no renderer keeps vanilla");

        var canceled = new ExtractBlockOutlineRenderStateEvent(null, null, BlockPos.ZERO, null, null, null, false, false, null, null);
        canceled.setCanceled(true);
        var some = state(List.of());
        expect(WorldFrame.answerKind(null, null, false) == 0, "none");
        expect(WorldFrame.answerKind(null, canceled, false) == 1, "canceled");
        expect(WorldFrame.answerKind(some, null, false) == 2, "vanilla");
        expect(WorldFrame.answerKind(some, null, true) == 3, "replaced");

        var block = new WorldFrame.Hit(new BlockPos(1, 64, -2), Direction.UP, new Vec3(1.5, 65.0, -1.5), true, false);
        expect(roundTrip(block).equals(block), "a block hit's wire form");
        expect(roundTrip(WorldFrame.NONE).equals(WorldFrame.NONE), "no hit's wire form");
        var none = new ByteArrayOutputStream();
        WorldFrame.writeHit(new DataOutputStream(none), WorldFrame.NONE);
        expect(java.util.Arrays.equals(none.toByteArray(), new byte[] {0, 0, 0}), "no hit is three zero bytes");

        var capture = new SubmitCapture(true);
        line(capture, 0);
        WorldFrame.outlining = true;
        try {
            line(capture, 4);
        } finally {
            WorldFrame.outlining = false;
        }
        var out = new WorldFrame.Recording(new ArrayList<>(), new LinkedHashMap<>());
        var pieces = new ArrayList<WorldFrame.Piece>();
        WorldFrame.submitted(capture, new WorldFrame.Cam(0, 0, 0, 0, 0, 70, 512), out, pieces);
        expect(pieces.size() == 2, "pieces " + pieces.size());
        expect((pieces.get(0).flags() & 8) == 0, "the plain line is not an outline");
        expect((pieces.get(1).flags() & 8) == 8, "the outline renderer's line is");
        System.out.println("WorldFrameOutlineCheck ok");
    }
}
