package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.boot.mixin.ParticleEngineAccess;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import net.minecraft.client.Camera;
import net.minecraft.client.Minecraft;
import net.minecraft.client.particle.ParticleGroup;
import net.minecraft.client.particle.ParticleRenderType;
import net.minecraft.client.particle.QuadParticleGroup;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.culling.Frustum;
import net.minecraft.client.renderer.state.level.CameraRenderState;

/** The groups a mod registered, which draw through the world recording; vanilla's own are recorded as quads or filled only by vanilla code. */
final class ParticleRecorder {
    private static final Set<Class<?>> FAILED = new HashSet<>();

    private ParticleRecorder() {}

    private static boolean vanilla(ParticleRenderType type) {
        return type == ParticleRenderType.SINGLE_QUADS || type == ParticleRenderType.ITEM_PICKUP || type == ParticleRenderType.ELDER_GUARDIANS || type == ParticleRenderType.NO_RENDER;
    }

    /** The mods' groups that hold particles, in the engine's order; a quad group is recorded as quads instead. */
    private static List<ParticleGroup<?>> live() {
        var out = new ArrayList<ParticleGroup<?>>();
        var minecraft = Minecraft.getInstance();
        if (minecraft == null || minecraft.particleEngine == null) return out;
        var access = (ParticleEngineAccess) minecraft.particleEngine;
        var groups = access.horizon$groups();
        for (var type : access.horizon$order()) {
            if (vanilla(type)) continue;
            var group = groups.get(type);
            if (group != null && !group.isEmpty() && !(group instanceof QuadParticleGroup)) out.add(group);
        }
        return out;
    }

    /** Whether a group a mod registered holds particles: the world must be recorded. */
    static boolean groupsLive() {
        return !live().isEmpty();
    }

    /** `ParticlesRenderState.submit` for the mods' groups, in the engine's order, at `LevelRenderer.submitFeatures`' place. */
    static void submitGroups(Frustum frustum, Camera camera, float partial, SubmitNodeCollector collector, CameraRenderState cameraState) {
        for (var group : live()) {
            try {
                var state = group.extractRenderState(frustum, camera, partial);
                state.submit(collector, cameraState);
                state.clear();
            } catch (Throwable t) {
                if (FAILED.add(group.getClass())) {
                    System.err.println("Horizon: a Java mod's particle group failed: " + group.getClass().getName());
                    t.printStackTrace();
                }
            }
        }
    }
}
