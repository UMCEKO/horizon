package dev.horizonlab.javamods.boot;

import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;

/** A strip or fan of n vertices is n-2 triangles drawn once each; triangles and quads group by their length. */
public final class RecorderPrimitivesCheck {
    private static void expect(PrimitiveTopology topology, int vertices, int primitives) {
        int[][] got = Recorder.primitives(topology, vertices);
        if (got.length != primitives) throw new AssertionError(topology + " " + vertices + " -> " + got.length);
    }

    public static void main(String[] args) {
        expect(PrimitiveTopology.TRIANGLE_STRIP, 6, 4);
        expect(PrimitiveTopology.TRIANGLE_FAN, 6, 4);
        expect(PrimitiveTopology.TRIANGLES, 9, 3);
        expect(PrimitiveTopology.QUADS, 8, 2);
        expect(PrimitiveTopology.TRIANGLE_STRIP, 2, 0);
        int[][] strip = Recorder.primitives(PrimitiveTopology.TRIANGLE_STRIP, 6);
        for (int k = 0; k < strip.length; k++) {
            if (strip[k][0] != k || strip[k][1] != k + 1 || strip[k][2] != k + 2 || strip[k][3] != strip[k][2]) throw new AssertionError("strip " + k);
        }
        if (Recorder.primitives(PrimitiveTopology.POINTS, 4) != null) throw new AssertionError("points");
        System.out.println("RecorderPrimitivesCheck ok");
    }
}
