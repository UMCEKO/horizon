package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.boot.mixin.VoxelShapeAccessor;
import dev.horizonlab.javamods.host.Host;
import net.minecraft.core.Direction;
import net.minecraft.world.phys.shapes.VoxelShape;

/** Rust's copy of a shape, per thread and bounded: a shape seen again costs a lookup, one seen once costs its extraction. */
final class ShapeHandles {
    private static final int SLOTS = 4096;
    private static final ThreadLocal<ShapeHandles> CACHE = ThreadLocal.withInitial(ShapeHandles::new);

    private final VoxelShape[] shapes = new VoxelShape[SLOTS];
    private final long[] handles = new long[SLOTS];

    static long of(VoxelShape shape) {
        ShapeHandles cache = CACHE.get();
        int slot = (System.identityHashCode(shape) * 0x9E3779B1) >>> 20;
        if (cache.shapes[slot] == shape) return cache.handles[slot];
        if (cache.shapes[slot] != null) Host.shapeFree(cache.handles[slot]);
        long handle = extract(shape);
        cache.shapes[slot] = shape;
        cache.handles[slot] = handle;
        return handle;
    }

    private static long extract(VoxelShape shape) {
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
        return Host.shapeHandle(x, y, z, occupied);
    }
}
