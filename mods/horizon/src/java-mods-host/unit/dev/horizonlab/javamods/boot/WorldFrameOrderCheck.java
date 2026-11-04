package dev.horizonlab.javamods.boot;

import java.util.ArrayList;
import java.util.List;
import net.neoforged.neoforge.client.event.ExtractLevelRenderStateEvent;
import net.neoforged.neoforge.client.event.PrepareRenderBuffersEvent;
import net.neoforged.neoforge.client.event.RenderLevelStageEvent;
import net.neoforged.neoforge.client.event.SubmitCustomGeometryEvent;
import net.neoforged.neoforge.common.NeoForge;
import org.joml.Matrix4f;

/** The world frame's events fire in the real client's order, with the stage each draw is tagged by, and OIT and the sky only when the frame has them. */
public final class WorldFrameOrderCheck {
    private static final List<String> SEEN = new ArrayList<>();

    private static <E extends net.neoforged.bus.api.Event> void listen(Class<E> type, String name) {
        NeoForge.EVENT_BUS.addListener(type, event -> SEEN.add(name + "@" + WorldFrame.stage));
    }

    private static List<String> run(boolean sky, boolean oit) {
        SEEN.clear();
        var frame = new WorldFrame.Frame(null, null, null, null, null, null, null, null, null, new Matrix4f(), List.of(), new RecordingRenderPass(new RecordingDevice()), sky, oit);
        WorldFrame.fire(frame, (what, event) -> NeoForge.EVENT_BUS.post(event));
        return new ArrayList<>(SEEN);
    }

    public static void main(String[] args) {
        listen(ExtractLevelRenderStateEvent.class, "Extract");
        listen(SubmitCustomGeometryEvent.class, "Submit");
        listen(PrepareRenderBuffersEvent.class, "Prepare");
        listen(RenderLevelStageEvent.AfterSky.class, "AfterSky");
        listen(RenderLevelStageEvent.AfterOpaqueBlocks.class, "AfterOpaqueBlocks");
        listen(RenderLevelStageEvent.AfterOpaqueFeatures.class, "AfterOpaqueFeatures");
        listen(RenderLevelStageEvent.AfterTranslucentFeatures.class, "AfterTranslucentFeatures");
        listen(RenderLevelStageEvent.AfterTranslucentBlocks.class, "AfterTranslucentBlocks");
        listen(RenderLevelStageEvent.AfterTranslucentParticles.class, "AfterTranslucentParticles");
        listen(RenderLevelStageEvent.AfterWeather.class, "AfterWeather");
        listen(RenderLevelStageEvent.AfterLevel.class, "AfterLevel");
        listen(RenderLevelStageEvent.OitTranslucent.class, "Oit");
        NeoForge.EVENT_BUS.start();
        var base = List.of("Extract@0", "Submit@0", "Prepare@0", "AfterSky@1", "AfterOpaqueBlocks@2", "AfterOpaqueFeatures@3", "AfterTranslucentFeatures@4", "AfterTranslucentBlocks@5",
                "AfterTranslucentParticles@6", "AfterWeather@7", "AfterLevel@8");
        var plain = run(true, false);
        if (!plain.equals(base)) throw new AssertionError(plain);
        var noSky = run(false, false);
        var expected = new ArrayList<>(base);
        expected.remove("AfterSky@1");
        if (!noSky.equals(expected)) throw new AssertionError(noSky);
        var improved = run(true, true);
        var withOit = new ArrayList<>(base);
        withOit.addAll(List.of("Oit@9", "Oit@9", "Oit@9"));
        if (!improved.equals(withOit)) throw new AssertionError(improved);
        System.out.println("WorldFrameOrderCheck ok");
    }
}
