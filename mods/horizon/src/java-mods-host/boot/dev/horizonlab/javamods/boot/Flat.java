package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.host.Host;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.UncheckedIOException;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.stats.Stat;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.Vec3;

/** What a bridge passes between Java and Rust: `raw` flattens an argument, `to` + the type's name rebuilds a result. */
public final class Flat {
    private Flat() {}

    public static boolean ON = true;

    public static void link(Class<?> target) {
        Host.link(target);
    }

    public static long raw(BlockPos pos) {
        return pos.asLong();
    }

    public static int raw(BlockState state) {
        return Block.BLOCK_STATE_REGISTRY.getId(state);
    }

    public static int raw(Block block) {
        return raw(block.defaultBlockState());
    }

    public static int raw(Direction direction) {
        return direction.ordinal();
    }

    /** The stat's type path, a tab, and its value's id. */
    public static String raw(Stat<?> stat) {
        return statKey(stat);
    }

    private static <T> String statKey(Stat<T> stat) {
        var type = stat.getType();
        return BuiltInRegistries.STAT_TYPE.getKey(type).getPath() + "\t" + type.getRegistry().getKey(stat.getValue());
    }

    public static String raw(Holder<?> holder) {
        return holder.unwrapKey().map(key -> key.identifier().toString()).orElse("");
    }

    public static double[] raw(Vec3 point) {
        return new double[] {point.x, point.y, point.z};
    }

    public static String raw(Heightmap.Types type) {
        return type.getSerializationKey().toLowerCase(java.util.Locale.ROOT);
    }

    public static byte[] raw(Component message) {
        try {
            var tag = new CompoundTag();
            tag.put("text", ComponentSerialization.CODEC.encodeStart(NbtOps.INSTANCE, message).getOrThrow());
            var bytes = new ByteArrayOutputStream();
            NbtIo.write(tag, new DataOutputStream(bytes));
            return bytes.toByteArray();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    public static BlockState toBlockState(int id) {
        return Block.BLOCK_STATE_REGISTRY.byId(id);
    }
}
