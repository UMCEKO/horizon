package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.vertex.VertexConsumer;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.charset.StandardCharsets;
import net.minecraft.client.gui.navigation.ScreenRectangle;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.state.gui.BlitRenderState;
import net.minecraft.client.renderer.state.gui.GuiElementRenderState;
import net.minecraft.client.renderer.state.gui.GuiItemRenderState;
import net.minecraft.client.renderer.state.gui.GuiRenderState;
import net.minecraft.client.renderer.state.gui.GuiTextRenderState;
import net.minecraft.client.renderer.state.gui.pip.PictureInPictureRenderState;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.network.chat.FontDescription;
import net.minecraft.network.chat.Style;
import net.minecraft.util.FormattedCharSequence;
import net.minecraft.world.item.ItemStack;
import org.joml.Matrix3x2fc;

/** A frame's GUI as the extractor submits it, in call order: quads with their texture's id, text with its styles, items; Rust draws them. */
public final class Recorder extends GuiRenderState {
    private static final int QUAD = 0, TEXT = 1, ITEM = 2;
    private static final Field TEXT_TEXT = field("text"), TEXT_X = field("x"), TEXT_Y = field("y"), TEXT_COLOR = field("color"),
            TEXT_BACKGROUND = field("backgroundColor"), TEXT_SHADOW = field("dropShadow");

    private final ByteArrayOutputStream bytes = new ByteArrayOutputStream();
    private final DataOutputStream out = new DataOutputStream(bytes);
    private int count;

    private static Field field(String name) {
        try {
            Field f = GuiTextRenderState.class.getDeclaredField(name);
            f.setAccessible(true);
            return f;
        } catch (NoSuchFieldException e) {
            throw new IllegalStateException(e);
        }
    }

    private static void text(DataOutputStream out, String value) throws IOException {
        byte[] b = value.getBytes(StandardCharsets.UTF_8);
        out.writeInt(b.length);
        out.write(b);
    }

    private void scissor(ScreenRectangle area) throws IOException {
        out.writeBoolean(area != null);
        if (area != null) {
            out.writeInt(area.left());
            out.writeInt(area.top());
            out.writeInt(area.right());
            out.writeInt(area.bottom());
        }
    }

    /** The corners `buildVertices` makes, four to a quad; a tiled blit makes one quad a tile. */
    private static final class Corners implements VertexConsumer {
        float[] x = new float[8], y = new float[8], u = new float[8], v = new float[8];
        int[] color = new int[8];
        int n = -1;

        @Override
        public VertexConsumer addVertex(float px, float py, float pz) {
            n++;
            if (n == x.length) {
                x = java.util.Arrays.copyOf(x, n * 2);
                y = java.util.Arrays.copyOf(y, n * 2);
                u = java.util.Arrays.copyOf(u, n * 2);
                v = java.util.Arrays.copyOf(v, n * 2);
                color = java.util.Arrays.copyOf(color, n * 2);
            }
            x[n] = px;
            y[n] = py;
            color[n] = -1;
            return this;
        }

        @Override
        public VertexConsumer setColor(int r, int g, int b, int a) {
            return setColor((a & 255) << 24 | (r & 255) << 16 | (g & 255) << 8 | b & 255);
        }

        @Override
        public VertexConsumer setColor(int argb) {
            if (n >= 0) color[n] = argb;
            return this;
        }

        @Override
        public VertexConsumer setUv(float pu, float pv) {
            if (n >= 0) {
                u[n] = pu;
                v[n] = pv;
            }
            return this;
        }

        @Override
        public VertexConsumer setUv1(int pu, int pv) {
            return this;
        }

        @Override
        public VertexConsumer setUv2(int pu, int pv) {
            return this;
        }

        @Override
        public VertexConsumer setUv3(float pu, float pv) {
            return this;
        }

        @Override
        public VertexConsumer setNormal(float nx, float ny, float nz) {
            return this;
        }

        @Override
        public VertexConsumer setLineWidth(float width) {
            return this;
        }
    }

    @Override
    public void addGuiElement(GuiElementRenderState element) {
        try {
            var corners = new Corners();
            element.buildVertices(corners);
            var setup = element.textureSetup();
            String texture = setup != null && setup.texure0() instanceof Sprites.View view ? view.id().toString() : "";
            var pipeline = element.pipeline();
            int kind = pipeline == RenderPipelines.GUI_INVERT ? 1 : pipeline == RenderPipelines.GUI_TEXT_HIGHLIGHT ? 2 : 0;
            boolean atlas = texture.contains(":textures/atlas/");
            for (int q = 0; q + 3 <= corners.n; q += 4) {
                // The block and item atlases' sprites sit one unit of U apart: the quad's middle names its sprite.
                var sprite = atlas ? Sprites.indexed((corners.u[q] + corners.u[q + 1] + corners.u[q + 2] + corners.u[q + 3]) / 4) : null;
                float shift = sprite == null ? 0 : sprite.getU0();
                out.writeByte(QUAD);
                text(out, sprite == null ? texture : Sprites.textureFile(sprite));
                out.writeByte(kind);
                for (int i = q; i < q + 4; i++) {
                    out.writeFloat(corners.x[i]);
                    out.writeFloat(corners.y[i]);
                    out.writeFloat(corners.u[i] - shift);
                    out.writeFloat(corners.v[i]);
                    out.writeInt(corners.color[i]);
                }
                scissor(element.scissorArea());
                count++;
            }
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    @Override
    public void addBlitToCurrentLayer(BlitRenderState blit) {
        addGuiElement(blit);
    }

    @Override
    public void addGlyphToCurrentLayer(GuiElementRenderState glyph) {
        addGuiElement(glyph);
    }

    private void pose(Matrix3x2fc pose, float x, float y) throws IOException {
        out.writeFloat(pose.m00() * x + pose.m10() * y + pose.m20());
        out.writeFloat(pose.m01() * x + pose.m11() * y + pose.m21());
        out.writeFloat(pose.m00());
        out.writeFloat(pose.m11());
    }

    @Override
    public void addText(GuiTextRenderState state) {
        try {
            var sequence = (FormattedCharSequence) TEXT_TEXT.get(state);
            out.writeByte(TEXT);
            pose(state.pose, TEXT_X.getInt(state), TEXT_Y.getInt(state));
            out.writeInt(TEXT_COLOR.getInt(state));
            out.writeBoolean(TEXT_SHADOW.getBoolean(state));
            out.writeInt(TEXT_BACKGROUND.getInt(state));
            var runs = new java.util.ArrayList<Object[]>();
            var run = new StringBuilder();
            Style[] last = {null};
            sequence.accept((index, style, codepoint) -> {
                if (last[0] != null && !style.equals(last[0])) {
                    runs.add(new Object[] {run.toString(), last[0]});
                    run.setLength(0);
                }
                last[0] = style;
                run.appendCodePoint(codepoint);
                return true;
            });
            if (last[0] != null) runs.add(new Object[] {run.toString(), last[0]});
            out.writeInt(runs.size());
            for (Object[] r : runs) {
                Style style = (Style) r[1];
                text(out, (String) r[0]);
                out.writeInt(style.getColor() == null ? -1 : style.getColor().getValue() & 0xFFFFFF);
                out.writeByte((style.isBold() ? 1 : 0) | (style.isItalic() ? 2 : 0) | (style.isUnderlined() ? 4 : 0) | (style.isStrikethrough() ? 8 : 0) | (style.isObfuscated() ? 16 : 0));
                text(out, style.getFont() instanceof FontDescription.Resource resource ? resource.id().toString() : "");
            }
            scissor(state.scissor);
            count++;
        } catch (IOException | IllegalAccessException e) {
            throw new IllegalStateException(e);
        }
    }

    /** `GuiGraphicsExtractor.item`, before it asks for a model Java doesn't have. */
    public void item(Matrix3x2fc pose, ItemStack stack, int x, int y, ScreenRectangle scissor) {
        if (stack.isEmpty()) return;
        try {
            var tag = (CompoundTag) ItemStack.CODEC.encodeStart(GameBridge.registries().createSerializationContext(NbtOps.INSTANCE), stack).getOrThrow();
            out.writeByte(ITEM);
            pose(pose, x, y);
            GameBridge.writeTag(out, tag);
            scissor(scissor);
            count++;
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private static Field scissorStack;
    private static java.lang.reflect.Method peek;

    /** The extractor's scissor now, which its own item call would have given the item. */
    public static ScreenRectangle scissor(net.minecraft.client.gui.GuiGraphicsExtractor graphics) {
        try {
            if (peek == null) {
                scissorStack = net.minecraft.client.gui.GuiGraphicsExtractor.class.getDeclaredField("scissorStack");
                scissorStack.setAccessible(true);
                peek = scissorStack.getType().getDeclaredMethod("peek");
                peek.setAccessible(true);
            }
            return (ScreenRectangle) peek.invoke(scissorStack.get(graphics));
        } catch (ReflectiveOperationException e) {
            return null;
        }
    }

    @Override
    public void addItem(GuiItemRenderState item) {}

    @Override
    public void addPicturesInPictureState(PictureInPictureRenderState state) {}

    byte[] bytes() throws IOException {
        var whole = new ByteArrayOutputStream();
        var head = new DataOutputStream(whole);
        head.writeInt(count);
        bytes.writeTo(whole);
        return whole.toByteArray();
    }
}
