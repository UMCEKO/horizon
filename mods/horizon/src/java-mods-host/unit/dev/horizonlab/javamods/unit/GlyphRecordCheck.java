package dev.horizonlab.javamods.unit;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import net.minecraft.client.gui.Font;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.Style;
import net.minecraft.resources.Identifier;
import net.minecraft.util.ARGB;
import org.joml.Matrix3x2f;

/** With text routed to glyphs, a text records each glyph at vanilla's position and colours, and its underline as one effect a glyph. */
public final class GlyphRecordCheck {
    private record G(int codepoint, String font, float x, float y, int color, int shadow, int flags, float bold, float offset) {}

    private record E(boolean behind, float x0, float y0, float x1, float y1, int color, int shadow, float offset) {}

    private record Recorded(float[] pose, List<G> glyphs, List<E> effects) {}

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private static String text(DataInputStream in) throws Exception {
        byte[] b = new byte[in.readInt()];
        in.readFully(b);
        return new String(b, java.nio.charset.StandardCharsets.UTF_8);
    }

    private static Recorded record(Font font, Style style, int color, boolean shadow, int background) throws Exception {
        var recorderClass = Class.forName("dev.horizonlab.javamods.boot.Recorder");
        var recorder = recorderClass.getDeclaredConstructor().newInstance();
        var state = new net.minecraft.client.renderer.state.gui.GuiTextRenderState(font, Component.literal("Ab").withStyle(style).getVisualOrderText(), new Matrix3x2f(), 0, 0, color, background, shadow, false, null);
        ((net.minecraft.client.renderer.state.gui.GuiRenderState) recorder).addText(state);
        Method bytes = recorderClass.getDeclaredMethod("bytes");
        bytes.setAccessible(true);
        var in = new DataInputStream(new ByteArrayInputStream((byte[]) bytes.invoke(recorder)));
        check(in.readInt() == 1, "one entry");
        check(in.readByte() == 5, "a GLYPHS entry");
        float[] pose = {in.readFloat(), in.readFloat(), in.readFloat(), in.readFloat()};
        var glyphs = new ArrayList<G>();
        for (int n = in.readInt(); n > 0; n--) {
            glyphs.add(new G(in.readInt(), text(in), in.readFloat(), in.readFloat(), in.readInt(), in.readInt(), in.readByte(), in.readFloat(), in.readFloat()));
        }
        var effects = new ArrayList<E>();
        for (int n = in.readInt(); n > 0; n--) {
            effects.add(new E(in.readBoolean(), in.readFloat(), in.readFloat(), in.readFloat(), in.readFloat(), in.readInt(), in.readInt(), in.readFloat()));
        }
        check(!in.readBoolean(), "no scissor");
        return new Recorded(pose, glyphs, effects);
    }

    public static void main(String[] args) throws Exception {
        var loader = Class.forName("dev.horizonlab.javamods.boot.ClientLoader");
        Method fonts = loader.getDeclaredMethod("fonts", float.class, Map.class);
        fonts.setAccessible(true);
        float[] table = new float['b' + 1];
        table['A'] = 6;
        table['b'] = 5;
        fonts.invoke(null, 6.0F, Map.of(Identifier.withDefaultNamespace("default"), table));
        Method make = loader.getDeclaredMethod("font");
        make.setAccessible(true);
        Font font = (Font) make.invoke(null);
        var routed = Class.forName("dev.horizonlab.javamods.boot.Glyphs").getDeclaredField("routed");
        routed.setAccessible(true);
        routed.setBoolean(null, true);

        int red = 0xFFFF0000;
        var plain = record(font, Style.EMPTY, red, true, 0);
        check(plain.glyphs.size() == 2 && plain.effects.isEmpty(), "two glyphs, no effects");
        check(plain.glyphs.get(0).codepoint == 'A' && plain.glyphs.get(1).codepoint == 'b', "code points");
        check(plain.glyphs.get(0).x == 0 && plain.glyphs.get(1).x == 6, "advances");
        check(plain.glyphs.get(0).font.equals("minecraft:default"), "font id");
        for (var g : plain.glyphs) {
            check(g.color == red, "colour");
            check(g.shadow == ARGB.scaleRGB(red, 0.25F), "vanilla's shadow colour");
        }
        check(record(font, Style.EMPTY, red, false, 0).glyphs.get(0).shadow == 0, "no shadow when off");

        var bold = record(font, Style.EMPTY.withBold(true), red, true, 0);
        check(bold.glyphs.get(1).x == 7, "a bold glyph advances by its bold offset");
        check(bold.glyphs.get(0).flags == 1 && bold.glyphs.get(0).bold == 1.0F, "bold flag and offset");

        var under = record(font, Style.EMPTY.withUnderlined(true), red, true, 0);
        check(under.effects.size() == 2, "one effect a glyph");
        check(under.effects.get(0).x0 == -1 && under.effects.get(0).x1 == 6, "first underline");
        check(under.effects.get(1).x0 == 6 && under.effects.get(1).x1 == 11, "second underline");
        check(under.effects.get(0).y0 == 8 && under.effects.get(0).y1 == 9, "underline rows");
        check(!under.effects.get(0).behind, "an underline is drawn over the glyphs");

        var backed = record(font, Style.EMPTY, red, false, 0x80000000);
        check(backed.effects.size() == 1 && backed.effects.get(0).behind && backed.effects.get(0).color == 0x80000000, "background first");
        System.out.println("GlyphRecordCheck ok");
    }
}
