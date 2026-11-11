package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.util.Arrays;
import java.util.List;
import org.joml.Quaternionf;
import org.joml.Vector3f;

/** The particle recording's facing test and its wire form. */
public final class ParticleRecorderCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) throws Exception {
        var probe = new Quaternionf().rotationYXZ((float) (Math.PI - Math.toRadians(240)), (float) -Math.toRadians(35), 0);
        var b = new Quaternionf().rotationZ(0.3f);
        var at = new Vector3f(1, 2, 3);
        check(ParticleRecorder.facing(b, at, new Quaternionf(probe).mul(b), at, probe) == 0, "camera facing");
        var yaw = new Quaternionf(0, probe.y, 0, probe.w).normalize();
        check(ParticleRecorder.facing(b, at, yaw.mul(b, new Quaternionf()), at, probe) == 1, "yaw facing");
        check(ParticleRecorder.facing(b, at, new Quaternionf(b), at, probe) == 2, "fixed");
        check(ParticleRecorder.facing(b, at, new Quaternionf().rotationX(0.2f).mul(b), at, probe) == -1, "unrelated rotation");
        check(ParticleRecorder.facing(b, at, new Quaternionf(probe).mul(b), new Vector3f(1.01f, 2, 3), probe) == -1, "positions apart");

        float[] f = new float[14];
        for (int i = 0; i < 14; i++) f[i] = i + 1;
        var sample = new ParticleRecorder.Sample(f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7], f[8], f[9], f[10], f[11], 13, 14);
        byte[] made = ParticleRecorder.encode(new double[] {1, 2, 3}, List.of("minecraft:particle/flame"), List.of(new ParticleRecorder.Rec(1, 0, sample, sample)));
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeDouble(1);
        out.writeDouble(2);
        out.writeDouble(3);
        out.writeInt(1);
        byte[] name = "minecraft:particle/flame".getBytes("UTF-8");
        out.writeInt(name.length);
        out.write(name);
        out.writeInt(1);
        out.writeByte(1);
        out.writeInt(0);
        for (int s = 0; s < 2; s++) {
            for (int i = 0; i < 12; i++) out.writeFloat(i + 1);
            out.writeInt(13);
            out.writeInt(14);
        }
        check(Arrays.equals(made, bytes.toByteArray()), "one quad's bytes");
    }
}
