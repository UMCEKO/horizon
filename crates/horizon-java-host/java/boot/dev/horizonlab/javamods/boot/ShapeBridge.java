package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.boot.mixin.VoxelShapeAccessor;
import dev.horizonlab.javamods.host.Host;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.SupportType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;

/** Serves SupportType.isSupporting from Rust; the block still supplies its shape in Java, so mods' overrides run. */
public final class ShapeBridge {
    private static final String MODE = System.getProperty("horizon.java.shapes", "rust");
    private static final boolean SHADOW = MODE.equals("shadow");
    private static final Map<VoxelShape, Integer> FLAGS = new IdentityHashMap<>();
    private static final List<String> MISMATCHES = new ArrayList<>();
    private static long served;
    private static long compared;
    private static long mismatched;
    private static long java;
    private static long nanos;
    private static String blocked;

    private ShapeBridge() {}

    public static boolean isSupporting(SupportType type, BlockState state, BlockGetter level, BlockPos pos, Direction direction) {
        if (MODE.equals("off") || patched()) return type.isSupporting(state, level, pos, direction);
        VoxelShape shape = state.getBlockSupportShape(level, pos);
        int flags = shape.getClass().getName().startsWith("net.minecraft.") ? flags(shape) : -1;
        if (flags < 0) {
            synchronized (FLAGS) {
                java++;
            }
            return type.isSupporting(state, level, pos, direction);
        }
        boolean rust = (flags >> (direction.ordinal() * 3 + type.ordinal()) & 1) != 0;
        if (!SHADOW) {
            synchronized (FLAGS) {
                served++;
            }
            return rust;
        }
        boolean vanilla = type.isSupporting(state, level, pos, direction);
        synchronized (FLAGS) {
            compared++;
            if (rust != vanilla && ++mismatched <= 20) {
                MISMATCHES.add(state + " " + type + " " + direction + " java=" + vanilla + " rust=" + rust + " shape=" + shape);
            }
        }
        return vanilla;
    }

    private static int flags(VoxelShape shape) {
        synchronized (FLAGS) {
            Integer known = FLAGS.get(shape);
            if (known != null) return known;
            long began = System.nanoTime();
            double[] x = shape.getCoords(Direction.Axis.X).toDoubleArray();
            double[] y = shape.getCoords(Direction.Axis.Y).toDoubleArray();
            double[] z = shape.getCoords(Direction.Axis.Z).toDoubleArray();
            int nx = Math.max(x.length - 1, 0);
            int ny = Math.max(y.length - 1, 0);
            int nz = Math.max(z.length - 1, 0);
            boolean[] occupied = new boolean[nx * ny * nz];
            var cells = ((VoxelShapeAccessor) shape).horizon$shape();
            int i = 0;
            for (int a = 0; a < nx; a++) {
                for (int b = 0; b < ny; b++) {
                    for (int c = 0; c < nz; c++) occupied[i++] = cells.isFull(a, b, c);
                }
            }
            int flags = Host.shapeFlags(x, y, z, occupied);
            FLAGS.put(shape, flags);
            nanos += System.nanoTime() - began;
            return flags;
        }
    }

    // A mod's mixin into this logic wins: Rust can't know what it changed, so Java keeps the work.
    private static boolean patched() {
        synchronized (FLAGS) {
            if (blocked == null) {
                blocked = "";
                Class<?>[] classes = {SupportType.class, VoxelShape.class, Shapes.class, Block.class};
                for (Class<?> c : classes) {
                    if (merged(c)) blocked = c.getName();
                }
                for (SupportType t : SupportType.values()) {
                    if (merged(t.getClass())) blocked = t.getClass().getName();
                }
            }
            return !blocked.isEmpty();
        }
    }

    private static boolean merged(Class<?> c) {
        for (Method m : c.getDeclaredMethods()) {
            for (var a : m.getDeclaredAnnotations()) {
                if (a.annotationType().getSimpleName().equals("MixinMerged") && !a.toString().contains("dev.horizonlab.javamods")) return true;
            }
        }
        return false;
    }

    public static String report() {
        synchronized (FLAGS) {
            return "mode=" + MODE + " blocked_by=" + (blocked == null ? "" : blocked) + " distinct_shape_objects=" + FLAGS.size()
                    + " served=" + served + " compared=" + compared + " mismatched=" + mismatched + " java_fallback=" + java
                    + " bridge_ms=" + nanos / 1000000 + " " + MISMATCHES;
        }
    }
}
