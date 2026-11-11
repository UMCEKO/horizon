package dev.horizonlab.javamods.boot;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.util.HashSet;
import java.util.Set;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;

/** `client_shape` replies carry the edges vanilla outlines the shape with. */
public final class ShapeEdgesCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private static Set<String> edges(VoxelShape shape) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(ClientBridge.shapeReply(shape)));
        in.readByte();
        int boxes = in.readInt();
        for (int i = 0; i < boxes * 6; i++) in.readDouble();
        int n = in.readInt();
        var set = new HashSet<String>();
        for (int i = 0; i < n; i++) {
            var sb = new StringBuilder();
            for (int j = 0; j < 6; j++) sb.append(in.readDouble()).append(',');
            set.add(sb.toString());
        }
        check(set.size() == n, "edges are distinct");
        check(in.available() == 0, "nothing trails the edges");
        return set;
    }

    private static void edge(Set<String> want, double... v) {
        var sb = new StringBuilder();
        for (double d : v) sb.append(d).append(',');
        want.add(sb.toString());
    }

    public static void main(String[] args) throws Exception {
        var want = new HashSet<String>();
        double[] ys = {0, 0.5};
        for (double y : ys) {
            edge(want, 0, y, 0, 1, y, 0);
            edge(want, 0, y, 1, 1, y, 1);
            edge(want, 0, y, 0, 0, y, 1);
            edge(want, 1, y, 0, 1, y, 1);
        }
        edge(want, 0, 0, 0, 0, 0.5, 0);
        edge(want, 1, 0, 0, 1, 0.5, 0);
        edge(want, 0, 0, 1, 0, 0.5, 1);
        edge(want, 1, 0, 1, 1, 0.5, 1);
        var got = edges(Shapes.box(0, 0, 0, 1, 0.5, 1));
        check(got.equals(want), "half slab edges, got " + got);
        var two = edges(Shapes.or(Shapes.box(0, 0, 0, 0.25, 0.25, 0.25), Shapes.box(0.75, 0.75, 0.75, 1, 1, 1)));
        check(two.size() == 24, "two boxes give 24 edges, got " + two.size());
    }
}
