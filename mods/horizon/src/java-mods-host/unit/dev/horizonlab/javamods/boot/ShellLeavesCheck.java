package dev.horizonlab.javamods.boot;

import java.io.DataInputStream;
import java.io.ByteArrayInputStream;

/** Frames joined by `Recorder.concat` stay one frame, in order. */
public final class ShellLeavesCheck {
    private static byte[] frame(int... fills) {
        var out = new java.io.ByteArrayOutputStream();
        var data = new java.io.DataOutputStream(out);
        try {
            data.writeInt(fills.length);
            for (int fill : fills) data.writeByte(fill);
        } catch (java.io.IOException e) {
            throw new IllegalStateException(e);
        }
        return out.toByteArray();
    }

    public static void main(String[] args) throws Exception {
        byte[] joined = Recorder.concat(frame(1), frame(2, 3));
        if (new DataInputStream(new ByteArrayInputStream(joined)).readInt() != 3) throw new AssertionError("count");
        if (!java.util.Arrays.equals(joined, frame(1, 2, 3))) throw new AssertionError("order");
        if (!java.util.Arrays.equals(Recorder.concat(frame(), frame(7)), frame(7))) throw new AssertionError("empty first");
        if (!java.util.Arrays.equals(Recorder.concat(frame(7), frame()), frame(7))) throw new AssertionError("empty last");
        if (ShellLeaves.COUNT != 14 || !ShellLeaves.name(ShellLeaves.BOOK_TOOLTIP).equals("BOOK_TOOLTIP")) throw new AssertionError("leaves");
        System.out.println("ShellLeavesCheck ok");
    }
}
