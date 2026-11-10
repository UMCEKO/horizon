package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import dev.horizonlab.javamods.boot.BlockRenders.Draw;
import dev.horizonlab.javamods.boot.BlockRenders.Line;
import dev.horizonlab.javamods.boot.BlockRenders.Quad;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import net.minecraft.client.model.Model;
import net.minecraft.client.renderer.OrderedSubmitNodeCollector;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.rendertype.RenderType;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import net.minecraft.client.renderer.texture.UvMapping;
import net.minecraft.client.resources.model.geometry.BakedQuad;
import net.minecraft.core.Direction;
import net.minecraft.resources.Identifier;
import net.minecraft.world.phys.Vec3;
import org.joml.Matrix3f;
import org.joml.Matrix4f;
import org.joml.Quaternionf;
import org.joml.Vector3f;

/** The collector a renderer submits to: each submission becomes pieces of geometry, in the frame of a block entity's pose or, in world mode, of the level's frame. */
final class SubmitCapture implements SubmitNodeCollector {
    /** Pieces in the level's frame (the pose applied), where a block entity's keep the pose apart for Rust to move. */
    final boolean world;
    final Geometry geometry = new Geometry();
    /** What a recorded item's special model draws with: its renderer is only told whether the stack has foil. */
    int foil;
    RenderType modelType;

    SubmitCapture(boolean world) {
        this.world = world;
    }

    /** One vertex while its attributes arrive. */
    private static final class Vert {
        float x, y, z, u, v, nx, ny, nz, width = 1;
        int color = -1, light;
        boolean hasNormal;
    }

    /** Collects vertices into the current piece, in its pose's frame. */
    final class Geometry implements VertexConsumer {
        final List<Draw> draws = new ArrayList<>();
        Draw draw;
        Identifier texture;
        PrimitiveTopology topology = PrimitiveTopology.QUADS;
        private final ArrayList<Vert> run = new ArrayList<>();
        private Vert current;
        private boolean blended;

        void begin(Draw d) {
            begin(d, d.type == null ? PrimitiveTopology.QUADS : d.type.primitiveTopology(), d.type == null ? null : setup(d.type), d.type != null && d.type.hasBlending());
        }

        /** A piece of a given topology, drawn with a texture or none, whether it blends. */
        void begin(Draw d, PrimitiveTopology topology, Identifier texture, boolean blended) {
            finish();
            draw = d;
            draws.add(d);
            this.texture = texture;
            this.topology = topology;
            this.blended = blended;
        }

        /** Ends the piece's vertices: one still waiting for its attributes is complete. */
        void finish() {
            commit();
            run.clear();
        }

        void flush() {
            commit();
        }

        private void commit() {
            Vert v = current;
            current = null;
            if (v == null || draw == null) return;
            run.add(v);
            int n = run.size();
            switch (topology) {
                case QUADS -> {
                    if (n % 4 == 0) {
                        quad(run.get(n - 4), run.get(n - 3), run.get(n - 2), run.get(n - 1));
                        run.clear();
                    }
                }
                case TRIANGLES -> {
                    if (n % 3 == 0) {
                        quad(run.get(n - 3), run.get(n - 2), run.get(n - 1), run.get(n - 1));
                        run.clear();
                    }
                }
                case TRIANGLE_STRIP -> {
                    if (n >= 3) {
                        // Every second triangle of a strip is wound the other way.
                        boolean odd = (n & 1) == 0;
                        quad(run.get(odd ? n - 2 : n - 3), run.get(odd ? n - 3 : n - 2), run.get(n - 1), run.get(n - 1));
                    }
                }
                case TRIANGLE_FAN -> {
                    if (n >= 3) quad(run.get(0), run.get(n - 2), run.get(n - 1), run.get(n - 1));
                }
                case LINES, DEBUG_LINES -> {
                    if (n % 2 == 0) {
                        line(run.get(n - 2), run.get(n - 1));
                        run.clear();
                    }
                }
                case DEBUG_LINE_STRIP -> {
                    if (n >= 2) line(run.get(n - 2), run.get(n - 1));
                }
                case POINTS -> run.clear();
            }
        }

        private void line(Vert a, Vert b) {
            draw.lines.add(new Line(new float[] {a.x, a.y, a.z, b.x, b.y, b.z}, a.color, a.width));
        }

        private void quad(Vert a, Vert b, Vert c, Vert d) {
            var vs = new Vert[] {a, b, c, d};
            float[] xyzuv = new float[20];
            int[] color = new int[4];
            int[] light = new int[4];
            for (int k = 0; k < 4; k++) {
                xyzuv[k * 5] = vs[k].x;
                xyzuv[k * 5 + 1] = vs[k].y;
                xyzuv[k * 5 + 2] = vs[k].z;
                xyzuv[k * 5 + 3] = vs[k].u;
                xyzuv[k * 5 + 4] = vs[k].v;
                color[k] = vs[k].color;
                light[k] = vs[k].light;
            }
            var made = textured(texture, xyzuv, color, light, a.hasNormal ? new float[] {a.nx, a.ny, a.nz} : null, blended);
            if (made != null) draw.quads.add(made);
        }

        @Override
        public VertexConsumer addVertex(float x, float y, float z) {
            commit();
            if (draw == null) return this;
            var p = draw.local.transformPosition(x, y, z, new Vector3f());
            var v = new Vert();
            v.x = p.x;
            v.y = p.y;
            v.z = p.z;
            current = v;
            return this;
        }

        @Override
        public VertexConsumer setColor(int r, int g, int b, int a) {
            if (current != null) current.color = a << 24 | r << 16 | g << 8 | b;
            return this;
        }

        @Override
        public VertexConsumer setColor(int argb) {
            if (current != null) current.color = argb;
            return this;
        }

        @Override
        public VertexConsumer setUv(float u, float v) {
            if (current != null) {
                current.u = u;
                current.v = v;
            }
            return this;
        }

        @Override
        public VertexConsumer setUv1(int u, int v) {
            return this;
        }

        @Override
        public VertexConsumer setUv2(int u, int v) {
            if (current != null) current.light = u | v << 16;
            return this;
        }

        @Override
        public VertexConsumer setUv3(float u, float v) {
            return this;
        }

        @Override
        public VertexConsumer setNormal(float x, float y, float z) {
            if (current != null && draw != null) {
                var local = draw.normals.transform(new Vector3f(x, y, z)).normalize();
                current.nx = local.x;
                current.ny = local.y;
                current.nz = local.z;
                current.hasNormal = true;
            }
            return this;
        }

        @Override
        public VertexConsumer setLineWidth(float width) {
            if (current != null) current.width = width;
            return this;
        }
    }

    /** A quad drawn with `texture`: a stitched atlas's sprite is found by the quad's UV middle and named by its file with sprite-relative UVs; null when the atlas has no sprite there. */
    static Quad textured(Identifier texture, float[] xyzuv, int[] color, int[] light, float[] normal, boolean blended) {
        String name;
        var spot = texture == null ? null : Sprites.locate(texture, (xyzuv[3] + xyzuv[8] + xyzuv[13] + xyzuv[18]) / 4, (xyzuv[4] + xyzuv[9] + xyzuv[14] + xyzuv[19]) / 4);
        if (spot != null) {
            var sprite = spot.sprite();
            name = file(Identifier.parse(spot.file()));
            for (int k = 0; k < 4; k++) {
                xyzuv[k * 5 + 3] = (xyzuv[k * 5 + 3] - sprite.getU0()) / (sprite.getU1() - sprite.getU0());
                xyzuv[k * 5 + 4] = (xyzuv[k * 5 + 4] - sprite.getV0()) / (sprite.getV1() - sprite.getV0());
            }
        } else if (texture != null && texture.getPath().startsWith("textures/atlas/")) {
            return null;
        } else if (texture != null) {
            name = file(texture);
        } else {
            // Untextured geometry (lights, beams) takes its colour alone, from the atlas's white texel.
            name = "horizon/white";
            for (int k = 0; k < 4; k++) xyzuv[k * 5 + 3] = xyzuv[k * 5 + 4] = 0.5f;
        }
        return new Quad(name, xyzuv, color, light, normal, blended);
    }

    static String file(Identifier texture) {
        return texture.withPath(p -> p.replaceFirst("^textures/", "").replaceFirst("\\.png$", "")).toString();
    }

    static Identifier setup(RenderType type) {
        try {
            var state = RenderType.class.getDeclaredField("state");
            state.setAccessible(true);
            var setup = state.get(type);
            var textures = setup.getClass().getDeclaredField("textures");
            textures.setAccessible(true);
            var binding = ((Map<?, ?>) textures.get(setup)).get("Sampler0");
            if (binding == null) return null;
            var location = binding.getClass().getDeclaredMethod("location");
            location.setAccessible(true);
            return (Identifier) location.invoke(binding);
        } catch (ReflectiveOperationException e) {
            return null;
        }
    }

    Draw start(PoseStack.Pose pose, RenderType type) {
        geometry.flush();
        var d = new Draw();
        var ops = world ? null : BlockRenders.ops(pose);
        d.ops = ops == null ? null : List.copyOf(ops);
        d.matrix = new Matrix4f(pose.pose());
        // In world mode the vertices arrive in the level's frame already.
        d.local = world ? new Matrix4f() : new Matrix4f(d.matrix).invert();
        d.normals = world ? new Matrix3f() : new Matrix3f(d.matrix).transpose();
        d.foil = foil;
        d.type = type;
        geometry.begin(d);
        return d;
    }

    /** `ModelPart.compile`: the part's vertices arrive in the stack's space, under this pose. */
    void part(PoseStack.Pose pose, RenderType type) {
        start(pose, type);
    }

    @Override
    public <S> void submitModel(Model<? super S> model, S state, PoseStack poseStack, RenderType renderType, int light, int overlay, int tint, UvMapping uvMapping, int outline) {
        geometry.flush();
        modelType = renderType;
        if (world) start(poseStack.last(), renderType);
        else BlockRenders.CURRENT = this;
        VertexConsumer buffer = uvMapping != null ? uvMapping.wrap(geometry) : geometry;
        model.setupAnim(state);
        model.renderToBuffer(poseStack, buffer, light, overlay, tint);
        geometry.flush();
        BlockRenders.CURRENT = null;
    }

    @Override
    public void submitCustomGeometry(PoseStack poseStack, RenderType renderType, CustomGeometryRenderer renderer) {
        var d = start(poseStack.last(), renderType);
        d.local = new Matrix4f();
        d.normals = new Matrix3f();
        renderer.render(world ? poseStack.last() : new PoseStack().last(), geometry);
        geometry.flush();
    }

    @Override
    public void submitBlockModel(PoseStack poseStack, RenderType renderType, List<net.minecraft.client.renderer.block.dispatch.BlockStateModelPart> parts, int[] tints, int light, int overlay, int outline) {
        var d = start(poseStack.last(), null);
        geometry.finish();
        for (var part : parts) {
            for (var direction : Direction.values()) for (var quad : part.getQuads(direction)) quad(d, quad, tints, light);
            for (var quad : part.getQuads(null)) quad(d, quad, tints, light);
        }
    }

    @Override
    public void submitItem(PoseStack poseStack, net.minecraft.world.item.ItemDisplayContext context, int light, int overlay, int outline, int[] tints,
            net.minecraft.client.resources.model.geometry.ItemQuads quads, net.minecraft.client.renderer.item.ItemStackRenderState.FoilType foil) {
        var d = start(poseStack.last(), null);
        d.foil = foil.ordinal();
        geometry.finish();
        for (var quad : quads.all()) quad(d, quad, tints, light);
    }

    private void quad(Draw d, BakedQuad quad, int[] tints, int light) {
        var sprite = quad.materialInfo().sprite();
        float[] xyzuv = new float[20];
        int[] color = new int[4];
        int[] lights = new int[4];
        int tintIndex = quad.materialInfo().tintIndex();
        int tint = tintIndex >= 0 && tints != null && tintIndex < tints.length ? tints[tintIndex] : -1;
        int emission = quad.materialInfo().lightEmission();
        for (int i = 0; i < 4; i++) {
            var p = quad.position(i);
            long uv = quad.packedUV(i);
            float x = p.x(), y = p.y(), z = p.z();
            if (world) {
                var moved = d.matrix.transformPosition(x, y, z, new Vector3f());
                x = moved.x;
                y = moved.y;
                z = moved.z;
            }
            xyzuv[i * 5] = x;
            xyzuv[i * 5 + 1] = y;
            xyzuv[i * 5 + 2] = z;
            xyzuv[i * 5 + 3] = (net.minecraft.client.model.geom.builders.UVPair.unpackU(uv) - sprite.getU0()) / (sprite.getU1() - sprite.getU0());
            xyzuv[i * 5 + 4] = (net.minecraft.client.model.geom.builders.UVPair.unpackV(uv) - sprite.getV0()) / (sprite.getV1() - sprite.getV0());
            color[i] = net.minecraft.util.ARGB.multiply(tint, quad.bakedColors().color(i));
            lights[i] = emission > 0 ? emission << 4 | emission << 20 : light;
        }
        var facing = quad.direction().getUnitVec3f();
        // Named as `quad` names a located sprite: by its texture file, which a mod's own atlas needn't share with the sprite's id.
        d.quads.add(new Quad(Sprites.file(sprite.atlasLocation(), sprite.contents().name()).toString(), xyzuv, color, lights, new float[] {facing.x(), facing.y(), facing.z()},
                quad.materialInfo().layer() == net.minecraft.client.renderer.chunk.ChunkSectionLayer.TRANSLUCENT));
    }

    @Override
    public OrderedSubmitNodeCollector order(int order) {
        return this;
    }

    @Override
    public void submitShapeOutline(PoseStack poseStack, net.minecraft.world.phys.shapes.VoxelShape shape, RenderType type, int color, float width, boolean afterTerrain) {
        if (!world) return;
        var d = start(poseStack.last(), type);
        d.afterTerrain = afterTerrain;
        geometry.finish();
        var pose = d.matrix;
        shape.forAllEdges((x1, y1, z1, x2, y2, z2) -> {
            var a = pose.transformPosition((float) x1, (float) y1, (float) z1, new Vector3f());
            var b = pose.transformPosition((float) x2, (float) y2, (float) z2, new Vector3f());
            d.lines.add(new Line(new float[] {a.x, a.y, a.z, b.x, b.y, b.z}, color, width));
        });
    }

    @Override
    public void submitShadow(PoseStack poseStack, float radius, List<net.minecraft.client.renderer.entity.state.EntityRenderState.ShadowPiece> pieces) {}

    @Override
    public void submitNameTag(PoseStack poseStack, Vec3 offset, int y, net.minecraft.network.chat.Component name, boolean seeThrough, int light, CameraRenderState camera) {}

    @Override
    public void submitText(PoseStack poseStack, float x, float y, net.minecraft.util.FormattedCharSequence text, boolean shadow, net.minecraft.client.gui.Font.DisplayMode mode, int light, int color, int background, int outline) {}

    @Override
    public void submitTextBackground(PoseStack poseStack, float x0, float y0, float x1, float y1, int color, net.minecraft.client.gui.Font.DisplayMode mode, int light) {}

    @Override
    public void submitFlame(PoseStack poseStack, net.minecraft.client.renderer.entity.state.EntityRenderState state, Quaternionf rotation) {}

    @Override
    public void submitLeash(PoseStack poseStack, net.minecraft.client.renderer.entity.state.EntityRenderState.LeashState state) {}

    @Override
    public <S> void submitCrumblingOverlay(Model<? super S> model, S state, PoseStack poseStack, RenderType type, int light, int overlay, int tint, net.minecraft.client.renderer.feature.ModelFeatureRenderer.CrumblingOverlay crumbling) {}

    @Override
    public void submitMovingBlock(PoseStack poseStack, net.minecraft.client.renderer.block.MovingBlockRenderState state, int outline) {}

    @Override
    public void submitBreakingBlockModel(PoseStack poseStack, List<net.minecraft.client.renderer.block.dispatch.BlockStateModelPart> parts, int progress, boolean translucent) {}

    @Override
    public void submitQuadParticleGroup(net.minecraft.client.renderer.state.level.QuadParticleRenderState particles) {}

    @Override
    public void submitGizmoPrimitives(net.minecraft.client.renderer.gizmos.DrawableGizmoPrimitives.Group group, CameraRenderState camera, boolean onTop) {}
}
