package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.boot.mixin.CameraAccess;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.world.phys.Vec3;
import net.neoforged.bus.api.Event;
import net.neoforged.neoforge.client.ClientHooks;
import net.neoforged.neoforge.client.event.CalculateDetachedCameraDistanceEvent;
import net.neoforged.neoforge.client.event.CalculatePlayerTurnEvent;
import net.neoforged.neoforge.client.event.ViewportEvent;
import net.neoforged.neoforge.common.NeoForge;

/** The view events (camera angles, detached distance, field of view, mouse turn), run on the window's camera values and answered as what the mods changed. */
final class ViewFrame {
    private ViewFrame() {}

    /** The window's values this frame, as `Input` 36 and `client_view_frame` carry them. */
    record Inputs(float partial, double x, double y, double z, float yaw, float pitch, byte mode, boolean minecart, float[] detached, float fov, float hudFov,
            double sensitivity, boolean smooth, boolean grabbed) {}

    static final int ANGLES = 1, DISTANCE = 2, FOV = 4, HUD_FOV = 8, TURN = 16;

    private static final List<Class<? extends Event>> EVENTS = List.of(ViewportEvent.ComputeCameraAngles.class, CalculateDetachedCameraDistanceEvent.class,
            ViewportEvent.ComputeFov.class, CalculatePlayerTurnEvent.class);

    /** Whether a loaded mod listens to a view event; decided once when the mods have loaded. Without one nothing runs. */
    static volatile boolean needed;
    /** The newest inputs the window sent. */
    static Inputs last;

    /** Called once the mods have loaded. */
    static void decide() {
        needed = EVENTS.stream().anyMatch(event -> Listeners.listened(NeoForge.EVENT_BUS, event));
        if (needed && System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: view events heard by the mods; the camera values run through them");
    }

    static Inputs read(DataInputStream in) throws IOException {
        float partial = in.readFloat();
        double x = in.readDouble(), y = in.readDouble(), z = in.readDouble();
        float yaw = in.readFloat(), pitch = in.readFloat();
        byte mode = in.readByte();
        boolean minecart = in.readBoolean();
        float[] detached = null;
        if (mode != 0) detached = new float[] {in.readFloat(), in.readFloat(), in.readFloat(), in.readFloat()};
        return new Inputs(partial, x, y, z, yaw, pitch, mode, minecart, detached, in.readFloat(), in.readFloat(), in.readDouble(), in.readBoolean(), in.readBoolean());
    }

    /** The camera as a frame's listeners find it: where the window put it, looking as the entity does; no `alignWithEntity`, no `update`. */
    static void fillCamera(Inputs in) {
        var minecraft = Minecraft.getInstance();
        var camera = (CameraAccess) minecraft.gameRenderer.mainCamera();
        if (minecraft.gameRenderer.mainCamera().entity() == null) minecraft.gameRenderer.mainCamera().setEntity(minecraft.player);
        camera.horizon$position(new Vec3(in.x(), in.y(), in.z()));
        camera.horizon$rotation3(in.yaw(), in.pitch(), 0.0F);
        camera.horizon$detached(in.mode() != 0);
        camera.horizon$initialized(true);
    }

    /** The events in vanilla's frame order, then what changed: `int mask`, then in bit order the angles (3 floats), distance, fov, hud fov, and the turn (sensitivity, smooth camera). */
    static byte[] take(Inputs in) throws IOException {
        var minecraft = Minecraft.getInstance();
        var out = new ByteArrayOutputStream();
        var data = new DataOutputStream(out);
        int mask = 0;
        double sensitivity = in.sensitivity();
        boolean smooth = in.smooth();
        if (in.grabbed()) {
            var turn = ClientHooks.getTurnPlayerValues(in.sensitivity(), in.smooth());
            sensitivity = turn.getMouseSensitivity();
            smooth = turn.getCinematicCameraEnabled();
            if (Double.compare(sensitivity, in.sensitivity()) != 0 || smooth != in.smooth()) mask |= TURN;
        }
        fillCamera(in);
        var camera = minecraft.gameRenderer.mainCamera();
        float[] angles = {in.yaw(), in.pitch(), 0.0F};
        if (!in.minecart()) {
            var event = new ViewportEvent.ComputeCameraAngles(camera, in.partial(), in.yaw(), in.pitch(), 0.0F);
            NeoForge.EVENT_BUS.post(event);
            angles = new float[] {event.getYaw(), event.getPitch(), event.getRoll()};
            ((CameraAccess) camera).horizon$rotation3(angles[0], angles[1], angles[2]);
            if (angles[0] != in.yaw() || angles[1] != in.pitch() || angles[2] != 0.0F) mask |= ANGLES;
        }
        float distance = 0.0F;
        if (in.mode() != 0) {
            float[] d = in.detached();
            distance = ClientHooks.getDetachedCameraDistance(camera, in.mode() == 2, d[0], d[1], d[2], d[3]);
            if (distance != Math.max(d[0] * d[1], d[2] * d[3])) mask |= DISTANCE;
        }
        float fov = ClientHooks.getFieldOfView(minecraft.gameRenderer, camera, in.partial(), in.fov());
        float hudFov = ClientHooks.getFieldOfView(minecraft.gameRenderer, camera, in.partial(), in.hudFov());
        ((CameraAccess) camera).horizon$fov(fov);
        ((CameraAccess) camera).horizon$hudFov(hudFov);
        if (fov != in.fov()) mask |= FOV;
        if (hudFov != in.hudFov()) mask |= HUD_FOV;
        data.writeInt(mask);
        if ((mask & ANGLES) != 0) for (float a : angles) data.writeFloat(a);
        if ((mask & DISTANCE) != 0) data.writeFloat(distance);
        if ((mask & FOV) != 0) data.writeFloat(fov);
        if ((mask & HUD_FOV) != 0) data.writeFloat(hudFov);
        if ((mask & TURN) != 0) {
            data.writeDouble(sensitivity);
            data.writeBoolean(smooth);
        }
        return out.toByteArray();
    }

    // --- Animation: `ClientBridge.noted`'s rule

    /** True while the view is changing: the window also asks for frames between the tick's firings. */
    static boolean animating;
    private static byte[] lastOutputs;
    private static int ticksSinceFrame;

    /** A firing of either source: differing from the one before it means the view is changing, two alike end it. */
    static void noted(byte[] outputs, float realtimeDeltaTicks) {
        boolean was = animating;
        boolean identical = java.util.Arrays.equals(outputs, lastOutputs);
        if (!identical) animating = true;
        else if (HudAnimation.ends(true, realtimeDeltaTicks)) animating = false;
        lastOutputs = outputs;
        if (animating && !was) ticksSinceFrame = 0;
    }

    /** A tick's firing: with no window frame for two ticks the window isn't serving them, and the tick's own carry the view. */
    static void tickEnded() {
        if (animating && ++ticksSinceFrame > 2) animating = false;
    }

    /** The tick's own firing, for the status: null when no mod listens or the window has sent no inputs. */
    static byte[] tick(float realtimeDeltaTicks) throws IOException {
        Inputs in = last;
        if (!needed || in == null) return null;
        byte[] outputs = take(in);
        noted(outputs, realtimeDeltaTicks);
        tickEnded();
        return outputs;
    }

    private static byte[] lastReplied;

    /** One window frame while the view changes: answers whether it still animates, whether the outputs differ from the last reply, then the outputs. */
    static byte[] frame(DataInputStream in, float realtimeDeltaTicks) throws IOException {
        Inputs inputs = read(in);
        last = inputs;
        byte[] outputs = needed ? take(inputs) : new byte[] {0, 0, 0, 0};
        ticksSinceFrame = 0;
        noted(outputs, realtimeDeltaTicks);
        boolean changed = !java.util.Arrays.equals(outputs, lastReplied);
        lastReplied = outputs;
        var out = new ByteArrayOutputStream();
        var data = new DataOutputStream(out);
        data.writeBoolean(animating);
        data.writeBoolean(changed);
        if (changed) data.write(outputs);
        return out.toByteArray();
    }
}
