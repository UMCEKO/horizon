package dev.horizonlab.javamods.unit;

import java.lang.reflect.Method;
import java.util.Map;
import net.minecraft.client.gui.Font;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.FontDescription;
import net.minecraft.network.chat.Style;
import net.minecraft.resources.Identifier;

/** Each font measures with its own advances, an unknown font with the missing glyph, a sprite with 8. */
public final class FontTablesCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private static int width(Font font, Style style) {
        return font.width(Component.literal("aa").withStyle(style));
    }

    public static void main(String[] args) throws Exception {
        var loader = Class.forName("dev.horizonlab.javamods.boot.ClientLoader");
        Method fonts = loader.getDeclaredMethod("fonts", float.class, Map.class);
        fonts.setAccessible(true);
        float[] def = new float['a' + 1];
        def['a'] = 6;
        float[] uniform = new float['a' + 1];
        uniform['a'] = 4;
        fonts.invoke(null, 6.0F, Map.of(
                Identifier.withDefaultNamespace("default"), def,
                Identifier.withDefaultNamespace("uniform"), uniform));
        Method make = loader.getDeclaredMethod("font");
        make.setAccessible(true);
        Font font = (Font) make.invoke(null);
        check(width(font, Style.EMPTY) == 12, "default");
        check(width(font, Style.EMPTY.withFont(new FontDescription.Resource(Identifier.withDefaultNamespace("uniform")))) == 8, "uniform");
        check(width(font, Style.EMPTY.withFont(new FontDescription.Resource(Identifier.parse("nomod:nofont")))) == 12, "unknown font is missing");
        var sprite = new FontDescription.AtlasSprite(Identifier.withDefaultNamespace("blocks"), Identifier.withDefaultNamespace("block/stone"));
        check(font.width(Component.literal("￼").withStyle(Style.EMPTY.withFont(sprite))) == 8, "sprite");
        var mixed = Component.literal("a").append(Component.literal("a").withStyle(Style.EMPTY.withFont(new FontDescription.Resource(Identifier.withDefaultNamespace("uniform")))));
        check(font.width(mixed) == 10, "mixed runs");
        System.out.println("FontTablesCheck ok");
    }
}
