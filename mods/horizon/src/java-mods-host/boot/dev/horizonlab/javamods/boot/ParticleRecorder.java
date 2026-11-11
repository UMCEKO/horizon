package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.boot.mixin.CameraAccess;
import dev.horizonlab.javamods.boot.mixin.ParticleEngineAccess;
import dev.horizonlab.javamods.boot.mixin.ParticleGroupAccess;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import net.minecraft.client.Camera;
import net.minecraft.client.Minecraft;
import net.minecraft.client.particle.ParticleEngine;
import net.minecraft.client.particle.ParticleRenderType;
import net.minecraft.client.particle.QuadParticleGroup;
import net.minecraft.client.particle.SingleQuadParticle;
import net.minecraft.client.renderer.state.level.QuadParticleRenderState;
import net.minecraft.resources.Identifier;
import net.minecraft.world.phys.Vec3;
import org.joml.Quaternionf;
import org.joml.Vector3f;

/** The Java engine's quad particles after a tick, as camera-free quads at partial 0 and 1, for the window to draw. */
public final class ParticleRecorder {
    private ParticleRecorder() {}

    record Sample(float x, float y, float z, float qx, float qy, float qz, float qw, float size, float u0, float u1, float v0, float v1, int color, int light) {}

    record Rec(int flags, int sprite, Sample a, Sample b) {}

    /** One `QuadParticleRenderState.add` call. */
    private record Entry(SingleQuadParticle.Layer layer, Sample sample) {}

    /** The state a particle extracts into: it keeps the calls in order. */
    private static final class Capture extends QuadParticleRenderState {
        final List<Entry> entries = new ArrayList<>();

        @Override
        public void add(SingleQuadParticle.Layer layer, float x, float y, float z, float xRot, float yRot, float zRot, float wRot, float scale, float u0, float u1, float v0, float v1, int color, int lightCoords) {
            entries.add(new Entry(layer, new Sample(x, y, z, xRot, yRot, zRot, wRot, scale, u0, u1, v0, v1, color, lightCoords)));
        }
    }

    private static final float EPS = 1e-4f;
    private static final Quaternionf PROBE = new Quaternionf().rotationYXZ((float) (Math.PI - Math.toRadians(240)), (float) -Math.toRadians(35), 0);
    private static final Set<String> WARNED = new HashSet<>();
    private static Camera recording;
    private static List<ParticleRenderType> order;
    private static boolean present;

    static void forget() {
        present = false;
    }

    private static boolean near(float a, float b) {
        return Math.abs(a - b) <= EPS;
    }

    private static boolean sameRotation(Quaternionf a, Quaternionf b) {
        var x = new Quaternionf(a).normalize();
        var y = new Quaternionf(b).normalize();
        boolean same = near(x.x, y.x) && near(x.y, y.y) && near(x.z, y.z) && near(x.w, y.w);
        boolean negated = near(x.x, -y.x) && near(x.y, -y.y) && near(x.z, -y.z) && near(x.w, -y.w);
        return same || negated;
    }

    /** How a quad faces the camera, told by its rotation under two cameras: 0 camera, 1 camera yaw, 2 fixed, -1 none of them. */
    static int facing(Quaternionf b, Vector3f bPos, Quaternionf c, Vector3f cPos, Quaternionf probe) {
        if (!near(bPos.x, cPos.x) || !near(bPos.y, cPos.y) || !near(bPos.z, cPos.z)) return -1;
        if (sameRotation(c, new Quaternionf(probe).mul(b))) return 0;
        var yaw = new Quaternionf(0, probe.y, 0, probe.w).normalize();
        if (sameRotation(c, yaw.mul(b))) return 1;
        if (sameRotation(c, b)) return 2;
        return -1;
    }

    static byte[] encode(double[] anchor, List<String> names, List<Rec> quads) {
        try {
            var bytes = new ByteArrayOutputStream();
            var out = new DataOutputStream(bytes);
            out.writeDouble(anchor[0]);
            out.writeDouble(anchor[1]);
            out.writeDouble(anchor[2]);
            out.writeInt(names.size());
            for (String name : names) {
                byte[] utf = name.getBytes(StandardCharsets.UTF_8);
                out.writeInt(utf.length);
                out.write(utf);
            }
            out.writeInt(quads.size());
            for (Rec quad : quads) {
                out.writeByte(quad.flags());
                out.writeInt(quad.sprite());
                sample(out, quad.a());
                sample(out, quad.b());
            }
            return bytes.toByteArray();
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private static void sample(DataOutputStream out, Sample s) throws IOException {
        for (float v : new float[] {s.x(), s.y(), s.z(), s.qx(), s.qy(), s.qz(), s.qw(), s.size(), s.u0(), s.u1(), s.v0(), s.v1()}) out.writeFloat(v);
        out.writeInt(s.color());
        out.writeInt(s.light());
    }

    private static void warnOnce(String key, String message) {
        if (WARNED.add(key)) System.err.println(message);
    }

    @SuppressWarnings("unchecked")
    private static List<ParticleRenderType> order() {
        if (order == null) {
            try {
                var field = ParticleEngine.class.getDeclaredField("RENDER_ORDER");
                field.setAccessible(true);
                order = (List<ParticleRenderType>) field.get(null);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }
        return order;
    }

    private static List<Entry> extract(SingleQuadParticle p, float partial) {
        var capture = new Capture();
        p.extract(capture, recording, partial);
        return capture.entries;
    }

    private static Quaternionf quat(Sample s) {
        return new Quaternionf(s.qx(), s.qy(), s.qz(), s.qw());
    }

    private static Vector3f pos(Sample s) {
        return new Vector3f(s.x(), s.y(), s.z());
    }

    /** A sample's sprite name and its sprite-relative sample. */
    private record Placed(String name, Sample sample) {}

    /** Null when the atlas has no sprite where the quad's UVs are. */
    private static Placed place(Entry entry) {
        var s = entry.sample();
        var atlas = entry.layer().textureAtlasLocation();
        var spot = Sprites.locate(atlas, (s.u0() + s.u1()) / 2, (s.v0() + s.v1()) / 2);
        if (spot == null) {
            warnOnce("atlas " + atlas, "Horizon: a Java particle's quad has no sprite in " + atlas);
            return null;
        }
        var sprite = spot.sprite();
        float du = sprite.getU1() - sprite.getU0(), dv = sprite.getV1() - sprite.getV0();
        var local = new Sample(s.x(), s.y(), s.z(), s.qx(), s.qy(), s.qz(), s.qw(), s.size(), (s.u0() - sprite.getU0()) / du, (s.u1() - sprite.getU0()) / du, (s.v0() - sprite.getV0()) / dv, (s.v1() - sprite.getV0()) / dv, s.color(), s.light());
        return new Placed(SubmitCapture.file(Identifier.parse(spot.file())), local);
    }

    private static int flags(SingleQuadParticle.Layer layer, int facing, int span) {
        return (layer.translucent() ? 1 : 0) | facing << 1 | span << 3;
    }

    /** The quads of one tick: the sprite names they share and the quads themselves. */
    private static final class Tape {
        final List<String> names = new ArrayList<>();
        final Map<String, Integer> index = new HashMap<>();
        final List<Rec> quads = new ArrayList<>();

        int sprite(String name) {
            return index.computeIfAbsent(name, n -> {
                names.add(n);
                return names.size() - 1;
            });
        }

        void whole(SingleQuadParticle.Layer layer, int facing, Placed a, Placed b) {
            quads.add(new Rec(flags(layer, facing, 0), sprite(a.name()), a.sample(), b.sample()));
        }

        void half(SingleQuadParticle.Layer layer, int facing, int span, Placed placed) {
            if (placed != null) quads.add(new Rec(flags(layer, Math.max(facing, 0), span), sprite(placed.name()), placed.sample(), placed.sample()));
        }
    }

    /** The recording of this tick's quad particles for the status; null when there is nothing to say. */
    static byte[] take(WorldFrame.Cam cam) {
        if (cam == null) return null;
        long started = Profile.ON ? System.nanoTime() : 0;
        var minecraft = Minecraft.getInstance();
        var tape = new Tape();
        if (minecraft.particleEngine != null) {
            if (recording == null) recording = new Camera();
            var access = (CameraAccess) recording;
            access.horizon$position(new Vec3(cam.x(), cam.y(), cam.z()));
            var groups = ((ParticleEngineAccess) minecraft.particleEngine).horizon$groups();
            for (var type : order()) {
                if (!(groups.get(type) instanceof QuadParticleGroup group)) continue;
                for (var particle : ((ParticleGroupAccess) group).horizon$particles()) {
                    if (!(particle instanceof SingleQuadParticle p)) continue;
                    try {
                        record(p, cam, access, tape);
                    } catch (Throwable t) {
                        warnOnce("class " + p.getClass().getName(), "Horizon: a Java particle's extract failed: " + p.getClass().getName());
                    }
                }
            }
        }
        if (Profile.ON) Profile.event("particles.record", System.nanoTime() - started);
        boolean any = !tape.quads.isEmpty();
        if (!any && !present) return null;
        present = any;
        return encode(new double[] {cam.x(), cam.y(), cam.z()}, tape.names, tape.quads);
    }

    private static void record(SingleQuadParticle p, WorldFrame.Cam cam, CameraAccess access, Tape tape) {
        access.horizon$rotation(180, 0);
        var a = extract(p, 0);
        var b = extract(p, 1);
        access.horizon$rotation(240, 35);
        var c = extract(p, 1);
        var facing = new int[b.size()];
        boolean fixed = false;
        for (int i = 0; i < facing.length; i++) {
            facing[i] = c.size() == b.size() ? facing(quat(b.get(i).sample()), pos(b.get(i).sample()), quat(c.get(i).sample()), pos(c.get(i).sample()), PROBE) : -1;
            fixed |= facing[i] < 0;
        }
        if (fixed) {
            access.horizon$rotation(cam.yaw(), cam.pitch());
            a = extract(p, 0);
            b = extract(p, 1);
            facing = new int[b.size()];
            Arrays.fill(facing, 2);
        }
        if (a.size() == b.size()) {
            for (int i = 0; i < a.size(); i++) {
                var pa = place(a.get(i));
                var pb = place(b.get(i));
                if (pa != null && pb != null && pa.name().equals(pb.name())) {
                    tape.whole(b.get(i).layer(), facing[i], pa, pb);
                } else {
                    tape.half(a.get(i).layer(), facing[i], 1, pa);
                    tape.half(b.get(i).layer(), facing[i], 2, pb);
                }
            }
        } else {
            for (int i = 0; i < a.size(); i++) tape.half(a.get(i).layer(), fixed ? 2 : i < facing.length ? facing[i] : 0, 1, place(a.get(i)));
            for (int i = 0; i < b.size(); i++) tape.half(b.get(i).layer(), facing[i], 2, place(b.get(i)));
        }
    }
}
