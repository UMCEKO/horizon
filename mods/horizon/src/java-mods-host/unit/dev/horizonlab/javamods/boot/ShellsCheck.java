package dev.horizonlab.javamods.boot;

import net.neoforged.bus.api.BusBuilder;
import net.neoforged.neoforge.client.event.ScreenEvent;

/** Which listeners make a shell: a mod's, not Horizon's, NeoForge's or the JDK's; and what a recording's change means. */
public final class ShellsCheck {
    public static void main(String[] args) throws Exception {
        if (Shells.isMods(String.class) || Shells.isMods(Shells.class) || Shells.isMods(net.minecraft.client.Minecraft.class)) throw new AssertionError("not a mod's");
        if (!Shells.isMods(org.joml.Vector2i.class)) throw new AssertionError("a mod's");
        var bus = BusBuilder.builder().build();
        bus.start();
        if (Shells.wanted(bus)) throw new AssertionError("an empty bus");
        bus.addListener(ScreenEvent.Render.Post.class, event -> {});
        if (Shells.wanted(bus)) throw new AssertionError("a listener of Horizon's own class");
        byte[][] a = new byte[Shells.SEGMENTS][];
        byte[][] b = new byte[Shells.SEGMENTS][];
        for (int i = 0; i < Shells.SEGMENTS; i++) {
            a[i] = new byte[] {(byte) i};
            b[i] = new byte[] {(byte) (i == 27 ? 99 : i)};
        }
        Shells.last = new byte[Shells.SEGMENTS][];
        if (!Shells.noteRecorded(a, 0, 0) || !Shells.animating) throw new AssertionError("a first recording changes");
        if (Shells.noteRecorded(a.clone(), 0, 0) || !Shells.animating) throw new AssertionError("alike with no real time passed keeps animating");
        if (Shells.noteRecorded(a.clone(), 0, 1) || Shells.animating) throw new AssertionError("alike after real time ends it");
        if (!Shells.noteRecorded(b, 0, 1) || !Shells.animating) throw new AssertionError("a differing one restarts it");
        if (!Shells.noteRecorded(b.clone(), 1 << 10, 1)) throw new AssertionError("a differing routed set changes it");
        var written = new java.io.ByteArrayOutputStream();
        Shells.write(new java.io.DataOutputStream(written), a, true, true);
        if (written.size() != 1 + 4 + 28 * 5 + 4) throw new AssertionError("a counted write " + written.size());
        System.out.println("ShellsCheck ok");
    }
}
