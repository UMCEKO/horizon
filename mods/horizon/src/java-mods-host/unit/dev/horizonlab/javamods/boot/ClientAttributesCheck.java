package dev.horizonlab.javamods.boot;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import net.minecraft.world.attribute.EnvironmentAttributeProbe;

/** A probe with no level answers every visual attribute's default, as its last and its new value. */
public final class ClientAttributesCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private static float[] floats(DataInputStream in, int n) throws Exception {
        var out = new float[n];
        for (int i = 0; i < n; i++) out[i] = in.readFloat();
        return out;
    }

    public static void main(String[] args) throws Exception {
        var bootstrapped = net.minecraft.server.Bootstrap.class.getDeclaredField("isBootstrapped");
        bootstrapped.setAccessible(true);
        bootstrapped.setBoolean(null, true);
        var bytes = new ByteArrayOutputStream();
        ClientAttributes.table(new DataOutputStream(bytes), new EnvironmentAttributeProbe());
        var in = new DataInputStream(new ByteArrayInputStream(bytes.toByteArray()));
        check(in.readInt() == ClientAttributes.VISUAL.size(), "every visual attribute is written");
        for (var visual : ClientAttributes.VISUAL) {
            var id = new byte[in.readInt()];
            in.readFully(id);
            check(new String(id, java.nio.charset.StandardCharsets.UTF_8).equals(visual.id()), "in table order: " + visual.id());
            int tag = in.readInt();
            check(tag == ClientAttributes.tag(visual.attribute()), "the type tag of " + visual.id());
            int width = tag == 1 ? 3 : tag == 2 ? 4 : 1;
            var last = floats(in, width);
            var now = floats(in, width);
            check(java.util.Arrays.equals(last, now), visual.id() + " last equals new");
            var fallback = visual.attribute().defaultValue();
            var expected = fallback instanceof Float f ? new float[] {f} : fallback instanceof org.joml.Vector3fc v ? new float[] {v.x(), v.y(), v.z()} : new float[] {((org.joml.Vector4fc) fallback).x(), ((org.joml.Vector4fc) fallback).y(), ((org.joml.Vector4fc) fallback).z(), ((org.joml.Vector4fc) fallback).w()};
            check(java.util.Arrays.equals(expected, now), visual.id() + " is the attribute's default");
        }
        check(in.available() == 0, "nothing after the table");
        System.out.println("ClientAttributesCheck ok");
    }
}
