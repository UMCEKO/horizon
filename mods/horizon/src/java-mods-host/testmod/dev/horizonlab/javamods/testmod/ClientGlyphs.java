package dev.horizonlab.javamods.testmod;

import java.util.ArrayList;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.font.TextRenderable;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.common.NeoForge;

/** A mod patching the text builder: its shadow colour reaches the glyphs the builder records, for its own font only. */
public final class ClientGlyphs {
    private ClientGlyphs() {}

    /** The font the shadow patch looks for. */
    public static final class ShadowFont extends Font {
        ShadowFont(Font.Provider provider) {
            super(provider);
        }
    }

    private static boolean done;

    static void register() {
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Post event) -> {
            Minecraft minecraft = Minecraft.getInstance();
            if (done || minecraft.player == null) return;
            done = true;
            Checks.check("glyph_shadow_from_patch", () -> {
                var plain = minecraft.font;
                ShadowFont patched;
                try {
                    var field = Font.class.getDeclaredField("provider");
                    field.setAccessible(true);
                    patched = new ShadowFont((Font.Provider) field.get(plain));
                } catch (ReflectiveOperationException e) {
                    throw new IllegalStateException(e);
                }
                int expected = net.minecraft.util.ARGB.scaleRGB(0xFFFF0000, 0.25F);
                return shadows(patched).stream().allMatch(c -> c == 0xFF00FF00) && shadows(plain).stream().allMatch(c -> c == expected);
            });
        });
    }

    /** The shadow colour of each glyph of "Ab" drawn red with a shadow; 0 when the builder made no glyph. */
    private static ArrayList<Integer> shadows(Font font) {
        var found = new ArrayList<Integer>();
        font.prepareText(net.minecraft.network.chat.Component.literal("Ab").getVisualOrderText(), 0, 0, 0xFFFF0000, true, false, 0).visit(new Font.GlyphVisitor() {
            @Override
            public void acceptGlyph(TextRenderable.Styled glyph) {
                try {
                    found.add((Integer) glyph.getClass().getMethod("shadowColor").invoke(glyph));
                } catch (ReflectiveOperationException e) {
                    throw new IllegalStateException(e);
                }
            }
        });
        if (found.isEmpty()) found.add(0);
        return found;
    }
}
