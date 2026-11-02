package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.model.Model;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.OrderedSubmitNodeCollector;
import net.minecraft.client.renderer.blockentity.BlockEntityRenderDispatcher;
import net.minecraft.client.renderer.blockentity.BlockEntityRenderer;
import net.minecraft.client.renderer.blockentity.BlockEntityRendererProvider;
import net.minecraft.client.renderer.blockentity.state.BlockEntityRenderState;
import net.minecraft.client.renderer.rendertype.RenderType;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import net.minecraft.client.renderer.texture.UvMapping;
import net.minecraft.client.resources.model.geometry.BakedQuad;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.EntityBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.Vec3;
import org.joml.Matrix4f;
import org.joml.Quaternionf;
import org.joml.Vector3f;

/** Mods' block entity renderers, recorded in this JVM when their block entity changes: geometry, and how each piece moves with time, for Rust to draw every frame. */
public final class BlockRenders {
    private BlockRenders() {}

    /** Ticks sampled to tell how each piece moves. */
    private static final int SAMPLES = 8;

    // --- PoseStack operations, recorded per pose while a renderer submits

    record Op(char type, float[] v) {}

    static volatile boolean tracking;
    private static final IdentityHashMap<Object, ArrayList<Op>> OPS = new IdentityHashMap<>();

    public static void op(Object pose, char type, float... v) {
        if (!tracking) return;
        var list = OPS.get(pose);
        if (list != null) list.add(new Op(type, v));
    }

    public static void rotate(Object pose, Quaternionf q) {
        if (tracking) op(pose, 'Q', q.x, q.y, q.z, q.w);
    }

    public static void reset(Object pose) {
        if (!tracking) return;
        var list = OPS.get(pose);
        if (list != null) list.clear();
    }

    /** `Pose.set`: the copy carries the operations of the pose it copies. */
    public static void copied(Object into, Object from) {
        if (!tracking) return;
        var list = OPS.get(from);
        if (list != null) OPS.put(into, new ArrayList<>(list));
        else OPS.remove(into);
    }

    // --- Renderers

    private static Map<BlockEntityType<?>, BlockEntityRenderer<?, ?>> renderers;
    private static final Map<Block, Boolean> HAS = new HashMap<>();

    /** Mods' renderers, made as the client's resource reload makes them; one that fails to build is left out. */
    @SuppressWarnings("unchecked")
    static synchronized Map<BlockEntityType<?>, BlockEntityRenderer<?, ?>> renderers() {
        if (renderers != null) return renderers;
        renderers = new HashMap<>();
        try {
            // Renderers fetch the standalone models the bake makes, some once when they're built.
            try {
                ClientModels.baked();
            } catch (Exception e) {
                System.err.println("Horizon: the code-built models didn't bake: " + e);
            }
            var minecraft = Minecraft.getInstance();
            var models = net.minecraft.client.model.geom.EntityModelSet.vanilla();
            var dispatcher = new BlockEntityRenderDispatcher(minecraft.font, () -> models,
                    new net.minecraft.client.renderer.block.BlockModelResolver(minecraft.getModelManager()),
                    new net.minecraft.client.renderer.item.ItemModelResolver(minecraft.getModelManager()), null, minecraft.getAtlasManager(), null);
            var field = Minecraft.class.getDeclaredField("blockEntityRenderDispatcher");
            field.setAccessible(true);
            field.set(minecraft, dispatcher);
            var context = new BlockEntityRendererProvider.Context(dispatcher, new net.minecraft.client.renderer.block.BlockModelResolver(minecraft.getModelManager()),
                    new net.minecraft.client.renderer.item.ItemModelResolver(minecraft.getModelManager()), null, models, minecraft.font, minecraft.getAtlasManager(), null);
            var providers = net.minecraft.client.renderer.blockentity.BlockEntityRenderers.class.getDeclaredField("PROVIDERS");
            providers.setAccessible(true);
            for (var entry : ((Map<BlockEntityType<?>, BlockEntityRendererProvider<?, ?>>) providers.get(null)).entrySet()) {
                var id = BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(entry.getKey());
                if (id == null || id.getNamespace().equals("minecraft")) continue;
                try {
                    renderers.put(entry.getKey(), entry.getValue().create(context));
                } catch (Throwable t) {
                    System.err.println("Horizon: the block entity renderer of " + id + " didn't build: " + t);
                }
            }
            var all = new HashMap<>(renderers);
            var map = BlockEntityRenderDispatcher.class.getDeclaredField("renderers");
            map.setAccessible(true);
            map.set(dispatcher, all);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
        System.err.println("Horizon: " + renderers.size() + " mods' block entity renderers");
        return renderers;
    }

    /** Whether a mod's renderer draws this block's block entity. */
    static synchronized boolean has(BlockState state) {
        if (!state.hasBlockEntity() || !(state.getBlock() instanceof EntityBlock)) return false;
        return HAS.computeIfAbsent(state.getBlock(), block -> {
            for (var type : renderers().keySet()) if (type.isValid(block.defaultBlockState())) return true;
            return false;
        });
    }

    // --- Which positions to record again

    private static final LongLinkedOpenHashSet DIRTY = new LongLinkedOpenHashSet();

    private static boolean sampling;

    public static boolean sampling() {
        return sampling;
    }

    static void dirty(BlockPos pos) {
        // What a client ticker run while sampling changes is part of the motion, not a change to record again.
        if (!sampling) DIRTY.add(pos.asLong());
    }

    private static final it.unimi.dsi.fastutil.longs.Long2LongOpenHashMap RECORDED = new it.unimi.dsi.fastutil.longs.Long2LongOpenHashMap();

    /** The server side changed a block entity a renderer draws: recorded again, at most once a second. */
    static void changed(net.minecraft.server.level.ServerLevel level, BlockEntity entity) {
        if (!has(entity.getBlockState())) return;
        long at = entity.getBlockPos().asLong();
        long now = level.getGameTime();
        if (now - RECORDED.getOrDefault(at, Long.MIN_VALUE / 2) < 20) return;
        RECORDED.put(at, now);
        dirty(entity.getBlockPos());
    }

    private static final it.unimi.dsi.fastutil.longs.Long2IntOpenHashMap SEEN = new it.unimi.dsi.fastutil.longs.Long2IntOpenHashMap();

    /** What a recording drew from on the server: its saved form and block state, which some mods change without saying so. */
    private static int seen(ClientLevel level, BlockPos pos) {
        var server = GameBridge.levelOrNull(level.dimension().identifier().toString());
        var source = server == null ? null : BlockEntities.held(server).get(pos.asLong());
        if (source == null || source.isRemoved()) return 0;
        return source.saveWithFullMetadata(server.registryAccess()).hashCode() * 31 + source.getBlockState().hashCode();
    }

    /** Once a second: the recorded block entities whose server side changed are recorded again. */
    private static long polled;

    static void poll(ClientLevel level) {
        long now = System.nanoTime();
        if (level == null || SEEN.isEmpty() || now - polled < 1_000_000_000L) return;
        polled = now;
        for (var entry : SEEN.long2IntEntrySet()) {
            var pos = BlockPos.of(entry.getLongKey());
            if (seen(level, pos) != entry.getIntValue()) dirty(pos);
        }
    }

    static int pending() {
        return DIRTY.size();
    }

    static void forget() {
        DIRTY.clear();
        RECORDED.clear();
        SEEN.clear();
    }

    // --- Recording

    /** One piece a renderer submitted: its quads in the frame of its pose, which the operations build. */
    static final class Draw {
        List<Op> ops;
        Matrix4f matrix;
        Matrix4f local;
        RenderType type;
        final List<Quad> quads = new ArrayList<>();
        /** An item drawn here by Rust: its stack, `ItemDisplayContext` ordinal and seed. */
        Object[] item;
    }

    record Quad(String texture, float[] xyzuv, int[] color, int[] light) {}

    /** Collects vertices into the current piece, in its pose's frame. */
    private static final class Geometry implements VertexConsumer {
        final List<Draw> draws = new ArrayList<>();
        Draw draw;
        Identifier texture;
        boolean quads;
        final float[] xyzuv = new float[20];
        final int[] color = new int[4];
        final int[] light = new int[4];
        int n = -1;

        void begin(Draw d) {
            finish();
            draw = d;
            draws.add(d);
            var setup = d.type == null ? null : setup(d.type);
            texture = setup;
            quads = d.type == null || d.type.primitiveTopology() == com.mojang.renderpearl.api.pipeline.PrimitiveTopology.QUADS;
            n = -1;
        }

        void finish() {
            n = -1;
        }

        private void vertexDone() {
            if (n == 3) {
                emit();
                n = -1;
            } else if (n == 2 && !quads) {
                // A triangle as a quad with its last corner twice.
                for (int k = 0; k < 5; k++) xyzuv[15 + k] = xyzuv[10 + k];
                color[3] = color[2];
                light[3] = light[2];
                emit();
                n = -1;
            }
        }

        private void emit() {
            String name;
            float shift = 0;
            if (texture != null && texture.getPath().startsWith("textures/atlas/")) {
                var sprite = Sprites.indexed((xyzuv[3] + xyzuv[8] + xyzuv[13] + xyzuv[18]) / 4);
                if (sprite == null) return;
                name = sprite.contents().name().toString();
                shift = sprite.getU0();
            } else if (texture != null) {
                name = texture.withPath(p -> p.replaceFirst("^textures/", "").replaceFirst("\\.png$", "")).toString();
            } else {
                // Untextured geometry (lights, beams) takes its colour alone, from the atlas's white texel.
                name = "horizon/white";
                for (int k = 0; k < 4; k++) xyzuv[k * 5 + 3] = xyzuv[k * 5 + 4] = 0.5f;
            }
            var copy = xyzuv.clone();
            for (int k = 0; k < 4; k++) copy[k * 5 + 3] -= shift;
            draw.quads.add(new Quad(name, copy, color.clone(), light.clone()));
        }

        @Override
        public VertexConsumer addVertex(float x, float y, float z) {
            if (draw == null) return this;
            if (n >= 0) vertexDone();
            n = n + 1;
            var p = draw.local.transformPosition(x, y, z, new Vector3f());
            xyzuv[n * 5] = p.x;
            xyzuv[n * 5 + 1] = p.y;
            xyzuv[n * 5 + 2] = p.z;
            color[n] = -1;
            light[n] = 0;
            pendingEnd = true;
            return this;
        }

        boolean pendingEnd;

        /** The last vertex of a quad is only known complete once its attributes are set; this ends it. */
        void flush() {
            if (n >= 0 && pendingEnd) vertexDone();
            pendingEnd = false;
        }

        @Override
        public VertexConsumer setColor(int r, int g, int b, int a) {
            if (n >= 0) color[n] = a << 24 | r << 16 | g << 8 | b;
            return this;
        }

        @Override
        public VertexConsumer setColor(int argb) {
            if (n >= 0) color[n] = argb;
            return this;
        }

        @Override
        public VertexConsumer setUv(float u, float v) {
            if (n >= 0) {
                xyzuv[n * 5 + 3] = u;
                xyzuv[n * 5 + 4] = v;
            }
            return this;
        }

        @Override
        public VertexConsumer setUv1(int u, int v) {
            return this;
        }

        @Override
        public VertexConsumer setUv2(int u, int v) {
            if (n >= 0) light[n] = u | v << 16;
            return this;
        }

        @Override
        public VertexConsumer setUv3(float u, float v) {
            return this;
        }

        @Override
        public VertexConsumer setNormal(float x, float y, float z) {
            return this;
        }

        @Override
        public VertexConsumer setLineWidth(float width) {
            return this;
        }
    }

    private static Identifier setup(RenderType type) {
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

    /** The collector a renderer submits to: each submission becomes pieces of geometry under the pose's operations. */
    private static final class Capture implements SubmitNodeCollector {
        final Geometry geometry = new Geometry();

        Draw start(PoseStack.Pose pose, RenderType type) {
            geometry.flush();
            var d = new Draw();
            var ops = OPS.get(pose);
            d.ops = ops == null ? null : List.copyOf(ops);
            d.matrix = new Matrix4f(pose.pose());
            d.local = new Matrix4f(d.matrix).invert();
            d.type = type;
            geometry.begin(d);
            return d;
        }

        /** `ModelPart.compile`: the part's vertices arrive in the stack's space, under this pose. */
        void part(PoseStack.Pose pose, RenderType type) {
            start(pose, type);
        }

        RenderType modelType;

        @Override
        public <S> void submitModel(Model<? super S> model, S state, PoseStack poseStack, RenderType renderType, int light, int overlay, int tint, UvMapping uvMapping, int outline) {
            geometry.flush();
            modelType = renderType;
            CURRENT = this;
            VertexConsumer buffer = uvMapping != null ? uvMapping.wrap(geometry) : geometry;
            model.setupAnim(state);
            model.renderToBuffer(poseStack, buffer, light, overlay, tint);
            geometry.flush();
            CURRENT = null;
        }

        @Override
        public void submitCustomGeometry(PoseStack poseStack, RenderType renderType, CustomGeometryRenderer renderer) {
            var d = start(poseStack.last(), renderType);
            d.local = new Matrix4f();
            renderer.render(new PoseStack().last(), geometry);
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
                xyzuv[i * 5] = p.x();
                xyzuv[i * 5 + 1] = p.y();
                xyzuv[i * 5 + 2] = p.z();
                xyzuv[i * 5 + 3] = (net.minecraft.client.model.geom.builders.UVPair.unpackU(uv) - sprite.getU0()) / (sprite.getU1() - sprite.getU0());
                xyzuv[i * 5 + 4] = (net.minecraft.client.model.geom.builders.UVPair.unpackV(uv) - sprite.getV0()) / (sprite.getV1() - sprite.getV0());
                color[i] = tint;
                lights[i] = emission > 0 ? emission << 4 | emission << 20 : light;
            }
            d.quads.add(new Quad(sprite.contents().name().toString(), xyzuv, color, lights));
        }

        @Override
        public OrderedSubmitNodeCollector order(int order) {
            return this;
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
        public void submitShapeOutline(PoseStack poseStack, net.minecraft.world.phys.shapes.VoxelShape shape, RenderType type, int color, float width, boolean afterTerrain) {}

        @Override
        public void submitQuadParticleGroup(net.minecraft.client.renderer.state.level.QuadParticleRenderState particles) {}

        @Override
        public void submitGizmoPrimitives(net.minecraft.client.renderer.gizmos.DrawableGizmoPrimitives.Group group, CameraRenderState camera, boolean onTop) {}
    }

    private static Capture CURRENT;
    private static Capture ACTIVE;
    private static final IdentityHashMap<Object, Object[]> ITEMS = new IdentityHashMap<>();

    /** `ItemModelResolver.updateForTopItem` while a renderer is recorded: the stack is kept for Rust, which has the item models. */
    public static boolean item(Object state, net.minecraft.world.item.ItemStack stack, net.minecraft.world.item.ItemDisplayContext context, int seed) {
        if (!sampling) return false;
        if (stack.isEmpty()) ITEMS.remove(state);
        else ITEMS.put(state, new Object[] {stack.copy(), context.ordinal(), seed});
        return true;
    }

    public static boolean hasItem(Object state) {
        return sampling && ITEMS.containsKey(state);
    }

    /** `ItemStackRenderState.submit` of a kept stack: a piece that is that item under the pose. */
    public static boolean submitItem(Object state, PoseStack poseStack) {
        var item = ITEMS.get(state);
        if (!tracking || item == null || ACTIVE == null) return false;
        ACTIVE.start(poseStack.last(), null).item = item;
        return true;
    }

    /** `ModelPart.compile` of a model a renderer submitted: a piece of its own under the part's pose. */
    public static void modelPart(PoseStack.Pose pose) {
        var capture = CURRENT;
        if (capture != null && tracking) capture.part(pose, capture.modelType);
    }

    // --- Sampling and motion

    @SuppressWarnings({"unchecked", "rawtypes"})
    private static List<Draw> sample(BlockEntityRenderer renderer, BlockEntity entity, CameraRenderState camera) {
        var state = (BlockEntityRenderState) renderer.createRenderState();
        renderer.extractRenderState(entity, state, 0.0F, camera.pos, null);
        var stack = new PoseStack();
        OPS.clear();
        OPS.put(stack.last(), new ArrayList<>());
        var capture = new Capture();
        ACTIVE = capture;
        tracking = true;
        try {
            renderer.submit(state, stack, capture, camera);
            capture.geometry.flush();
        } finally {
            tracking = false;
            CURRENT = null;
            ACTIVE = null;
            OPS.clear();
        }
        capture.geometry.draws.removeIf(d -> d.quads.isEmpty() && d.item == null);
        return capture.geometry.draws;
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    private static void tick(ClientLevel level, BlockEntity entity) {
        BlockState state = entity.getBlockState();
        if (!(state.getBlock() instanceof EntityBlock block)) return;
        var ticker = block.getTicker(level, state, (BlockEntityType) entity.getType());
        if (ticker != null) ticker.tick(level, entity.getBlockPos(), state, entity);
    }

    /** Each piece of the block entity's drawing at successive ticks, with the client's ticker run between them. */
    private static List<List<Draw>> samples(ClientLevel level, BlockEntity entity, BlockEntityRenderer<?, ?> renderer) {
        var camera = new CameraRenderState();
        camera.pos = Vec3.atCenterOf(entity.getBlockPos()).add(0, 1, 4);
        camera.blockPos = BlockPos.containing(camera.pos);
        camera.initialized = true;
        var data = (ClientLevel.ClientLevelData) level.getLevelData();
        long base = data.getGameTime();
        var samples = new ArrayList<List<Draw>>();
        sampling = true;
        try {
            for (int s = 0; s < SAMPLES; s++) {
                data.setGameTime(base + s);
                if (s > 0) {
                    try {
                        tick(level, entity);
                    } catch (Throwable t) {
                        // A client ticker that needs what this level lacks: the drawing stays as it was.
                    }
                }
                samples.add(sample(renderer, entity, camera));
            }
        } finally {
            ITEMS.clear();
            sampling = false;
            data.setGameTime(base);
        }
        return samples;
    }

    /** A value over the sampled ticks: constant, linear, or `a sin(w t) + b cos(w t) + c`; null when it is none of them. */
    static float[] fit(double[] x) {
        int n = x.length;
        double scale = 1;
        for (double v : x) scale = Math.max(scale, Math.abs(v));
        double tolerance = 2e-4 * scale;
        boolean constant = true;
        for (double v : x) constant &= Math.abs(v - x[0]) <= tolerance;
        if (constant) return new float[] {0, (float) x[0]};
        double slope = (x[n - 1] - x[0]) / (n - 1);
        boolean linear = true;
        for (int i = 0; i < n; i++) linear &= Math.abs(x[0] + slope * i - x[i]) <= tolerance;
        if (linear) return new float[] {1, (float) x[0], (float) slope};
        double best = Double.MAX_VALUE;
        double[] fit = null;
        for (int k = 1; k <= 4000; k++) {
            double w = Math.PI * k / 4000;
            double[][] a = new double[3][3];
            double[] b = new double[3];
            for (int i = 0; i < n; i++) {
                double[] row = {Math.sin(w * i), Math.cos(w * i), 1};
                for (int r = 0; r < 3; r++) {
                    b[r] += row[r] * x[i];
                    for (int c = 0; c < 3; c++) a[r][c] += row[r] * row[c];
                }
            }
            double[] p = solve(a, b);
            if (p == null) continue;
            double error = 0;
            for (int i = 0; i < n; i++) error = Math.max(error, Math.abs(p[0] * Math.sin(w * i) + p[1] * Math.cos(w * i) + p[2] - x[i]));
            if (error < best) {
                best = error;
                fit = new double[] {p[0], p[1], p[2], w};
            }
        }
        if (fit == null || best > 2e-3 * scale) return null;
        return new float[] {2, (float) fit[0], (float) fit[1], (float) fit[2], (float) fit[3]};
    }

    private static double[] solve(double[][] a, double[] b) {
        int n = b.length;
        double[][] m = new double[n][n + 1];
        for (int r = 0; r < n; r++) {
            System.arraycopy(a[r], 0, m[r], 0, n);
            m[r][n] = b[r];
        }
        for (int c = 0; c < n; c++) {
            int pivot = c;
            for (int r = c + 1; r < n; r++) if (Math.abs(m[r][c]) > Math.abs(m[pivot][c])) pivot = r;
            if (Math.abs(m[pivot][c]) < 1e-12) return null;
            var t = m[c];
            m[c] = m[pivot];
            m[pivot] = t;
            for (int r = 0; r < n; r++) {
                if (r == c) continue;
                double f = m[r][c] / m[c][c];
                for (int k = c; k <= n; k++) m[r][k] -= f * m[c][k];
            }
        }
        double[] x = new double[n];
        for (int r = 0; r < n; r++) x[r] = m[r][n] / m[r][r];
        return x;
    }

    /** The matrix the operations build. */
    static Matrix4f build(List<Op> ops) {
        var m = new Matrix4f();
        for (var op : ops) {
            float[] v = op.v();
            switch (op.type()) {
                case 'T' -> m.translate(v[0], v[1], v[2]);
                case 'S' -> m.scale(v[0], v[1], v[2]);
                case 'Q' -> m.rotate(new Quaternionf(v[0], v[1], v[2], v[3]));
                case 'M' -> m.mul(new Matrix4f().set(v));
                case 'I' -> m.identity();
                default -> {}
            }
        }
        return m;
    }

    private static boolean close(Matrix4f a, Matrix4f b) {
        float[] x = a.get(new float[16]);
        float[] y = b.get(new float[16]);
        for (int i = 0; i < 16; i++) if (Math.abs(x[i] - y[i]) > 1e-3f * (1 + Math.abs(x[i]))) return false;
        return true;
    }

    private static boolean sameGeometry(Draw a, Draw b) {
        if ((a.item == null) != (b.item == null)) return false;
        if (a.item != null && (!net.minecraft.world.item.ItemStack.matches((net.minecraft.world.item.ItemStack) a.item[0], (net.minecraft.world.item.ItemStack) b.item[0]) || !a.item[1].equals(b.item[1]))) return false;
        if (a.quads.size() != b.quads.size()) return false;
        for (int i = 0; i < a.quads.size(); i++) {
            var p = a.quads.get(i);
            var q = b.quads.get(i);
            if (!p.texture().equals(q.texture())) return false;
            for (int k = 0; k < 20; k++) if (Math.abs(p.xyzuv()[k] - q.xyzuv()[k]) > 1e-3f) return false;
            for (int k = 0; k < 4; k++) if (p.color()[k] != q.color()[k] || p.light()[k] != q.light()[k]) return false;
        }
        return true;
    }

    /** One piece's operations as functions of the tick, or its first sample's matrix alone when they don't fit. */
    private static void writeMotion(DataOutputStream out, List<Draw> across) throws IOException {
        var first = across.get(0);
        List<float[]> program = motion(across);
        if (program == null) {
            out.writeInt(1);
            out.writeByte('M');
            for (float f : first.matrix.get(new float[16])) out.writeFloat(f);
            return;
        }
        out.writeInt(program.size());
        for (float[] op : program) for (int i = 0; i < op.length; i++) {
            if (i == 0) out.writeByte((int) op[0]);
            else out.writeFloat(op[i]);
        }
    }

    /** Encoded ops: type, then for T and S three params, for R an axis then a param, for Q four constants, for M sixteen; a param is kind then values. */
    private static List<float[]> motion(List<Draw> across) {
        var first = across.get(0);
        if (first.ops == null) return null;
        for (var d : across) {
            if (d.ops == null || d.ops.size() != first.ops.size() || !close(build(d.ops), d.matrix) || !sameGeometry(first, d)) return null;
            for (int i = 0; i < d.ops.size(); i++) if (d.ops.get(i).type() != first.ops.get(i).type()) return null;
        }
        var program = new ArrayList<float[]>();
        int n = across.size();
        for (int i = 0; i < first.ops.size(); i++) {
            char type = first.ops.get(i).type();
            int width = first.ops.get(i).v().length;
            double[][] values = new double[width][n];
            for (int s = 0; s < n; s++) for (int k = 0; k < width; k++) values[k][s] = across.get(s).ops.get(i).v()[k];
            boolean varies = false;
            for (int k = 0; k < width; k++) for (int s = 1; s < n; s++) varies |= Math.abs(values[k][s] - values[k][0]) > 1e-6;
            if (type == 'Q' && varies) {
                var axisAngle = axisAngles(across, i);
                if (axisAngle == null) return null;
                float[] angle = fit(axisAngle[1]);
                if (angle == null) return null;
                program.add(concat(new float[] {'R', (float) axisAngle[0][0], (float) axisAngle[0][1], (float) axisAngle[0][2]}, angle));
            } else if ((type == 'T' || type == 'S') && varies) {
                var op = new ArrayList<Float>();
                op.add((float) type);
                for (int k = 0; k < 3; k++) {
                    float[] f = fit(values[k]);
                    if (f == null) return null;
                    for (float v : f) op.add(v);
                }
                float[] flat = new float[op.size()];
                for (int k = 0; k < flat.length; k++) flat[k] = op.get(k);
                program.add(flat);
            } else if (varies) {
                return null;
            } else if (type == 'T' || type == 'S') {
                float[] v = first.ops.get(i).v();
                program.add(new float[] {type, 0, v[0], 0, v[1], 0, v[2]});
            } else if (type == 'I') {
                program.add(new float[] {'I'});
            } else {
                program.add(concat(new float[] {type}, first.ops.get(i).v()));
            }
        }
        return program;
    }

    /** A rotation's axis and its unwrapped angles across the samples, when the axis stays put. */
    private static double[][] axisAngles(List<Draw> across, int index) {
        int n = across.size();
        double[] axis = null;
        double[] angles = new double[n];
        for (int s = 0; s < n; s++) {
            float[] q = across.get(s).ops.get(index).v();
            var quat = new Quaternionf(q[0], q[1], q[2], q[3]).normalize();
            double angle = 2 * Math.acos(Math.max(-1, Math.min(1, quat.w)));
            double sin = Math.sqrt(Math.max(0, 1 - quat.w * quat.w));
            if (sin < 1e-6) {
                angles[s] = 0;
                continue;
            }
            double[] a = {quat.x / sin, quat.y / sin, quat.z / sin};
            if (axis == null) axis = a;
            double dot = a[0] * axis[0] + a[1] * axis[1] + a[2] * axis[2];
            if (Math.abs(Math.abs(dot) - 1) > 1e-3) return null;
            angles[s] = dot < 0 ? -angle : angle;
        }
        if (axis == null) return null;
        for (int s = 1; s < n; s++) {
            while (angles[s] - angles[s - 1] > Math.PI) angles[s] -= 2 * Math.PI;
            while (angles[s] - angles[s - 1] < -Math.PI) angles[s] += 2 * Math.PI;
        }
        return new double[][] {axis, angles};
    }

    private static float[] concat(float[] a, float[] b) {
        float[] c = java.util.Arrays.copyOf(a, a.length + b.length);
        System.arraycopy(b, 0, c, a.length, b.length);
        return c;
    }

    /** Each changed position's pieces: count -1 when nothing draws there now. */
    static byte[] take(ClientLevel level) throws IOException {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        var positions = DIRTY.toLongArray();
        DIRTY.clear();
        out.writeInt(positions.length);
        for (long at : positions) {
            out.writeLong(at);
            var entity = ClientBridge.blockEntity(level, BlockPos.of(at));
            var renderer = entity == null ? null : renderers().get(entity.getType());
            List<List<Draw>> samples = null;
            if (renderer != null) {
                try {
                    ClientBridge.refresh(level, entity);
                    samples = samples(level, entity, renderer);
                } catch (Throwable t) {
                    System.err.println("Horizon: the block entity renderer at " + BlockPos.of(at) + " failed: " + t);
                }
            }
            if (renderer != null) SEEN.put(at, seen(level, BlockPos.of(at)));
            else SEEN.remove(at);
            if (samples == null || samples.get(0).isEmpty()) {
                out.writeInt(-1);
                continue;
            }
            var first = samples.get(0);
            boolean aligned = true;
            for (var sample : samples) aligned &= sample.size() == first.size();
            out.writeInt(first.size());
            for (int i = 0; i < first.size(); i++) {
                var d = first.get(i);
                out.writeBoolean(d.type != null && d.type.hasBlending());
                var across = new ArrayList<Draw>();
                if (aligned) for (var sample : samples) across.add(sample.get(i));
                else across.add(d);
                writeMotion(out, across);
                out.writeInt(d.quads.size());
                for (var q : d.quads) {
                    byte[] name = q.texture().getBytes(java.nio.charset.StandardCharsets.UTF_8);
                    out.writeInt(name.length);
                    out.write(name);
                    for (int k = 0; k < 4; k++) {
                        for (int c = 0; c < 5; c++) out.writeFloat(q.xyzuv()[k * 5 + c]);
                        out.writeInt(q.color()[k]);
                        out.writeInt(q.light()[k]);
                    }
                }
                out.writeBoolean(d.item != null);
                if (d.item != null) {
                    var tag = (net.minecraft.nbt.CompoundTag) net.minecraft.world.item.ItemStack.CODEC.encodeStart(GameBridge.registries().createSerializationContext(net.minecraft.nbt.NbtOps.INSTANCE), (net.minecraft.world.item.ItemStack) d.item[0]).getOrThrow();
                    GameBridge.writeTag(out, tag);
                    out.writeByte((int) d.item[1]);
                    out.writeInt((int) d.item[2]);
                }
            }
        }
        return bytes.toByteArray();
    }
}
