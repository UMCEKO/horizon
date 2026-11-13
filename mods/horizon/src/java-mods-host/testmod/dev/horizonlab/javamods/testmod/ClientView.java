package dev.horizonlab.javamods.testmod;

import net.minecraft.client.Minecraft;
import net.minecraft.core.registries.BuiltInRegistries;
import net.neoforged.neoforge.client.event.CalculateDetachedCameraDistanceEvent;
import net.neoforged.neoforge.client.event.CalculatePlayerTurnEvent;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.client.event.ViewportEvent;
import net.neoforged.neoforge.common.NeoForge;

/** The view events: each of the items `tilt`, `zoom`, `slow_mouse` and `far_camera` acts only while held. */
final class ClientView {
    private ClientView() {}

    private static int angles, fovs, ticks;
    private static boolean checkedInputs;

    private static boolean held(String item) {
        var player = Minecraft.getInstance().player;
        return player != null && BuiltInRegistries.ITEM.getKey(player.getMainHandItem().getItem()).equals(net.minecraft.resources.Identifier.parse("horizon_java_test:" + item));
    }

    static void register() {
        NeoForge.EVENT_BUS.addListener((ViewportEvent.ComputeCameraAngles event) -> {
            angles++;
            if (!held("tilt")) return;
            var player = Minecraft.getInstance().player;
            if (!checkedInputs) {
                checkedInputs = true;
                float partial = (float) event.getPartialTick();
                Checks.check("angles_inputs_native", () -> Math.abs(event.getYaw() - player.getViewYRot(partial)) < 1e-3 && Math.abs(event.getPitch() - player.getViewXRot(partial)) < 1e-3);
            }
            event.setRoll(20f);
        });
        NeoForge.EVENT_BUS.addListener((ViewportEvent.ComputeFov event) -> {
            fovs++;
            if (held("zoom")) event.setFOV(event.getFOV() * 0.5f);
        });
        NeoForge.EVENT_BUS.addListener((CalculatePlayerTurnEvent event) -> {
            if (held("slow_mouse")) event.setMouseSensitivity(event.getMouseSensitivity() * 0.5);
        });
        NeoForge.EVENT_BUS.addListener((CalculateDetachedCameraDistanceEvent event) -> {
            if (held("far_camera")) event.setDistance(8f);
        });
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Post event) -> tick());
    }

    /** With nothing held and the camera in first person, the tick's own firing is the only one: once for the angles, twice for the field of view. */
    private static void tick() {
        var minecraft = Minecraft.getInstance();
        boolean idle = minecraft.level != null && minecraft.options.getCameraType().isFirstPerson() && !held("tilt") && !held("zoom") && !held("slow_mouse") && !held("far_camera");
        if (!idle) {
            ticks = 0;
            return;
        }
        if (ticks == 0) angles = fovs = 0;
        if (++ticks == 21) {
            int a = angles, f = fovs;
            Checks.check("view_events_tick_rate", () -> a == 20 && f == 40);
            ticks = 0;
        }
    }
}
