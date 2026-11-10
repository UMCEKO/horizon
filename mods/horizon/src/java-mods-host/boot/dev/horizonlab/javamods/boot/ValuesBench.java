package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.boot.mixin.VoxelShapeAccessor;
import dev.horizonlab.javamods.host.Host;
import java.lang.reflect.Constructor;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Random;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.util.Mth;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;
import net.minecraft.world.level.levelgen.synth.SimplexNoise;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.BitSetDiscreteVoxelShape;
import net.minecraft.world.phys.shapes.BooleanOp;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;

/** Measures crossing into Rust against the JIT for the pure value methods; run with the op `values_bench`. */
public final class ValuesBench {
    private ValuesBench() {}

    static {
        Host.link(ValuesBench.class);
    }

    static native int nop(int x);
    static native double[] vec3Add(double[] a, double[] b);
    static native double vec3DistSq(double ax, double ay, double az, double bx, double by, double bz);
    static native double[] aabbMove(double[] box, double[] by);
    static native double atan2(double y, double x);
    static native double clamp(double v, double lo, double hi);
    static native double lerp(double a, double p, double q);
    static native float wrapDegrees(float angle);
    static native double vec3Length(double x, double y, double z);
    static native boolean aabbIntersects(double a0, double a1, double a2, double a3, double a4, double a5, double b0, double b1, double b2, double b3, double b4, double b5);
    static native long blockRelative(long pos, int dir, int steps);
    static native long shapeNew(double[] x, double[] y, double[] z, boolean[] cells);
    static native int joinNotEmpty(long a, long b, int truth);
    static native int joinNotEmptyArrays(double[] ax, double[] ay, double[] az, boolean[] ac, double[] bx, double[] by, double[] bz, boolean[] bc, int truth);
    static native int joinNotEmptyMixed(long a, double[] bx, double[] by, double[] bz, boolean[] bc, int truth);
    static native double[] joinUnoptimized(long a, long b, int truth);
    static native double[] optimize(long a);
    static native boolean[] takeCells();
    static native double[] clip(long shape, double[] from, double[] to, long pos);
    static native long simplexNew(long seed, double ox, double oy, double oz);
    static native double[] simplexTable(long h);
    static native float simplex2(long h, double x, double y);
    static native float simplex3(long h, double x, double y, double z);

    interface Work {
        long run(int n);
    }

    static long sink;
    static final int M = 4095;
    static final Direction[] DIRS = Direction.values();
    static final IdentityHashMap<VoxelShape, Long> HANDLES = new IdentityHashMap<>();
    static final IdentityHashMap<SimplexNoise, Long> NOISES = new IdentityHashMap<>();
    static Constructor<?> ARRAY_SHAPE;

    static String op(String arg) throws Exception {
        ARRAY_SHAPE = Class.forName("net.minecraft.world.phys.shapes.ArrayVoxelShape")
                .getDeclaredConstructor(net.minecraft.world.phys.shapes.DiscreteVoxelShape.class, double[].class, double[].class, double[].class);
        ARRAY_SHAPE.setAccessible(true);
        var out = new StringBuilder();
        String which = arg == null || arg.isEmpty() ? "all" : arg;
        out.append("rounds alternate Java and Rust; ns per call, median [min..max]; speedup = java median / rust median\n");
        if (which.equals("all") || which.equals("parity")) parity(out);
        if (which.equals("all") || which.equals("cheap")) cheap(out);
        if (which.equals("all") || which.equals("shapes")) shapes(out);
        if (which.equals("all") || which.equals("noise")) noise(out);
        out.append("sink=").append(sink).append('\n');
        return out.toString();
    }

    // ---- harness ----

    static void bench(StringBuilder out, String name, int n, String[] names, Work[] works) {
        int rounds = 11;
        for (Work w : works) {
            w.run(n);
            w.run(n);
        }
        double[][] times = new double[works.length][rounds];
        for (int r = 0; r < rounds; r++) {
            for (int k = 0; k < works.length; k++) {
                int w = (k + r) % works.length;
                long began = System.nanoTime();
                sink += works[w].run(n);
                times[w][r] = (System.nanoTime() - began) / (double) n;
            }
        }
        double base = 0;
        for (int w = 0; w < works.length; w++) {
            Arrays.sort(times[w]);
            double median = times[w][rounds / 2];
            if (w == 0) base = median;
            out.append(String.format("%-22s %-34s %9.1f [%7.1f..%7.1f]  %s%n", name, names[w], median, times[w][0], times[w][rounds - 1],
                    w == 0 ? "" : String.format("speedup %.2fx", base / median)));
        }
    }

    static long bits(double d) {
        return Double.doubleToRawLongBits(d);
    }

    static int truth(BooleanOp op) {
        return (op.apply(false, false) ? 1 : 0) | (op.apply(false, true) ? 2 : 0) | (op.apply(true, false) ? 4 : 0) | (op.apply(true, true) ? 8 : 0);
    }

    // ---- cheap ones ----

    static void cheap(StringBuilder out) {
        Random rnd = new Random(1);
        Vec3[] va = new Vec3[M + 1];
        Vec3[] vb = new Vec3[M + 1];
        AABB[] boxes = new AABB[M + 1];
        BlockPos[] pos = new BlockPos[M + 1];
        double[] ys = new double[M + 1];
        double[] xs = new double[M + 1];
        Direction[] dirs = new Direction[M + 1];
        int[] steps = new int[M + 1];
        for (int i = 0; i <= M; i++) {
            va[i] = new Vec3(rnd.nextDouble() * 200 - 100, rnd.nextDouble() * 200 - 100, rnd.nextDouble() * 200 - 100);
            vb[i] = new Vec3(rnd.nextDouble() * 200 - 100, rnd.nextDouble() * 200 - 100, rnd.nextDouble() * 200 - 100);
            boxes[i] = new AABB(va[i], vb[i]);
            pos[i] = new BlockPos(rnd.nextInt(60000) - 30000, rnd.nextInt(320) - 64, rnd.nextInt(60000) - 30000);
            ys[i] = rnd.nextDouble() * 200 - 100;
            xs[i] = rnd.nextDouble() * 200 - 100;
            dirs[i] = DIRS[rnd.nextInt(6)];
            steps[i] = rnd.nextInt(5) - 2;
        }
        bench(out, "nop crossing", 5_000_000, new String[] {"java: s += i", "rust: nop(i)"},
                new Work[] {n -> { long s = 0; for (int i = 0; i < n; i++) s += i & 7; return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += nop(i & 7); return s; }});
        bench(out, "Vec3.add", 5_000_000, new String[] {"java", "rust (double[] in, double[] out)"},
                new Work[] {n -> { long s = 0; for (int i = 0; i < n; i++) s += bits(va[i & M].add(vb[i & M]).x); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) {
                            double[] o = vec3Add(Flat.raw(va[i & M]), Flat.raw(vb[i & M]));
                            s += bits(new Vec3(o[0], o[1], o[2]).x);
                        } return s; }});
        bench(out, "Vec3.distanceToSqr", 5_000_000, new String[] {"java", "rust (6 doubles in, double out)"},
                new Work[] {n -> { long s = 0; for (int i = 0; i < n; i++) s += bits(va[i & M].distanceToSqr(vb[i & M])); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) { Vec3 a = va[i & M]; Vec3 b = vb[i & M]; s += bits(vec3DistSq(a.x, a.y, a.z, b.x, b.y, b.z)); } return s; }});
        bench(out, "AABB.move(Vec3)", 5_000_000, new String[] {"java", "rust (double[6]+double[3] in/out)"},
                new Work[] {n -> { long s = 0; for (int i = 0; i < n; i++) s += bits(boxes[i & M].move(va[i & M]).maxZ); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) {
                            AABB b = boxes[i & M];
                            double[] o = aabbMove(new double[] {b.minX, b.minY, b.minZ, b.maxX, b.maxY, b.maxZ}, Flat.raw(va[i & M]));
                            s += bits(new AABB(o[0], o[1], o[2], o[3], o[4], o[5]).maxZ);
                        } return s; }});
        bench(out, "Mth.atan2", 5_000_000, new String[] {"java", "rust (2 doubles in, double out)"},
                new Work[] {n -> { long s = 0; for (int i = 0; i < n; i++) s += bits(Mth.atan2(ys[i & M], xs[i & M])); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += bits(atan2(ys[i & M], xs[i & M])); return s; }});
        float[] fs = new float[M + 1];
        for (int i = 0; i <= M; i++) fs[i] = (float) (rnd.nextGaussian() * 500);
        bench(out, "Mth.clamp", 5_000_000, new String[] {"java", "rust (3 doubles in, double out)"},
                new Work[] {n -> { long s = 0; for (int i = 0; i < n; i++) s += bits(Mth.clamp(ys[i & M], -50.0, xs[i & M])); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += bits(clamp(ys[i & M], -50.0, xs[i & M])); return s; }});
        bench(out, "Mth.lerp", 5_000_000, new String[] {"java", "rust (3 doubles in, double out)"},
                new Work[] {n -> { long s = 0; for (int i = 0; i < n; i++) s += bits(Mth.lerp(0.37, ys[i & M], xs[i & M])); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += bits(lerp(0.37, ys[i & M], xs[i & M])); return s; }});
        bench(out, "Mth.wrapDegrees(float)", 5_000_000, new String[] {"java", "rust (float in, float out)"},
                new Work[] {n -> { long s = 0; for (int i = 0; i < n; i++) s += Float.floatToRawIntBits(Mth.wrapDegrees(fs[i & M])); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += Float.floatToRawIntBits(wrapDegrees(fs[i & M])); return s; }});
        bench(out, "Vec3.length", 5_000_000, new String[] {"java", "rust (3 doubles in, double out)"},
                new Work[] {n -> { long s = 0; for (int i = 0; i < n; i++) s += bits(va[i & M].length()); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) { Vec3 v = va[i & M]; s += bits(vec3Length(v.x, v.y, v.z)); } return s; }});
        bench(out, "AABB.intersects(AABB)", 5_000_000, new String[] {"java", "rust (12 doubles in, boolean out)"},
                new Work[] {n -> { long s = 0; for (int i = 0; i < n; i++) s += boxes[i & M].intersects(boxes[(i + 7) & M]) ? 1 : 0; return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) { AABB a = boxes[i & M], b = boxes[(i + 7) & M];
                            s += aabbIntersects(a.minX, a.minY, a.minZ, a.maxX, a.maxY, a.maxZ, b.minX, b.minY, b.minZ, b.maxX, b.maxY, b.maxZ) ? 1 : 0; } return s; }});
        bench(out, "BlockPos.relative(d,n)", 5_000_000, new String[] {"java", "rust (long,int,int in, long out)"},
                new Work[] {n -> { long s = 0; for (int i = 0; i < n; i++) s += pos[i & M].relative(dirs[i & M], steps[i & M]).getX(); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += BlockPos.of(blockRelative(pos[i & M].asLong(), dirs[i & M].ordinal(), steps[i & M])).getX(); return s; }});
    }

    // ---- shapes ----

    static VoxelShape b(double x1, double y1, double z1, double x2, double y2, double z2) {
        return Shapes.box(x1 / 16, y1 / 16, z1 / 16, x2 / 16, y2 / 16, z2 / 16);
    }

    static List<VoxelShape> pool(Random rnd) {
        List<VoxelShape> pool = new ArrayList<>();
        pool.add(Shapes.block());
        pool.add(b(0, 0, 0, 16, 8, 16));
        pool.add(b(0, 8, 0, 16, 16, 16));
        pool.add(Shapes.or(b(0, 0, 0, 16, 8, 16), b(0, 8, 0, 16, 16, 8)));
        pool.add(Shapes.or(b(0, 0, 0, 16, 8, 16), b(0, 8, 0, 8, 16, 8), b(8, 8, 8, 16, 16, 16)));
        pool.add(Shapes.or(b(6, 0, 6, 10, 16, 10), b(7, 6, 0, 9, 9, 6), b(7, 12, 0, 9, 15, 6), b(10, 6, 7, 16, 9, 9), b(10, 12, 7, 16, 15, 9)));
        pool.add(Shapes.join(Shapes.block(), b(2, 4, 2, 14, 16, 14), BooleanOp.ONLY_FIRST));
        pool.add(b(1, 0, 1, 15, 14, 15));
        pool.add(b(6, 0, 6, 10, 10, 10));
        pool.add(Shapes.or(b(7, 0, 7, 9, 16, 9), b(7, 0, 0, 9, 16, 7)));
        pool.add(b(0, 0, 0, 16, 3, 16));
        pool.add(Shapes.or(b(0, 0, 0, 16, 13, 16), b(4, 13, 4, 12, 16, 12)));
        pool.add(Shapes.or(b(0, 10, 0, 16, 16, 16), b(4, 4, 4, 12, 10, 12), b(6, 0, 6, 10, 4, 10)));
        pool.add(Shapes.or(b(2, 0, 2, 14, 4, 14), b(4, 4, 5, 12, 5, 11), b(6, 5, 6, 10, 10, 10), b(3, 10, 4, 13, 16, 12)));
        pool.add(Shapes.empty());
        for (int i = 0; i < 24; i++) {
            VoxelShape s = rbox(rnd);
            int extra = rnd.nextInt(3);
            for (int k = 0; k < extra; k++) s = Shapes.or(s, rbox(rnd));
            pool.add(s);
        }
        return pool;
    }

    static VoxelShape rbox(Random rnd) {
        int x1 = rnd.nextInt(14), y1 = rnd.nextInt(14), z1 = rnd.nextInt(14);
        return b(x1, y1, z1, x1 + 1 + rnd.nextInt(16 - x1), y1 + 1 + rnd.nextInt(16 - y1), z1 + 1 + rnd.nextInt(16 - z1));
    }

    static final BooleanOp[] OPS = {BooleanOp.AND, BooleanOp.OR, BooleanOp.ONLY_FIRST, BooleanOp.ONLY_SECOND, BooleanOp.NOT_SAME, BooleanOp.FIRST, BooleanOp.SECOND};

    static long handle(VoxelShape s) {
        synchronized (HANDLES) {
            Long h = HANDLES.get(s);
            if (h == null) {
                double[] x = s.getCoords(Direction.Axis.X).toDoubleArray();
                double[] y = s.getCoords(Direction.Axis.Y).toDoubleArray();
                double[] z = s.getCoords(Direction.Axis.Z).toDoubleArray();
                h = shapeNew(x, y, z, cells(s, x.length - 1, y.length - 1, z.length - 1));
                HANDLES.put(s, h);
            }
            return h;
        }
    }

    static boolean[] cells(VoxelShape s, int nx, int ny, int nz) {
        nx = Math.max(nx, 0);
        ny = Math.max(ny, 0);
        nz = Math.max(nz, 0);
        boolean[] occupied = new boolean[nx * ny * nz];
        var grid = ((VoxelShapeAccessor) s).horizon$shape();
        int i = 0;
        for (int a = 0; a < nx; a++) {
            for (int b = 0; b < ny; b++) {
                for (int c = 0; c < nz; c++) occupied[i++] = grid.isFull(a, b, c);
            }
        }
        return occupied;
    }

    static double[][] coords(VoxelShape s) {
        return new double[][] {s.getCoords(Direction.Axis.X).toDoubleArray(), s.getCoords(Direction.Axis.Y).toDoubleArray(), s.getCoords(Direction.Axis.Z).toDoubleArray()};
    }

    static VoxelShape rebuild(double[] flat, boolean[] cells) {
        if (flat == null) return null;
        int nx = (int) flat[0], ny = (int) flat[1], nz = (int) flat[2];
        if (nx * ny * nz == 0) return Shapes.empty();
        var grid = new BitSetDiscreteVoxelShape(nx, ny, nz);
        int i = 0;
        for (int a = 0; a < nx; a++) {
            for (int b = 0; b < ny; b++) {
                for (int c = 0; c < nz; c++) {
                    if (cells[i++]) grid.fill(a, b, c);
                }
            }
        }
        int at = 3;
        double[] xs = Arrays.copyOfRange(flat, at, at += nx + 1);
        double[] ys = Arrays.copyOfRange(flat, at, at += ny + 1);
        double[] zs = Arrays.copyOfRange(flat, at, at + nz + 1);
        try {
            return (VoxelShape) ARRAY_SHAPE.newInstance(grid, xs, ys, zs);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    static VoxelShape fromRust(long a, long b, int truth, boolean optimize) {
        double[] flat = optimize ? optimize(a) : joinUnoptimized(a, b, truth);
        return rebuild(flat, flat == null ? null : takeCells());
    }

    static boolean same(VoxelShape a, VoxelShape b) {
        for (Direction.Axis axis : Direction.Axis.values()) {
            if (!Arrays.equals(a.getCoords(axis).toDoubleArray(), b.getCoords(axis).toDoubleArray())) return false;
        }
        double[][] c = coords(a);
        boolean[] ca = cells(a, c[0].length - 1, c[1].length - 1, c[2].length - 1);
        boolean[] cb = cells(b, c[0].length - 1, c[1].length - 1, c[2].length - 1);
        return Arrays.equals(ca, cb);
    }

    static void shapes(StringBuilder out) {
        Random rnd = new Random(7);
        List<VoxelShape> pool = pool(rnd);
        List<VoxelShape> all = new ArrayList<>(pool);
        for (int i = 0; i < 40; i++) all.add(pool.get(rnd.nextInt(pool.size())).move((rnd.nextInt(5) - 2) / 16.0 * 4, 0, (rnd.nextInt(5) - 2) / 16.0 * 4));
        VoxelShape[] a = new VoxelShape[M + 1];
        VoxelShape[] bb = new VoxelShape[M + 1];
        BooleanOp[] ops = new BooleanOp[M + 1];
        long[] ha = new long[M + 1];
        long[] hb = new long[M + 1];
        int[] tr = new int[M + 1];
        for (int i = 0; i <= M; i++) {
            a[i] = all.get(rnd.nextInt(all.size()));
            bb[i] = all.get(rnd.nextInt(all.size()));
            ops[i] = OPS[rnd.nextInt(OPS.length)];
            ha[i] = handle(a[i]);
            hb[i] = handle(bb[i]);
            tr[i] = truth(ops[i]);
        }
        bench(out, "Shapes.joinIsNotEmpty", 400_000,
                new String[] {"java", "rust handle in hand", "rust handle via IdentityHashMap", "rust arrays each call (no cache)"},
                new Work[] {
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += Shapes.joinIsNotEmpty(a[i & M], bb[i & M], ops[i & M]) ? 1 : 0; return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += joinNotEmpty(ha[i & M], hb[i & M], tr[i & M]); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += joinNotEmpty(handle(a[i & M]), handle(bb[i & M]), truth(ops[i & M])); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) {
                            VoxelShape x = a[i & M], y = bb[i & M];
                            double[][] cx = coords(x), cy = coords(y);
                            s += joinNotEmptyArrays(cx[0], cx[1], cx[2], cells(x, cx[0].length - 1, cx[1].length - 1, cx[2].length - 1),
                                    cy[0], cy[1], cy[2], cells(y, cy[0].length - 1, cy[1].length - 1, cy[2].length - 1), truth(ops[i & M]));
                        } return s; }});
        VoxelShape[] blockShape = new VoxelShape[M + 1];
        double[] ox = new double[M + 1], oy = new double[M + 1], oz = new double[M + 1];
        AABB[] entity = new AABB[M + 1];
        long[] hs = new long[M + 1];
        for (int i = 0; i <= M; i++) {
            blockShape[i] = pool.get(rnd.nextInt(14));
            hs[i] = handle(blockShape[i]);
            ox[i] = rnd.nextInt(2000) - 1000;
            oy[i] = rnd.nextInt(100);
            oz[i] = rnd.nextInt(2000) - 1000;
            double w = 0.3 + rnd.nextDouble() * 0.6, h = 0.5 + rnd.nextDouble() * 1.8;
            double cx = ox[i] + rnd.nextDouble() * 1.6 - 0.3, cy = oy[i] + rnd.nextDouble() * 1.6 - 0.5, cz = oz[i] + rnd.nextDouble() * 1.6 - 0.3;
            entity[i] = new AABB(cx - w / 2, cy, cz - w / 2, cx + w / 2, cy + h, cz + w / 2);
        }
        bench(out, "joinIsNotEmpty placement", 400_000,
                new String[] {"java: move(pos) vs create(box)", "rust: both extracted each call", "extraction only (no native)"},
                new Work[] {
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += Shapes.joinIsNotEmpty(blockShape[i & M].move(ox[i & M], oy[i & M], oz[i & M]), Shapes.create(entity[i & M]), BooleanOp.AND) ? 1 : 0; return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) {
                            VoxelShape x = blockShape[i & M].move(ox[i & M], oy[i & M], oz[i & M]), y = Shapes.create(entity[i & M]);
                            double[][] cx = coords(x), cy = coords(y);
                            s += joinNotEmptyArrays(cx[0], cx[1], cx[2], cells(x, cx[0].length - 1, cx[1].length - 1, cx[2].length - 1),
                                    cy[0], cy[1], cy[2], cells(y, cy[0].length - 1, cy[1].length - 1, cy[2].length - 1), 8);
                        } return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) {
                            VoxelShape x = blockShape[i & M].move(ox[i & M], oy[i & M], oz[i & M]), y = Shapes.create(entity[i & M]);
                            double[][] cx = coords(x), cy = coords(y);
                            s += cells(x, cx[0].length - 1, cx[1].length - 1, cx[2].length - 1).length + cells(y, cy[0].length - 1, cy[1].length - 1, cy[2].length - 1).length;
                        } return s; }});
        AABB[] local = new AABB[M + 1];
        for (int i = 0; i <= M; i++) {
            double w = 0.3 + rnd.nextDouble() * 0.6, h = 0.5 + rnd.nextDouble() * 1.8;
            double cx = rnd.nextDouble() * 1.6 - 0.3, cy = rnd.nextDouble() * 1.6 - 0.5, cz = rnd.nextDouble() * 1.6 - 0.3;
            local[i] = new AABB(cx - w / 2, cy, cz - w / 2, cx + w / 2, cy + h, cz + w / 2);
        }
        bench(out, "joinIsNotEmpty cached+box", 400_000,
                new String[] {"java: block shape vs create(box)", "rust: handle + box extracted each call"},
                new Work[] {
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += Shapes.joinIsNotEmpty(blockShape[i & M], Shapes.create(local[i & M]), BooleanOp.AND) ? 1 : 0; return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) {
                            VoxelShape y = Shapes.create(local[i & M]);
                            double[][] cy = coords(y);
                            s += joinNotEmptyMixed(hs[i & M], cy[0], cy[1], cy[2], cells(y, cy[0].length - 1, cy[1].length - 1, cy[2].length - 1), 8);
                        } return s; }});
        bench(out, "Shapes.joinUnoptimized", 200_000,
                new String[] {"java", "rust handles, result as arrays only", "rust handles, rebuilt as a VoxelShape", "rust via IdentityHashMap, rebuilt"},
                new Work[] {
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += Shapes.joinUnoptimized(a[i & M], bb[i & M], ops[i & M]).getCoords(Direction.Axis.X).size(); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) { double[] f = joinUnoptimized(ha[i & M], hb[i & M], tr[i & M]); s += f == null ? 0 : takeCells().length + (long) f[0]; } return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += fromRust(ha[i & M], hb[i & M], tr[i & M], false).getCoords(Direction.Axis.X).size(); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += fromRust(handle(a[i & M]), handle(bb[i & M]), truth(ops[i & M]), false).getCoords(Direction.Axis.X).size(); return s; }});
        List<VoxelShape> unopt = new ArrayList<>();
        for (int i = 0; i < 64; i++) unopt.add(Shapes.joinUnoptimized(pool.get(rnd.nextInt(pool.size())), pool.get(rnd.nextInt(pool.size())).move(rnd.nextInt(3) / 4.0, 0, 0), BooleanOp.OR));
        VoxelShape[] u = new VoxelShape[M + 1];
        long[] hu = new long[M + 1];
        for (int i = 0; i <= M; i++) {
            u[i] = unopt.get(rnd.nextInt(unopt.size()));
            hu[i] = handle(u[i]);
        }
        bench(out, "VoxelShape.optimize", 200_000,
                new String[] {"java", "rust handle, result as arrays only", "rust handle, rebuilt as a VoxelShape", "rust via IdentityHashMap, rebuilt"},
                new Work[] {
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += u[i & M].optimize().getCoords(Direction.Axis.X).size(); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) { double[] f = optimize(hu[i & M]); s += f == null ? 0 : takeCells().length + (long) f[0]; } return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += fromRust(hu[i & M], 0, 0, true).getCoords(Direction.Axis.X).size(); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += fromRust(handle(u[i & M]), 0, 0, true).getCoords(Direction.Axis.X).size(); return s; }});
        Vec3[] from = new Vec3[M + 1];
        Vec3[] to = new Vec3[M + 1];
        BlockPos[] at = new BlockPos[M + 1];
        VoxelShape[] cs = new VoxelShape[M + 1];
        long[] hc = new long[M + 1];
        for (int i = 0; i <= M; i++) {
            at[i] = new BlockPos(rnd.nextInt(2000) - 1000, rnd.nextInt(100), rnd.nextInt(2000) - 1000);
            from[i] = new Vec3(at[i].getX() + rnd.nextDouble() * 4 - 1.5, at[i].getY() + rnd.nextDouble() * 4 - 1.5, at[i].getZ() + rnd.nextDouble() * 4 - 1.5);
            to[i] = new Vec3(at[i].getX() + rnd.nextDouble() * 4 - 1.5, at[i].getY() + rnd.nextDouble() * 4 - 1.5, at[i].getZ() + rnd.nextDouble() * 4 - 1.5);
            cs[i] = pool.get(rnd.nextInt(pool.size()));
            hc[i] = handle(cs[i]);
        }
        bench(out, "VoxelShape.clip", 400_000,
                new String[] {"java", "rust handle in hand", "rust via IdentityHashMap"},
                new Work[] {
                        n -> { long s = 0; for (int i = 0; i < n; i++) { BlockHitResult r = cs[i & M].clip(from[i & M], to[i & M], at[i & M]); s += r == null ? 1 : 2 + r.getDirection().ordinal(); } return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) { BlockHitResult r = hit(clip(hc[i & M], Flat.raw(from[i & M]), Flat.raw(to[i & M]), at[i & M].asLong()), at[i & M]); s += r == null ? 1 : 2 + r.getDirection().ordinal(); } return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) { BlockHitResult r = hit(clip(handle(cs[i & M]), Flat.raw(from[i & M]), Flat.raw(to[i & M]), at[i & M].asLong()), at[i & M]); s += r == null ? 1 : 2 + r.getDirection().ordinal(); } return s; }});
    }

    static BlockHitResult hit(double[] o, BlockPos pos) {
        return o == null ? null : new BlockHitResult(new Vec3(o[0], o[1], o[2]), DIRS[(int) o[3]], pos, o[4] != 0);
    }

    // ---- noise ----

    static long noiseHandle(SimplexNoise noise, long seed) {
        synchronized (NOISES) {
            return NOISES.computeIfAbsent(noise, k -> newNoise(noise, seed));
        }
    }

    static long newNoise(SimplexNoise noise, long seed) {
        try {
            var cls = Class.forName("net.minecraft.world.level.levelgen.synth.GradientNoise");
            double[] o = new double[3];
            String[] names = {"offsetX", "offsetY", "offsetZ"};
            for (int i = 0; i < 3; i++) {
                var f = cls.getDeclaredField(names[i]);
                f.setAccessible(true);
                o[i] = f.getDouble(noise);
            }
            return simplexNew(seed, o[0], o[1], o[2]);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    static final long[] SEEDS = {1L, 42L, 8675309L, -77L, 123456789012L, 5L, 99L, 31337L};

    static void noise(StringBuilder out) {
        Random rnd = new Random(3);
        SimplexNoise[] noises = new SimplexNoise[SEEDS.length];
        long[] handles = new long[SEEDS.length];
        for (int i = 0; i < SEEDS.length; i++) {
            noises[i] = new SimplexNoise(new XoroshiroRandomSource(SEEDS[i]));
            handles[i] = noiseHandle(noises[i], SEEDS[i]);
        }
        double[] x = new double[M + 1], y = new double[M + 1], z = new double[M + 1];
        for (int i = 0; i <= M; i++) {
            x[i] = rnd.nextDouble() * 2000 - 1000;
            y[i] = rnd.nextDouble() * 2000 - 1000;
            z[i] = rnd.nextDouble() * 2000 - 1000;
        }
        bench(out, "SimplexNoise.get(x,y)", 2_000_000, new String[] {"java", "rust handle in hand", "rust via IdentityHashMap"},
                new Work[] {
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += Float.floatToRawIntBits(noises[i & 7].get(x[i & M], y[i & M])); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += Float.floatToRawIntBits(simplex2(handles[i & 7], x[i & M], y[i & M])); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += Float.floatToRawIntBits(simplex2(noiseHandle(noises[i & 7], 0), x[i & M], y[i & M])); return s; }});
        bench(out, "SimplexNoise.get(x,y,z)", 2_000_000, new String[] {"java", "rust handle in hand", "rust via IdentityHashMap"},
                new Work[] {
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += Float.floatToRawIntBits(noises[i & 7].get(x[i & M], y[i & M], z[i & M])); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += Float.floatToRawIntBits(simplex3(handles[i & 7], x[i & M], y[i & M], z[i & M])); return s; },
                        n -> { long s = 0; for (int i = 0; i < n; i++) s += Float.floatToRawIntBits(simplex3(noiseHandle(noises[i & 7], 0), x[i & M], y[i & M], z[i & M])); return s; }});
    }

    // ---- parity ----

    static void parity(StringBuilder out) {
        Random rnd = new Random(99);
        int bad;
        String first;

        bad = 0;
        first = "";
        for (int i = 0; i < 5000; i++) {
            Vec3 a = new Vec3(rnd.nextGaussian() * 50, rnd.nextGaussian() * 50, rnd.nextGaussian() * 50);
            Vec3 c = new Vec3(rnd.nextGaussian() * 50, rnd.nextGaussian() * 50, rnd.nextGaussian() * 50);
            double[] o = vec3Add(Flat.raw(a), Flat.raw(c));
            Vec3 j = a.add(c);
            if (bits(o[0]) != bits(j.x) || bits(o[1]) != bits(j.y) || bits(o[2]) != bits(j.z)) {
                bad++;
                first = a + " + " + c;
            }
            AABB box = new AABB(a, c);
            double[] m = aabbMove(new double[] {box.minX, box.minY, box.minZ, box.maxX, box.maxY, box.maxZ}, Flat.raw(a));
            AABB jm = box.move(a);
            if (bits(m[0]) != bits(jm.minX) || bits(m[1]) != bits(jm.minY) || bits(m[2]) != bits(jm.minZ) || bits(m[3]) != bits(jm.maxX) || bits(m[4]) != bits(jm.maxY)
                    || bits(m[5]) != bits(jm.maxZ)) {
                bad++;
                first = "move " + box + " by " + a;
            }
            if (bits(vec3DistSq(a.x, a.y, a.z, c.x, c.y, c.z)) != bits(a.distanceToSqr(c))) {
                bad++;
                first = "distSq " + a + " " + c;
            }
        }
        out.append("parity Vec3.add / AABB.move / Vec3.distanceToSqr: mismatches=").append(bad).append(' ').append(first).append('\n');

        bad = 0;
        first = "";
        for (int i = 0; i < 20000; i++) {
            double y = i % 4 == 0 ? rnd.nextInt(5) - 2 : rnd.nextGaussian() * 10;
            double x = i % 5 == 0 ? rnd.nextInt(5) - 2 : rnd.nextGaussian() * 10;
            if (bits(atan2(y, x)) != bits(Mth.atan2(y, x))) {
                bad++;
                first = "atan2(" + y + "," + x + ") rust=" + atan2(y, x) + " java=" + Mth.atan2(y, x);
            }
        }
        out.append("parity Mth.atan2 (20000): mismatches=").append(bad).append(' ').append(first).append('\n');

        bad = 0;
        first = "";
        for (int i = 0; i < 20000; i++) {
            BlockPos p = new BlockPos(rnd.nextInt(60000000) - 30000000, rnd.nextInt(4096) - 2048, rnd.nextInt(60000000) - 30000000);
            Direction d = DIRS[rnd.nextInt(6)];
            int n = rnd.nextInt(200) - 100;
            BlockPos j = p.relative(d, n);
            BlockPos r = BlockPos.of(blockRelative(p.asLong(), d.ordinal(), n));
            if (!j.equals(r)) {
                bad++;
                first = p + " " + d + " " + n + " java=" + j + " rust=" + r;
            }
        }
        out.append("parity BlockPos.relative (20000): mismatches=").append(bad).append(' ').append(first).append('\n');

        List<VoxelShape> pool = pool(rnd);
        List<VoxelShape> all = new ArrayList<>(pool);
        for (int i = 0; i < 200; i++) {
            all.add(pool.get(rnd.nextInt(pool.size())).move((rnd.nextInt(33) - 16) / 16.0, (rnd.nextInt(33) - 16) / 16.0, (rnd.nextInt(33) - 16) / 16.0));
        }
        int checked = 0;
        bad = 0;
        first = "";
        int errors = 0;
        for (int i = 0; i < 6000; i++) {
            VoxelShape x = all.get(rnd.nextInt(all.size())), y = all.get(rnd.nextInt(all.size()));
            BooleanOp op = OPS[rnd.nextInt(OPS.length)];
            int r = joinNotEmpty(handle(x), handle(y), truth(op));
            checked++;
            if (r < 0) {
                errors++;
                continue;
            }
            if ((r == 1) != Shapes.joinIsNotEmpty(x, y, op)) {
                bad++;
                first = x + " | " + y + " op=" + truth(op);
            }
        }
        out.append("parity Shapes.joinIsNotEmpty (").append(checked).append("): mismatches=").append(bad).append(" rust_errors=").append(errors).append(' ').append(first).append('\n');

        bad = 0;
        first = "";
        errors = 0;
        int emptyForm = 0;
        for (int i = 0; i < 3000; i++) {
            VoxelShape x = all.get(rnd.nextInt(all.size())), y = all.get(rnd.nextInt(all.size()));
            BooleanOp op = OPS[rnd.nextInt(OPS.length)];
            VoxelShape r = fromRust(handle(x), handle(y), truth(op), false);
            if (r == null) {
                errors++;
                continue;
            }
            VoxelShape j = Shapes.joinUnoptimized(x, y, op);
            if (j.isEmpty() && r.isEmpty() && !same(j, r)) {
                emptyForm++;
            } else if (!same(j, r)) {
                bad++;
                first = x + " | " + y + " op=" + truth(op) + " javaX=" + j.getCoords(Direction.Axis.X) + " rustX=" + r.getCoords(Direction.Axis.X) + " javaY=" + j.getCoords(Direction.Axis.Y) + " rustY=" + r.getCoords(Direction.Axis.Y) + " javaZ=" + j.getCoords(Direction.Axis.Z) + " rustZ=" + r.getCoords(Direction.Axis.Z) + " javaCls=" + j.getClass().getSimpleName();
            }
        }
        out.append("parity Shapes.joinUnoptimized (3000, coords+cells): mismatches=").append(bad).append(" empty_in_both_but_coords_differ=").append(emptyForm).append(" rust_errors=").append(errors).append(' ').append(first).append('\n');

        bad = 0;
        first = "";
        errors = 0;
        for (int i = 0; i < 3000; i++) {
            VoxelShape x = Shapes.joinUnoptimized(all.get(rnd.nextInt(all.size())), all.get(rnd.nextInt(all.size())), BooleanOp.OR);
            VoxelShape r = fromRust(handle(x), 0, 0, true);
            if (r == null) {
                errors++;
                continue;
            }
            if (!same(x.optimize(), r)) {
                bad++;
                first = x.toString();
            }
        }
        out.append("parity VoxelShape.optimize (3000, coords+cells): mismatches=").append(bad).append(" rust_errors=").append(errors).append(' ').append(first).append('\n');

        bad = 0;
        first = "";
        int hits = 0;
        for (int i = 0; i < 20000; i++) {
            VoxelShape s = pool.get(rnd.nextInt(pool.size()));
            BlockPos p = new BlockPos(rnd.nextInt(2000) - 1000, rnd.nextInt(100), rnd.nextInt(2000) - 1000);
            double grid = rnd.nextInt(3) == 0 ? 16.0 : 0;
            Vec3 f = grid > 0 ? new Vec3(p.getX() + rnd.nextInt(33) / 16.0 - 0.5, p.getY() + rnd.nextInt(33) / 16.0 - 0.5, p.getZ() + rnd.nextInt(33) / 16.0 - 0.5)
                    : new Vec3(p.getX() + rnd.nextDouble() * 4 - 1.5, p.getY() + rnd.nextDouble() * 4 - 1.5, p.getZ() + rnd.nextDouble() * 4 - 1.5);
            Vec3 t = grid > 0 ? new Vec3(p.getX() + rnd.nextInt(33) / 16.0 - 0.5, p.getY() + rnd.nextInt(33) / 16.0 - 0.5, p.getZ() + rnd.nextInt(33) / 16.0 - 0.5)
                    : new Vec3(p.getX() + rnd.nextDouble() * 4 - 1.5, p.getY() + rnd.nextDouble() * 4 - 1.5, p.getZ() + rnd.nextDouble() * 4 - 1.5);
            BlockHitResult j = s.clip(f, t, p);
            BlockHitResult r = hit(clip(handle(s), Flat.raw(f), Flat.raw(t), p.asLong()), p);
            if (j != null) hits++;
            boolean ok = (j == null) == (r == null)
                    && (j == null || (bits(j.getLocation().x) == bits(r.getLocation().x) && bits(j.getLocation().y) == bits(r.getLocation().y)
                            && bits(j.getLocation().z) == bits(r.getLocation().z) && j.getDirection() == r.getDirection() && j.isInside() == r.isInside()));
            if (!ok) {
                bad++;
                first = s + " from " + f + " to " + t + " at " + p + " java=" + j + " rust=" + r;
            }
        }
        out.append("parity VoxelShape.clip (20000, ").append(hits).append(" hits): mismatches=").append(bad).append(' ').append(first).append('\n');

        bad = 0;
        first = "";
        for (int s = 0; s < SEEDS.length; s++) {
            SimplexNoise java = new SimplexNoise(new XoroshiroRandomSource(SEEDS[s]));
            long h = newNoise(java, SEEDS[s]);
            for (int i = 0; i < 2000; i++) {
                double x = rnd.nextDouble() * 4000 - 2000, y = rnd.nextDouble() * 4000 - 2000, z = rnd.nextDouble() * 4000 - 2000;
                if (Float.floatToRawIntBits(java.get(x, y)) != Float.floatToRawIntBits(simplex2(h, x, y))
                        || Float.floatToRawIntBits(java.get(x, y, z)) != Float.floatToRawIntBits(simplex3(h, x, y, z))) {
                    bad++;
                    first = "seed " + SEEDS[s] + " " + x + "," + y + "," + z + " java2=" + java.get(x, y) + " rust2=" + simplex2(h, x, y) + " java3=" + java.get(x, y, z) + " rust3=" + simplex3(h, x, y, z);
                }
            }
        }
        out.append("parity SimplexNoise.get 2d+3d (16000): mismatches=").append(bad).append(' ').append(first).append('\n');
    }
}
