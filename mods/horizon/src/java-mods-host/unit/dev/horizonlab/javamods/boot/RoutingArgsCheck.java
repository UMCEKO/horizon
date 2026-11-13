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
        // a static no-op route answers status 0, no player's changes (-1) and an empty sound list
        var request = new java.io.ByteArrayOutputStream();
        var write = new java.io.DataOutputStream(request);
        write.writeInt(0);
        write.writeByte(2);
        var reply = new Crossing.Out();
        Routing.benchCall(Crossing.In.of(request.toByteArray()), reply);
        byte[] bytes = reply.toByteArray();
        var read = Crossing.In.of(bytes);
        if (read.readByte() != 0 || read.readInt() != -1 || read.readInt() != 0 || read.available() != 0) throw new AssertionError("status 0, -1, then an empty sound count: " + bytes.length);
        values();
        uses();
        System.out.println("RoutingArgsCheck ok");
    }

    /** `a_hit_writes_as_eight_doubles` and `a_use_result_encodes_as_the_ints_java_decodes` of Rust's `java_use` tests. */
    private static void uses() throws Exception {
        var bytes = java.util.HexFormat.of().parseHex("400a000000000000" + "c04fc00000000000" + "401f000000000000" + "3ff0000000000000" + "3ff0000000000000" + "4008000000000000" + "c050000000000000" + "401c000000000000" + "00000001");
        var in = Crossing.In.of(bytes);
        var hit = (net.minecraft.world.phys.BlockHitResult) Routing.reader(net.minecraft.world.phys.BlockHitResult.class).read(in, new Routing.Args("k"));
        if (hit.getLocation().x != 3.25 || hit.getLocation().y != -63.5 || hit.getLocation().z != 7.75 || hit.getDirection() != net.minecraft.core.Direction.UP || !hit.isInside() || !hit.getBlockPos().equals(new BlockPos(3, -64, 7))) throw new AssertionError("a hit: " + hit);
        if (Routing.reader(net.minecraft.world.InteractionHand.class).read(in, new Routing.Args("k")) != net.minecraft.world.InteractionHand.OFF_HAND || in.available() != 0) throw new AssertionError("a hand is its ordinal");
        if (Flat.toInteractionResult(16) != net.minecraft.world.InteractionResult.SUCCESS || Flat.toInteractionResult(0) != net.minecraft.world.InteractionResult.CONSUME
                || Flat.toInteractionResult(1) != net.minecraft.world.InteractionResult.FAIL || Flat.toInteractionResult(2) != net.minecraft.world.InteractionResult.PASS) throw new AssertionError("a use result is its kind and its swing source");
        var round = Flat.raw(hit);
        if (!java.util.Arrays.equals(round, new double[] {3.25, -63.5, 7.75, 1, 1, 3, -64, 7})) throw new AssertionError("a hit goes back as the same eight numbers");
    }

    static java.util.Optional<net.minecraft.world.phys.Vec3> optional() {
        return java.util.Optional.empty();
    }

    static void unreadable(java.util.List<String> list) {}

    /** `vec3_and_effect_args_write_as_java_reads_them` and `optional_vec3_reads_both_cases`, byte for byte. */
    private static void values() throws Exception {
        var bytes = java.util.HexFormat.of().parseHex("3ff8000000000000c0000000000000003fd00000000000000000000f6d696e6563726166743a73706565640000006400000001000101000000000000000f6d696e6563726166743a626c6f636b0000000f6d696e6563726166743a73746f6e65000000136d696e6563726166743a6f766572776f726c64");
        var in = Crossing.In.of(bytes);
        var call = new Routing.Args("k");
        var vec = (net.minecraft.world.phys.Vec3) Routing.reader(net.minecraft.world.phys.Vec3.class).read(in, call);
        if (vec.x != 1.5 || vec.y != -2.0 || vec.z != 0.25) throw new AssertionError("a Vec3 is three doubles");
        if (!in.text().equals("minecraft:speed") || in.readInt() != 100 || in.readInt() != 1 || in.readBoolean() || !in.readBoolean() || !in.readBoolean()) throw new AssertionError("an effect: id, duration, amplifier, ambient, visible, icon");
        if (Routing.reader(net.minecraft.world.entity.Entity.class).read(in, call) != null) throw new AssertionError("entity id 0 is null");
        var key = (net.minecraft.resources.ResourceKey<?>) Routing.reader(net.minecraft.resources.ResourceKey.class).read(in, call);
        if (!key.toString().contains("minecraft:block") || !key.toString().contains("minecraft:stone")) throw new AssertionError("a key is its registry, then its id: " + key);
        if (!in.text().equals("minecraft:overworld") || in.available() != 0) throw new AssertionError("a level is its dimension's text");
        var writer = Routing.writer(RoutingArgsCheck.class.getDeclaredMethod("optional").getGenericReturnType());
        var absent = new Crossing.Out();
        writer.write(java.util.Optional.empty(), absent);
        var present = new Crossing.Out();
        writer.write(java.util.Optional.of(new net.minecraft.world.phys.Vec3(1, 2, 3)), present);
        var hex = java.util.HexFormat.of();
        if (!hex.formatHex(absent.toByteArray()).equals("00") || !hex.formatHex(present.toByteArray()).equals("01" + "3ff0000000000000" + "4000000000000000" + "4008000000000000")) throw new AssertionError("an Optional<Vec3> is a flag, then three doubles");
        var method = RoutingArgsCheck.class.getDeclaredMethod("unreadable", java.util.List.class);
        if (Routing.plan("k", method, false).unreadable() == null) throw new AssertionError("a type with no reader is reported");
        try {
            Routing.plan("k", method, true);
            throw new AssertionError("a gated route with no reader must fail the boot");
        } catch (IllegalStateException expected) {
            if (!expected.getMessage().equals("route k: no argument reader for java.util.List")) throw new AssertionError(expected.getMessage());
        }
    }
}
