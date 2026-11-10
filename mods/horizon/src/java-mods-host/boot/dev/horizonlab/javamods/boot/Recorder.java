package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.vertex.VertexConsumer;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.charset.StandardCharsets;
import com.mojang.renderpearl.api.pipeline.BlendFunction;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
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
    private static final int QUAD = 0, TEXT = 1, ITEM = 2, BLUR = 3;
    private static final Field TEXT_TEXT = field("text"), TEXT_X = field("x"), TEXT_Y = field("y"), TEXT_COLOR = field("color"),
            TEXT_BACKGROUND = field("backgroundColor"), TEXT_SHADOW = field("dropShadow");

    private final ByteArrayOutputStream bytes = new ByteArrayOutputStream();
    private final DataOutputStream out = new DataOutputStream(bytes);
    private int count;
    private final boolean discard;

    public Recorder() {
        this(false);
    }

    /** A recorder that keeps nothing: for drawing whose layout side effects matter but whose pixels are native. */
    public Recorder(boolean discard) {
        this.discard = discard;
    }

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
        if (discard) return;
        try {
            var corners = new Corners();
            element.buildVertices(corners);
            var setup = element.textureSetup();
            String texture = setup != null && setup.texure0() instanceof Sprites.View view ? view.id().toString() : "";
            var pipeline = element.pipeline();
            int kind = blend(pipeline);
            boolean repeat = setup != null && setup.texure0() instanceof Sprites.View view && view.repeat();
            var atlas = texture.isEmpty() ? null : net.minecraft.resources.Identifier.tryParse(texture);
            var topology = pipeline.getPrimitiveTopology();
            var primitives = primitives(topology, corners.n + 1);
            if (primitives == null && WARNED.add(pipeline.getLocation().toString() + topology)) {
                System.err.println("Horizon: a GUI pipeline " + pipeline.getLocation() + " draws " + topology + ", which a Java screen can't show");
            }
            if (primitives != null) for (int[] p : primitives) primitive(corners, p, element, texture, atlas, kind, repeat);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private static final java.util.Set<String> WARNED = java.util.concurrent.ConcurrentHashMap.newKeySet();

    /** The `QUAD` entries of vertex indices a topology builds from `count` vertices; a triangle repeats its last. Null when Rust has no such primitive. */
    static int[][] primitives(PrimitiveTopology topology, int count) {
        switch (topology) {
            case QUADS, LINES -> {
                var quads = new int[Math.max(count / 4, 0)][];
                for (int i = 0; i < quads.length; i++) quads[i] = new int[] {i * 4, i * 4 + 1, i * 4 + 2, i * 4 + 3};
                return quads;
            }
            case TRIANGLES -> {
                var triangles = new int[Math.max(count / 3, 0)][];
                for (int i = 0; i < triangles.length; i++) triangles[i] = new int[] {i * 3, i * 3 + 1, i * 3 + 2, i * 3 + 2};
                return triangles;
            }
            case TRIANGLE_STRIP -> {
                var triangles = new int[Math.max(count - 2, 0)][];
                for (int i = 0; i < triangles.length; i++) triangles[i] = new int[] {i, i + 1, i + 2, i + 2};
                return triangles;
            }
            case TRIANGLE_FAN -> {
                var triangles = new int[Math.max(count - 2, 0)][];
                for (int i = 0; i < triangles.length; i++) triangles[i] = new int[] {0, i + 1, i + 2, i + 2};
                return triangles;
            }
            default -> {
                return null;
            }
        }
    }

    /** 1 inverts, 2 adds, 0 blends as translucent; any other blend function is drawn as translucent. */
    private static int blend(RenderPipeline pipeline) {
        var target = pipeline.getColorTargetStates().get(0);
        var function = target == null ? null : target.blendFunction().orElse(null);
        if (function == null || function.equals(BlendFunction.TRANSLUCENT)) return 0;
        if (function.equals(BlendFunction.INVERT)) return 1;
        if (function.equals(BlendFunction.ADDITIVE)) return 2;
        if (WARNED.add(pipeline.getLocation().toString())) {
            System.err.println("Horizon: a GUI pipeline " + pipeline.getLocation() + " blends with " + function + ", drawn as translucent");
        }
        return 0;
    }

    private void primitive(Corners corners, int[] at, GuiElementRenderState element, String texture, net.minecraft.resources.Identifier atlas, int kind, boolean repeat) throws IOException {
        // A stitched atlas's quad is its sprite's: the primitive's middle names it, and Rust draws that texture's file.
        boolean triangle = at[2] == at[3];
        float middleU = 0, middleV = 0;
        int points = triangle ? 3 : 4;
        for (int i = 0; i < points; i++) {
            middleU += corners.u[at[i]];
            middleV += corners.v[at[i]];
        }
        var spot = atlas == null ? null : Sprites.locate(atlas, middleU / points, middleV / points);
        var sprite = spot == null ? null : spot.sprite();
        out.writeByte(QUAD);
        text(out, spot == null ? texture : spot.file());
        out.writeByte(kind);
        out.writeByte(!triangle && spot == null && repeat && !texture.isEmpty() ? 1 : 0);
        for (int i : at) {
            out.writeFloat(corners.x[i]);
            out.writeFloat(corners.y[i]);
            out.writeFloat(sprite == null ? corners.u[i] : (corners.u[i] - sprite.getU0()) / (sprite.getU1() - sprite.getU0()));
            out.writeFloat(sprite == null ? corners.v[i] : (corners.v[i] - sprite.getV0()) / (sprite.getV1() - sprite.getV0()));
            out.writeInt(corners.color[i]);
        }
        scissor(element.scissorArea());
        count++;
    }

    @Override
    public void addBlitToCurrentLayer(BlitRenderState blit) {
        if (discard) return;
        addGuiElement(blit);
    }

    @Override
    public void addGlyphToCurrentLayer(GuiElementRenderState glyph) {
        if (discard) return;
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
        if (discard) return;
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
        if (discard || stack.isEmpty()) return;
        try {
            var tag = (CompoundTag) ItemStack.CODEC.encodeStart(Clients.registries.createSerializationContext(NbtOps.INSTANCE), stack).getOrThrow();
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
    public void blurBeforeThisStratum() {
        super.blurBeforeThisStratum();
        if (discard) return;
        try {
            out.writeByte(BLUR);
            count++;
        } catch (IOException e) {
            throw new IllegalStateException(e);
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

    /** The entries since the last cut as a frame, then a fresh start: one recording in segments. */
    byte[] cut() throws IOException {
        byte[] frame = bytes();
        bytes.reset();
        count = 0;
        return frame;
    }

    /** Two frames as one: the entries of `a`, then of `b`. */
    static byte[] concat(byte[] a, byte[] b) throws IOException {
        var whole = new ByteArrayOutputStream();
        var head = new DataOutputStream(whole);
        head.writeInt(new java.io.DataInputStream(new java.io.ByteArrayInputStream(a)).readInt() + new java.io.DataInputStream(new java.io.ByteArrayInputStream(b)).readInt());
        whole.write(a, 4, a.length - 4);
        whole.write(b, 4, b.length - 4);
        return whole.toByteArray();
    }
}
