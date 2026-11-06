package dev.horizonlab.javamods.boot;

import net.minecraft.core.BlockPos;

/** A route's arguments and reply, byte for byte as Rust's `java_routing` tests write and read them. */
public final class RoutingArgsCheck {
    public static void main(String[] args) throws Exception {
        // `route_args_write_as_java_reads_them`: -2i32, 0x0102030405060708i64, 1.5f32, -2.25f64, true, BlockPos(1, 2, 3), "hi", "yo"
        var out = new java.io.ByteArrayOutputStream();
        var data = new java.io.DataOutputStream(out);
        data.writeInt(-2);
        data.writeLong(0x0102_0304_0506_0708L);
        data.writeFloat(1.5f);
        data.writeDouble(-2.25);
        data.writeBoolean(true);
        data.writeLong(BlockPos.asLong(1, 2, 3));
        data.writeInt(2);
        data.write("hi".getBytes());
        data.writeInt(2);
        data.write("yo".getBytes());
        var in = Crossing.In.of(out.toByteArray());
        if (in.readInt() != -2 || in.readLong() != 0x0102_0304_0506_0708L || in.readFloat() != 1.5f || in.readDouble() != -2.25 || !in.readBoolean()) throw new AssertionError("scalars");
        if (!BlockPos.of(in.readLong()).equals(new BlockPos(1, 2, 3))) throw new AssertionError("a position is a packed long");
        if (!in.text().equals("hi") || !in.text().equals("yo") || in.available() != 0) throw new AssertionError("text");
        String hex = java.util.HexFormat.of().formatHex(out.toByteArray());
        String rust = "fffffffe0102030405060708" + "3fc00000" + "c002000000000000" + "01" + String.format("%016x", BlockPos.asLong(1, 2, 3)) + "00000002" + "6869" + "00000002" + "796f";
        if (!hex.equals(rust)) throw new AssertionError("the layout Rust's test hard-codes: " + hex);
        // an ItemStack goes as a length and its saved NBT, which GameBridge.decode reads
        byte[] tag = {10, 0, 0, 8, 0, 2, 'i', 'd', 0, 15, 'm', 'i', 'n', 'e', 'c', 'r', 'a', 'f', 't', ':', 's', 't', 'o', 'n', 'e', 0};
        var stack = new java.io.ByteArrayOutputStream();
        var stackData = new java.io.DataOutputStream(stack);
        stackData.writeInt(tag.length);
        stackData.write(tag);
        var stackIn = Crossing.In.of(stack.toByteArray());
        if (!java.util.Arrays.equals(stackIn.readNBytes(stackIn.readInt()), tag) || stackIn.available() != 0) throw new AssertionError("a stack is a length and its tag");
        // a static no-op route answers status 0 and an empty sound list
        var request = new java.io.ByteArrayOutputStream();
        var write = new java.io.DataOutputStream(request);
        write.writeInt(0);
        write.writeByte(2);
        var reply = new Crossing.Out();
        Routing.benchCall(Crossing.In.of(request.toByteArray()), reply);
        byte[] bytes = reply.toByteArray();
        if (bytes[0] != 0 || bytes.length != 1 + 4) throw new AssertionError("status 0, then an empty sound count: " + bytes.length);
        System.out.println("RoutingArgsCheck ok");
    }
}
