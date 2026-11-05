package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.platform.NativeImage;
import com.mojang.blaze3d.platform.Transparency;
import java.util.Collections;
import java.util.Map;
import java.util.WeakHashMap;
import java.util.function.IntBinaryOperator;
import net.minecraft.client.renderer.texture.SpriteContents;

/** What vanilla asks a sprite's pixels after the stitch (`isTransparent`, `computeTransparency`), kept at 2 bits a pixel once the pixels are freed. */
public final class SpriteMasks {
    private SpriteMasks() {}

    /** Per pixel, row by row: whether its alpha is 0, and whether it is 255. */
    public record Mask(int width, int height, long[] clear, long[] opaque) {
        public static Mask of(int width, int height, IntBinaryOperator alphaAt) {
            int words = (width * height + 63) >>> 6;
            var mask = new Mask(width, height, new long[words], new long[words]);
            for (int y = 0; y < height; y++) {
                for (int x = 0; x < width; x++) {
                    int alpha = alphaAt.applyAsInt(x, y), i = y * width + x;
                    if (alpha == 0) mask.clear[i >>> 6] |= 1L << i;
                    else if (alpha == 255) mask.opaque[i >>> 6] |= 1L << i;
                }
            }
            return mask;
        }

        /** An alpha with the same answers as the pixel's: 0, 255, or 128 for a translucent one. */
        public int alpha(int x, int y) {
            int i = y * width + x;
            return (clear[i >>> 6] >>> i & 1) != 0 ? 0 : (opaque[i >>> 6] >>> i & 1) != 0 ? 255 : 128;
        }

        /** As `NativeImage.computeTransparency` answers for the region. */
        public Transparency transparency(int x0, int y0, int x1, int y1) {
            if (x0 < 0 || y0 < 0 || x1 > width || y1 > height) throw new IllegalArgumentException("Cannot compute translucency out of bounds: [" + x0 + ", " + y0 + ", " + x1 + ", " + y1 + "] in " + width + "x" + height + " image");
            boolean transparent = false, translucent = false;
            for (int y = y0; y < y1; y++) {
                for (int x = x0; x < x1; x++) {
                    int alpha = alpha(x, y);
                    if (alpha == 0) transparent = true;
                    else if (alpha != 255) translucent = true;
                }
            }
            return Transparency.of(transparent, translucent);
        }
    }

    private static final Map<SpriteContents, Mask> MASKS = Collections.synchronizedMap(new WeakHashMap<>());

    /** The mask of a sprite whose pixels were freed, or null. */
    public static Mask of(SpriteContents contents) {
        return MASKS.get(contents);
    }

    /** Takes the mask of an RGBA image and returns whether the sprite may free it. */
    static boolean keep(SpriteContents contents, NativeImage image) {
        if (image.format() != NativeImage.Format.RGBA) return false;
        MASKS.put(contents, Mask.of(image.getWidth(), image.getHeight(), (x, y) -> net.minecraft.util.ARGB.alpha(image.getPixel(x, y))));
        return true;
    }
}
