package dev.horizonlab.javamods.boot;

import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;
import java.util.HexFormat;
import java.util.List;
import net.minecraft.network.FriendlyByteBuf;

/** `ChunkMirror`'s header and biome sections, byte for byte as Rust's `mirror` tests write and read them. */
public final class ChunkMirrorCheck {
    public static void main(String[] args) {
        // `a_header_and_a_biome_section_write_the_bytes_java_expects`
        var header = new ChunkMirror.Header("minecraft:overworld", List.of("minecraft:plains"), 7L);
        String rust = "00000001000000136d696e6563726166743a6f766572776f726c6400000001000000106d696e6563726166743a706c61696e730000000000000007";
        if (!HexFormat.of().formatHex(header.write()).equals(rust)) throw new AssertionError("header: " + HexFormat.of().formatHex(header.write()));
        if (!ChunkMirror.Header.read(HexFormat.of().parseHex(rust)).equals(header)) throw new AssertionError("header read");
        int[] values = new int[64];
        for (int i = 0; i < 64; i++) values[i] = i % 2 == 0 ? 5 : 9;
        var out = new FriendlyByteBuf(Unpooled.buffer());
        ChunkMirror.writeValues(out, values, 6);
        String section = HexFormat.of().formatHex(ByteBufUtil.getBytes(out));
        if (!section.equals("01020509aaaaaaaaaaaaaaaa")) throw new AssertionError("biome section: " + section);
        int[] back = ChunkMirror.readValues(new FriendlyByteBuf(Unpooled.wrappedBuffer(HexFormat.of().parseHex(section))), 64, 6);
        if (!java.util.Arrays.equals(back, values)) throw new AssertionError("biome section read");
        // Past three distinct biomes the section goes global: the indices themselves, at the session's width.
        int[] many = new int[64];
        for (int i = 0; i < 64; i++) many[i] = i % 40;
        var wide = new FriendlyByteBuf(Unpooled.buffer());
        ChunkMirror.writeValues(wide, many, 6);
        if (wide.getByte(0) != 6 || !java.util.Arrays.equals(ChunkMirror.readValues(wide, 64, 6), many)) throw new AssertionError("global biome section");
        System.out.println("ChunkMirrorCheck ok");
    }
}
