package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.BitSet;
import java.util.List;
import java.util.Map;

/** The particle facts' wire form and the replaced-provider test, with no boot. */
public final class ParticleFactsCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) throws Exception {
        var owned = new BitSet();
        owned.set(1);
        var expected = new ByteArrayOutputStream();
        var out = new DataOutputStream(expected);
        out.writeInt(2);
        out.writeInt(15);
        out.write("minecraft:flame".getBytes(StandardCharsets.UTF_8));
        out.writeBoolean(false);
        out.writeInt(7);
        out.write("x:spark".getBytes(StandardCharsets.UTF_8));
        out.writeBoolean(true);
        check(Arrays.equals(ParticleFacts.write(List.of("minecraft:flame", "x:spark"), owned), expected.toByteArray()), "the reply's bytes");
        Map<String, Object> providers = Map.of("minecraft:flame", new fakemod.FakeProvider(), "minecraft:smoke", Integer.valueOf(1));
        check(ParticleFacts.replaced(providers, n -> false).equals(List.of("minecraft:flame")), "a mod's class on a vanilla type");
        check(ParticleFacts.replaced(providers, n -> n.equals("minecraft:flame")).isEmpty(), "an owned type is not a replacement");
    }
}
