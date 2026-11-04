package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;

/** The crossing's `In` and `Out` are `DataInputStream`'s and `DataOutputStream`'s bytes; nested requests and big replies keep their own buffers. */
public final class CrossingCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private static void text(DataOutputStream out, String value) throws Exception {
        byte[] bytes = value.getBytes(StandardCharsets.UTF_8);
        out.writeInt(bytes.length);
        out.write(bytes);
    }

    private static void fields(DataOutputStream out) throws Exception {
        out.writeInt(-5);
        out.writeLong(-1L << 40);
        out.writeFloat(1.5f);
        out.writeDouble(-2.25);
        out.writeBoolean(true);
        out.writeByte(-3);
        out.writeShort(-300);
        out.writeChar('x');
        text(out, "minecraft:overworld");
        text(out, "");
        text(out, "héllo 世界 😀");
        out.write(new byte[] {1, 2, 3});
        out.writeUTF("modified \u0000 utf");
        out.writeBytes("bytes");
        out.writeChars("chars");
    }

    private static Crossing.In wrap(byte[] bytes) {
        ByteBuffer arena = ByteBuffer.allocateDirect(bytes.length + 16);
        arena.put(0, bytes);
        Crossing.In in = new Crossing.In();
        in.set(arena, bytes.length);
        return in;
    }

    /** Each writer a handler may use, about 50 KB of it; the last is the shape of the `items` reply. */
    private static void path(int kind, DataOutputStream out) throws Exception {
        int n = kind == 0 || kind == 6 ? 50_000 : kind == 1 ? 12_000 : 6_000;
        for (int i = 0; i < n; i++) {
            switch (kind) {
                case 0 -> out.write(i);
                case 1 -> out.writeInt(i);
                case 2 -> out.writeLong(i);
                case 3 -> out.write(new byte[8]);
                case 4 -> out.writeDouble(i);
                case 5 -> out.writeUTF("id" + i);
                case 6 -> out.writeBoolean(i % 2 == 0);
                default -> {
                    if (i == 0) out.writeInt(1500);
                    if (i < 1500) {
                        text(out, "mekanism:item_number_" + i);
                        out.writeBoolean(true);
                        out.writeBoolean(false);
                        out.writeBoolean(true);
                        out.writeBoolean(false);
                    }
                }
            }
        }
    }

    public static void main(String[] args) throws Exception {
        var expected = new ByteArrayOutputStream();
        fields(new DataOutputStream(expected));
        var out = new Crossing.Out();
        fields(out);
        check(Arrays.equals(expected.toByteArray(), out.toByteArray()), "Out is DataOutputStream's bytes");
        check(out.size() == expected.size(), "Out counts like DataOutputStream");
        out.reset();
        check(out.length() == 0 && out.size() == 0, "reset empties the reply");

        var in = wrap(expected.toByteArray());
        check(in.readInt() == -5 && in.readLong() == -1L << 40 && in.readFloat() == 1.5f && in.readDouble() == -2.25, "numbers");
        check(in.readBoolean() && in.readByte() == -3, "bool and byte");
        in.readNBytes(4);
        check(in.name().equals("minecraft:overworld") && in.text().isEmpty(), "text and empty text");
        check(in.text().equals("héllo 世界 😀"), "non-ASCII text");
        check(Arrays.equals(in.readNBytes(3), new byte[] {1, 2, 3}), "raw bytes");

        var names = new ByteArrayOutputStream();
        var data = new DataOutputStream(names);
        for (String name : new String[] {"a:b", "a:b", "c:d", "a:b", "e", "f", "g", "h", "a:b"}) text(data, name);
        var reading = wrap(names.toByteArray());
        String first = reading.name();
        check(reading.name() == first, "a repeated name is the same String");
        check(reading.name().equals("c:d") && reading.name() == first, "other names keep the first");
        for (String want : new String[] {"e", "f", "g", "h", "a:b"}) check(reading.name().equals(want), "name " + want);
        try {
            reading.readInt();
            check(false, "short input throws");
        } catch (java.io.UncheckedIOException expectedEnd) {
            // the end of the input
        }

        Crossing.free("t_echo", (input, reply) -> reply.write(input.rest()));
        final Crossing.In[] seen = new Crossing.In[1];
        Crossing.free("t_inner", (input, reply) -> reply.writeInt(input.readInt() + 1));
        Crossing.free("t_outer", (input, reply) -> {
            seen[0] = input;
            int a = input.readInt();
            var asked = new Crossing.Out();
            asked.writeInt(a);
            int inner = ByteBuffer.wrap(Crossing.call("t_inner", asked.toByteArray())).getInt();
            reply.writeInt(a);
            reply.writeInt(inner);
            reply.writeInt(input.readInt());
        });
        Crossing.free("t_big", (input, reply) -> reply.write(new byte[input.readInt()]));
        Crossing.free("t_throw", (input, reply) -> {
            throw new IllegalStateException("t_throw");
        });
        Crossing.free("t_none", (input, reply) -> {});
        Crossing.free("t_path", (input, reply) -> path(input.readInt(), reply));
        Crossing.seal();

        check(Arrays.equals(Crossing.call("t_echo", new byte[] {4, 5, 6}), new byte[] {4, 5, 6}), "echo");
        check(Crossing.call("t_none", new byte[0]).length == 0, "an empty reply is empty");
        var nested = new Crossing.Out();
        nested.writeInt(10);
        nested.writeInt(77);
        var reply = ByteBuffer.wrap(Crossing.call("t_outer", nested.toByteArray()));
        check(reply.getInt() == 10 && reply.getInt() == 11 && reply.getInt() == 77, "a nested request leaves the outer input intact");
        try {
            seen[0].readInt();
            check(false, "the frame is invalidated after the call");
        } catch (java.io.UncheckedIOException released) {
            // released
        }
        var big = new Crossing.Out();
        big.writeInt(300_000);
        check(Crossing.call("t_big", big.toByteArray()).length == 300_000, "an overflowing reply arrives whole");
        for (int kind = 0; kind < 8; kind++) {
            var asked = new Crossing.Out();
            asked.writeInt(kind);
            var expectedBytes = new ByteArrayOutputStream();
            path(kind, new DataOutputStream(expectedBytes));
            byte[] got = Crossing.call("t_path", asked.toByteArray());
            check(got.length > 40_000 && Arrays.equals(got, expectedBytes.toByteArray()), "a large reply through write path " + kind);
            check(Arrays.equals(got, Crossing.call("t_path", asked.toByteArray())), "the same reply again, from a grown buffer, path " + kind);
        }
        try {
            Crossing.call("t_throw", new byte[0]);
            check(false, "a throwing op fails");
        } catch (IllegalStateException failed) {
            check(failed.getMessage().contains("t_throw"), "the failure names the op");
        }
        check(Arrays.equals(Crossing.call("t_echo", new byte[] {9}), new byte[] {9}), "the next request succeeds after a throw");
        System.out.println("CrossingCheck ok");
    }
}
