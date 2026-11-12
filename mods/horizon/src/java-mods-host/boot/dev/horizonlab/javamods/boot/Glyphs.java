package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.font.TextRenderable;
import net.minecraft.client.gui.font.glyphs.EffectGlyph;
import net.minecraft.client.renderer.rendertype.RenderType;
import net.minecraft.network.chat.Style;
import net.minecraft.resources.Identifier;
import org.joml.Matrix4fc;

/** Text kept glyph by glyph, as the (possibly mod-patched) `PreparedTextBuilder` computed it; Rust draws these. */
public final class Glyphs {
    private Glyphs() {}

    private static final String[] KEYS = {
        "net/minecraft/client/gui/Font$PreparedTextBuilder#accept(ILnet/minecraft/network/chat/Style;Lnet/minecraft/client/gui/font/glyphs/BakedGlyph;)Z",
        "net/minecraft/client/gui/Font$PreparedTextBuilder#getShadowColor(Lnet/minecraft/network/chat/Style;I)I",
    };

    /** Whether a loaded mod patches the builder, so text is recorded as glyphs. */
    static volatile boolean routed;

    /** Runs once the routes are known. */
    static void decide() {
        var on = new java.util.ArrayList<String>();
        for (String key : KEYS) if (Routing.route(key) >= 0) on.add(key.substring(key.indexOf('#') + 1));
        routed = !on.isEmpty();
        if (routed) System.err.println("Horizon: text recorded as glyphs (" + String.join(", ", on) + ")");
    }

    private static RuntimeException unseen() {
        return new UnsupportedOperationException("a recorded glyph is drawn by Horizon, not by Java");
    }

    public record Glyph(int codepoint, Identifier font, float x, float y, int color, int shadowColor, Style style, float boldOffset, float shadowOffset, float advance)
            implements TextRenderable.Styled {
        @Override
        public void render(Matrix4fc pose, VertexConsumer buffer, int packedLightCoords, boolean flat) {
            throw unseen();
        }

        @Override
        public RenderType renderType(Font.DisplayMode displayMode) {
            throw unseen();
        }

        @Override
        public GpuTextureView textureView() {
            throw unseen();
        }

        @Override
        public RenderPipeline guiPipeline() {
            throw unseen();
        }

        @Override
        public float left() {
            return x;
        }

        @Override
        public float top() {
            return y;
        }

        @Override
        public float right() {
            return x + advance;
        }

        @Override
        public float bottom() {
            return y + 8.0F;
        }
    }

    public record Effect(float x0, float y0, float x1, float y1, int color, int shadowColor, float shadowOffset) implements TextRenderable {
        @Override
        public void render(Matrix4fc pose, VertexConsumer buffer, int packedLightCoords, boolean flat) {
            throw unseen();
        }

        @Override
        public RenderType renderType(Font.DisplayMode displayMode) {
            throw unseen();
        }

        @Override
        public GpuTextureView textureView() {
            throw unseen();
        }

        @Override
        public RenderPipeline guiPipeline() {
            throw unseen();
        }

        @Override
        public float left() {
            return x0;
        }

        @Override
        public float top() {
            return y0;
        }

        @Override
        public float right() {
            return x1;
        }

        @Override
        public float bottom() {
            return y1;
        }
    }

    /** Effects Java does not draw: each becomes an `Effect`. */
    static final EffectGlyph EFFECTS = (x0, y0, x1, y1, depth, color, shadowColor, shadowOffset) -> new Effect(x0, y0, x1, y1, color, shadowColor, shadowOffset);
}
