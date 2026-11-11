package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.boot.WorldFrame.Cam;
import dev.horizonlab.javamods.boot.WorldFrame.Piece;
import dev.horizonlab.javamods.boot.WorldFrame.Recording;
import java.util.List;
import java.util.Map;
import net.minecraft.client.CloudStatus;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.DynamicGpuData;
import net.minecraft.client.renderer.SkyRenderer;
import net.minecraft.client.renderer.WeatherEffectRenderer;
import net.minecraft.client.renderer.oit.OitStage;
import net.minecraft.client.renderer.state.level.LevelRenderState;
import net.minecraft.client.renderer.state.level.SkyRenderState;
import net.minecraft.client.renderer.state.level.WeatherRenderState;
import net.minecraft.util.ARGB;
import net.minecraft.world.attribute.EnvironmentAttributes;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.client.CustomCloudsRenderer;
import net.neoforged.neoforge.client.CustomEnvironmentEffectsRendererManager;
import net.neoforged.neoforge.client.CustomSkyboxRenderer;
import net.neoforged.neoforge.client.CustomWeatherEffectRenderer;
import org.joml.Matrix4f;
import org.joml.Matrix4fc;
import org.joml.Vector3f;
import org.joml.Vector4f;

/** Mods' custom sky, cloud and weather renderers: the ones that apply where the camera stands run against the recording device, and what they draw goes to native as camera-anchored pieces. */
public final class Environments {
    private Environments() {}

    /** Bits of the status: what the renderers replaced. */
    public static final int SKY = 1, CLOUDS = 2, WEATHER = 4, RAIN_TICK = 8;

    /** Whether any mod registered a custom renderer: decided once the registry exists. */
    static volatile boolean active;
    private static volatile boolean decided;
    /** What the last recording replaced, as the status carries it. */
    static volatile int bits;
    /** The last recording held a renderer's draws: one more recording clears them. */
    private static boolean recorded;
    private static Renderers current = new Renderers(null, null, null);

    /** The renderers that apply at one camera. */
    record Renderers(CustomSkyboxRenderer sky, CustomCloudsRenderer clouds, CustomWeatherEffectRenderer weather) {
        boolean any() {
            return sky != null || clouds != null || weather != null;
        }
    }

    /** What a recording hands the renderers, as vanilla's level renderer would. */
    record Inputs(LevelRenderState state, SkyRenderState sky, WeatherRenderState weather, Vec3 cam, Matrix4fc viewRotation, CloudStatus cloudStatus, int cloudColor, float cloudHeight, int cloudRange,
            boolean oit) {}

    private static int size(String field) throws ReflectiveOperationException {
        var f = CustomEnvironmentEffectsRendererManager.class.getDeclaredField(field);
        f.setAccessible(true);
        var map = (Map<?, ?>) f.get(null);
        return map == null ? -1 : map.size();
    }

    /** Whether the registry holds a renderer, once the registry exists; logged once. */
    static void decide() {
        if (decided) return;
        try {
            int sky = size("CUSTOM_SKYBOX_RENDERERS"), clouds = size("CUSTOM_CLOUD_RENDERERS"), weather = size("CUSTOM_WEATHER_EFFECT_RENDERERS");
            if (sky < 0 || clouds < 0 || weather < 0) return;
            decided = true;
            active = sky + clouds + weather > 0;
            System.err.println("Horizon: custom environment renderers: sky " + sky + ", clouds " + clouds + ", weather " + weather);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    static Renderers select(net.minecraft.world.level.Level level, Vec3 cam) {
        return new Renderers(CustomEnvironmentEffectsRendererManager.getCustomSkyboxRenderer(level, cam), CustomEnvironmentEffectsRendererManager.getCustomCloudsRenderer(level, cam),
                CustomEnvironmentEffectsRendererManager.getCustomWeatherEffectRenderer(level, cam));
    }

    /** Whether a recording is owed here: a renderer applies at the camera, or the last recording held a renderer's draws. Otherwise the status says nothing is replaced. */
    static boolean wanted(Cam cam) {
        decide();
        if (!active) return false;
        var minecraft = Minecraft.getInstance();
        if (minecraft == null || minecraft.level == null || minecraft.player == null || cam == null) return false;
        current = select(minecraft.level, new Vec3(cam.x(), cam.y(), cam.z()));
        if (current.any() || recorded) return true;
        bits = 0;
        return false;
    }

    /** What a new world or join starts from. */
    static void forget() {
        recorded = false;
        bits = 0;
        current = new Renderers(null, null, null);
    }

    private static void run(String what, Runnable body) {
        try {
            body.run();
        } catch (Throwable t) {
            System.err.println("Horizon: a custom " + what + " renderer threw");
            t.printStackTrace();
        }
    }

    private static void start(int stage) {
        WorldFrame.DEVICE.draws.clear();
        WorldFrame.stage = stage;
    }

    /** The renderers' draws for one camera, appended to a recording as anchored pieces; the bits of what they replaced. */
    static int record(Renderers r, Inputs in, Cam cam, Recording out, List<Piece> pieces) {
        int replaced = 0;
        var view = in.viewRotation();
        if (r.sky() != null) {
            start(WorldFrame.SKY_STAGE);
            var slice = WorldFrame.transform(new DynamicGpuData.Transform(new Matrix4f(view), new Vector4f(1, 1, 1, 1), new Vector3f(), new Matrix4f()));
            boolean[] done = new boolean[1];
            run("skybox", () -> done[0] = r.sky().renderSky(in.state(), in.sky(), view, slice));
            if (done[0]) replaced |= SKY;
            WorldFrame.anchored(cam, view, WorldFrame.SKY, out, pieces);
        }
        if (r.clouds() != null && in.cloudStatus() != CloudStatus.OFF && ARGB.alpha(in.cloudColor()) > 0) {
            start(WorldFrame.CLOUDS_STAGE);
            boolean[] done = new boolean[1];
            run("clouds", () -> {
                r.clouds().prepare(in.state(), in.cam(), in.cloudStatus(), in.cloudColor(), in.cloudHeight(), in.cloudRange(), view);
                if (in.oit()) {
                    for (var stage : OitStage.values()) done[0] |= r.clouds().renderCloudsOit(in.state(), in.cloudStatus(), view, stage, WorldFrame.lightmap(), null);
                } else {
                    done[0] = r.clouds().renderClouds(in.state(), in.cloudStatus(), view, new RecordingRenderPass(WorldFrame.DEVICE));
                }
            });
            if (done[0]) replaced |= CLOUDS;
            WorldFrame.anchored(cam, view, WorldFrame.CLOUDS, out, pieces);
        }
        if (r.weather() != null) {
            start(WorldFrame.WEATHER_STAGE);
            boolean[] done = new boolean[1];
            run("weather", () -> {
                r.weather().prepare(in.state(), in.weather(), in.cam());
                if (in.oit()) {
                    for (var stage : OitStage.values()) done[0] |= r.weather().renderSnowAndRainOit(in.state(), in.weather(), in.cam(), stage, new RecordingRenderPass(WorldFrame.DEVICE));
                } else {
                    done[0] = r.weather().renderSnowAndRain(in.state(), in.weather(), in.cam(), new RecordingRenderPass(WorldFrame.DEVICE));
                }
            });
            if (done[0]) replaced |= WEATHER;
            WorldFrame.anchored(cam, view, WorldFrame.WEATHER, out, pieces);
        }
        WorldFrame.DEVICE.draws.clear();
        return replaced;
    }

    /** The rain tick vanilla's level runs each tick: the bit says the mod's weather renderer did it instead. */
    static int tick(Renderers r, net.minecraft.client.multiplayer.ClientLevel level, net.minecraft.client.Camera camera) {
        if (r.weather() == null) return 0;
        boolean[] done = new boolean[1];
        run("weather", () -> done[0] = r.weather().tickRain(level, level == null ? 0 : level.getGameTime(), camera));
        return done[0] ? RAIN_TICK : 0;
    }

    /** The renderers' part of a world recording: the state vanilla's extraction fills, then their draws; on a tick, the rain tick too. */
    static void record(WorldFrame.Frame f, Cam cam, float partial, boolean tick, Recording out, List<Piece> pieces) throws ReflectiveOperationException {
        var chosen = current;
        var r = new Renderers(f.sky() ? chosen.sky() : null, chosen.clouds(), chosen.weather());
        if (!r.any()) {
            recorded = false;
            bits = 0;
            return;
        }
        var level = f.level();
        var camera = new Vec3(cam.x(), cam.y(), cam.z());
        var state = f.state();
        var probe = f.camera().attributeProbe();
        state.cloudColor = ARGB.colorFromVector4f(probe.getValue(EnvironmentAttributes.CLOUD_COLOR, partial));
        if (ARGB.alpha(state.cloudColor) > 0) state.cloudHeight = probe.getValue(EnvironmentAttributes.CLOUD_HEIGHT, partial);
        if (r.sky() != null) Sprites.unallocated(SkyRenderer.class).extractRenderState(level, partial, f.camera(), state.skyRenderState);
        if (r.weather() != null) {
            state.weatherRenderState.reset();
            Sprites.unallocated(WeatherEffectRenderer.class).extractRenderState(level, partial, camera, state.weatherRenderState);
        }
        var inputs = new Inputs(state, state.skyRenderState, state.weatherRenderState, camera, f.viewRotation(), f.options().cloudStatus, state.cloudColor, state.cloudHeight, f.options().cloudRange,
                f.oit());
        int replaced = record(r, inputs, cam, out, pieces);
        int rain = tick ? tick(r, level, f.camera()) : bits & RAIN_TICK;
        bits = replaced | rain;
        recorded = true;
    }
}
