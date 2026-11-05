package dev.horizonlab.facts;

import java.io.StringWriter;

/** The streamed quads are the text the Gson tree gave: floats, HTML escapes, and a culled and an unculled quad. */
public final class BakedWriterCheck {
    private static final String EXPECTED = "[{\"v\":[[0.0,0.0,0.0,0.0,0.0],[1.0,0.0,0.0,1.0,0.0],[1.0,1.0,0.0,1.0,1.0],[0.0,1.0,0.0,0.0,1.0]],"
            + "\"sprite\":\"mod:block/a\\u003db\",\"face\":\"north\",\"cull\":\"north\",\"tint\":-1,\"shade\":true,\"emission\":0,\"layer\":\"solid\"},"
            + "{\"v\":[[0.1,0.25,1.0E-4,0.5,0.75],[1.0,0.0,0.0,1.0,0.0],[1.0,1.0,0.0,1.0,1.0],[0.0,1.0,0.0,0.0,1.0]],"
            + "\"sprite\":\"mod:block/c\",\"face\":\"up\",\"tint\":2,\"shade\":false,\"emission\":15,\"layer\":\"translucent\"}]";

    public static void main(String[] args) throws Exception {
        var text = new StringWriter();
        var w = BakedWriter.writer(text);
        w.beginArray();
        var square = new float[] {0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 0, 1};
        BakedWriter.writeQuad(w, square, "mod:block/a=b", "north", "north", -1, true, 0, "solid");
        var second = square.clone();
        second[0] = 0.1f;
        second[1] = 0.25f;
        second[2] = 0.0001f;
        second[3] = 0.5f;
        second[4] = 0.75f;
        BakedWriter.writeQuad(w, second, "mod:block/c", "up", null, 2, false, 15, "translucent");
        w.endArray();
        w.flush();
        if (!text.toString().equals(EXPECTED)) throw new AssertionError("got " + text);
    }
}
