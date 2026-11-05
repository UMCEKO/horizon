package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.ByteArrayInputStream;

/** A recording cut in segments: each cut is a frame of the entries since the last, and together they hold what one uncut recording holds. */
public final class RecorderCutCheck {
    private static int count(byte[] frame) throws Exception {
        return new DataInputStream(new ByteArrayInputStream(frame)).readInt();
    }

    private static byte[] entries(byte[] frame) {
        return java.util.Arrays.copyOfRange(frame, 4, frame.length);
    }

    /** A frame blurs once; the check wants several entries, so it lets the blur again. */
    private static void blur(Recorder recorder) throws Exception {
        var field = net.minecraft.client.renderer.state.gui.GuiRenderState.class.getDeclaredField("firstStratumAfterBlur");
        field.setAccessible(true);
        field.setInt(recorder, Integer.MAX_VALUE);
        recorder.blurBeforeThisStratum();
    }

    public static void main(String[] args) throws Exception {
        var cut = new Recorder();
        blur(cut);
        byte[] first = cut.cut();
        blur(cut);
        blur(cut);
        byte[] second = cut.cut();
        byte[] none = cut.cut();
        if (count(first) != 1 || count(second) != 2 || count(none) != 0) throw new AssertionError(count(first) + " " + count(second) + " " + count(none));
        var whole = new Recorder();
        blur(whole);
        blur(whole);
        blur(whole);
        var joined = new ByteArrayOutputStream();
        joined.write(entries(first));
        joined.write(entries(second));
        if (!java.util.Arrays.equals(joined.toByteArray(), entries(whole.bytes()))) throw new AssertionError("the segments' entries differ from one recording's");
        System.out.println("RecorderCutCheck ok");
    }
}
