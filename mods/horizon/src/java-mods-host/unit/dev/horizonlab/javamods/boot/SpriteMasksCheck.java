package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.platform.Transparency;

/** A 16x16 sprite with holes and a translucent patch answers its pixels' transparency from the mask exactly as the pixels do. */
public final class SpriteMasksCheck {
    private static final int W = 16, H = 16;

    private static int alpha(int x, int y) {
        if (x >= 4 && x < 6 && y >= 8 && y < 10) return 0;
        if (x == 15 && y == 15) return 100;
        if (x == 0 && y == 0) return 1;
        if (x == 7 && y == 7) return 254;
        return 255;
    }

    private static Transparency reference(int x0, int y0, int x1, int y1) {
        boolean transparent = false, translucent = false;
        for (int y = y0; y < y1; y++) for (int x = x0; x < x1; x++) {
            int a = alpha(x, y);
            if (a == 0) transparent = true;
            else if (a != 255) translucent = true;
        }
        return Transparency.of(transparent, translucent);
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) {
        var mask = SpriteMasks.Mask.of(W, H, SpriteMasksCheck::alpha);
        for (int y = 0; y < H; y++) for (int x = 0; x < W; x++) {
            check((alpha(x, y) == 0) == (mask.alpha(x, y) == 0), "transparent at " + x + "," + y);
            check((alpha(x, y) == 255) == (mask.alpha(x, y) == 255), "opaque at " + x + "," + y);
        }
        for (int x0 = 0; x0 < W; x0 += 3) for (int y0 = 0; y0 < H; y0 += 5) for (int x1 = x0; x1 <= W; x1 += 4) for (int y1 = y0; y1 <= H; y1 += 3)
            check(mask.transparency(x0, y0, x1, y1).equals(reference(x0, y0, x1, y1)), "region " + x0 + "," + y0 + "," + x1 + "," + y1);
        check(mask.transparency(0, 0, W, H).equals(Transparency.TRANSPARENT_AND_TRANSLUCENT), "whole sprite");
        check(mask.transparency(8, 0, 12, 4).isOpaque(), "an opaque corner");
        boolean threw = false;
        try {
            mask.transparency(0, 0, W + 1, H);
        } catch (IllegalArgumentException e) {
            threw = true;
        }
        check(threw, "out of bounds throws as the image does");
        System.out.println("SpriteMasksCheck ok");
    }
}
