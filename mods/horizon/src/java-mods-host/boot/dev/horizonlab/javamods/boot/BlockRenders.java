package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.vertex.PoseStack;
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
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.blockentity.BlockEntityRenderDispatcher;
import net.minecraft.client.renderer.blockentity.BlockEntityRenderer;
import net.minecraft.client.renderer.blockentity.BlockEntityRendererProvider;
import net.minecraft.client.renderer.blockentity.state.BlockEntityRenderState;
import net.minecraft.client.renderer.rendertype.RenderType;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.EntityBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.Vec3;
import org.joml.Matrix3f;
import org.joml.Matrix4f;
import org.joml.Quaternionf;

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

    /** Mods' renderers, as the client's resource reload made them. */
    @SuppressWarnings("unchecked")
    static synchronized Map<BlockEntityType<?>, BlockEntityRenderer<?, ?>> renderers() {
        if (renderers != null) return renderers;
        renderers = new HashMap<>();
        try {
            var map = BlockEntityRenderDispatcher.class.getDeclaredField("renderers");
            map.setAccessible(true);
            var all = (Map<BlockEntityType<?>, BlockEntityRenderer<?, ?>>) map.get(Minecraft.getInstance().getBlockEntityRenderDispatcher());
            all.forEach((type, renderer) -> {
                if (BootFacts.ownsBlockEntityType(type)) renderers.put(type, renderer);
            });
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
        if (!sampling && DIRTY.add(pos.asLong())) Clients.changes.incrementAndGet();
    }

    static int pending() {
        return DIRTY.size();
    }

    static void forget() {
        DIRTY.clear();
        PROBED.clear();
        DEPENDENT.clear();
        camera = null;
    }

    private static volatile Vec3 camera;
    /** Positions whose renderer draws different faces from another side: what a camera move records again. */
    private static final it.unimi.dsi.fastutil.longs.LongOpenHashSet DEPENDENT = new it.unimi.dsi.fastutil.longs.LongOpenHashSet();

    /** The window's camera, as `extractRenderState` gets it each frame: renderers choose faces by it, so those in range record again when its block changes. */
    static void camera(double x, double y, double z) {
        camera = new Vec3(x, y, z);
        var around = camera;
        var it = DEPENDENT.iterator();
        while (it.hasNext()) {
            var pos = BlockPos.of(it.nextLong());
            if (Vec3.atCenterOf(pos).distanceToSqr(around) <= 64.0 * 64.0) dirty(pos);
        }
    }

    /** The camera state a renderer sees for a block, the real camera or it mirrored through the block's centre. */
    private static CameraRenderState cameraAt(BlockPos pos, boolean mirrored) {
        var state = new CameraRenderState();
        var real = camera != null ? camera : Vec3.atCenterOf(pos).add(0, 1, 4);
        state.pos = mirrored ? Vec3.atCenterOf(pos).scale(2).subtract(real) : real;
        state.blockPos = BlockPos.containing(state.pos);
        state.initialized = true;
        return state;
    }

    /** Whether what is drawn differs between the camera and its mirror through the centre: such a renderer chooses its faces by the camera. */
    static boolean dependsOnCamera(java.util.function.Function<Vec3, List<Draw>> render, Vec3 center, Vec3 camera) {
        return hash(render.apply(camera)) != hash(render.apply(center.scale(2).subtract(camera)));
    }

    private static final it.unimi.dsi.fastutil.longs.Long2LongOpenHashMap PROBED = new it.unimi.dsi.fastutil.longs.Long2LongOpenHashMap();

    /** Vanilla draws every block entity each frame from the client's state, which a mod's payload changes without a block update: what a renderer draws now against what it drew at the last look, and a position whose drawing differs is recorded again. */
    static void probe(ClientLevel level, java.util.Collection<BlockEntity> entities) {
        for (var entity : entities) {
            var renderer = renderers().get(entity.getType());
            if (renderer == null || entity.isRemoved()) {
                DEPENDENT.remove(entity.getBlockPos().asLong());
                continue;
            }
            long at = entity.getBlockPos().asLong();
            if (DIRTY.contains(at)) continue;
            long hash;
            try {
                hash = hash(sample(renderer, entity, cameraAt(entity.getBlockPos(), false)));
            } catch (Throwable t) {
                failedOnce("a block entity renderer looked at " + entity.getBlockPos() + " and", t);
                continue;
            }
            boolean seen = PROBED.containsKey(at);
            if (PROBED.put(at, hash) != hash && seen) dirty(entity.getBlockPos());
        }
        ITEMS.clear();

    }

    /** What a piece draws, not where its pose has moved it: a turning piece keeps its hash. */
    static long hash(List<Draw> draws) {
        long h = draws.size();
        for (var d : draws) {
            h = h * 31 + d.quads.size();
            for (var q : d.quads) {
                h = h * 31 + q.texture().hashCode();
                h = h * 31 + java.util.Arrays.hashCode(q.xyzuv());
                h = h * 31 + java.util.Arrays.hashCode(q.color());
                h = h * 31 + java.util.Arrays.hashCode(q.light());
            }
            if (d.item != null) h = h * 31 + net.minecraft.world.item.ItemStack.hashItemAndComponents((net.minecraft.world.item.ItemStack) d.item[0]);
        }
        return h;
    }

    // --- Recording

    /** One piece a renderer submitted: its quads in the frame of its pose, which the operations build. */
    static final class Draw {
        List<Op> ops;
        Matrix4f matrix;
        Matrix4f local;
        RenderType type;
        final List<Quad> quads = new ArrayList<>();
        final List<Line> lines = new ArrayList<>();
        /** `submitShapeOutline`'s `afterTerrain`. */
        boolean afterTerrain;
        /** An item drawn here by Rust: its stack, `ItemDisplayContext` ordinal and seed. */
        Object[] item;
        /** The transpose of the pose's rotation, which takes a normal the pose moved back into the piece's frame. */
        Matrix3f normals;
        /** `ItemStackRenderState.FoilType` ordinal, for a recorded item. */
        int foil;
    }

    record Quad(String texture, float[] xyzuv, int[] color, int[] light, float[] normal, boolean blended) {}

    /** A segment of a line topology's piece: its ends in the piece's frame, ARGB and width in pixels. */
    record Line(float[] ends, int color, float width) {}

    static SubmitCapture CURRENT;
    private static SubmitCapture ACTIVE;

    static ArrayList<Op> ops(Object pose) {
        return OPS.get(pose);
    }
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
        var capture = new SubmitCapture(false);
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

    /** A tick's time wherever a renderer reads it: the level's, and the frame's `LevelRenderer.extractLevel` copies from it. */
    private static void sampleTime(ClientLevel level, long time) {
        ((ClientLevel.ClientLevelData) level.getLevelData()).setGameTime(time);
        Minecraft.getInstance().gameRenderer.gameRenderState().levelRenderState.gameTime = time;
    }

    /** Each field of a block entity with its value now, for a recording to put back: the level's own ticks advance it, not this. */
    private static List<Runnable> fields(BlockEntity entity) {
        var restore = new ArrayList<Runnable>();
        for (Class<?> c = entity.getClass(); c != null && c != Object.class; c = c.getSuperclass()) {
            for (var field : c.getDeclaredFields()) {
                if (java.lang.reflect.Modifier.isStatic(field.getModifiers())) continue;
                try {
                    field.setAccessible(true);
                    Object value = field.get(entity);
                    restore.add(() -> {
                        try {
                            field.set(entity, value);
                        } catch (IllegalAccessException e) {
                            throw new IllegalStateException(e);
                        }
                    });
                } catch (IllegalAccessException | RuntimeException e) {
                    // A field its module keeps closed stays as the recording left it.
                }
            }
        }
        return restore;
    }

    /** Each piece of the block entity's drawing at successive ticks, with the client's ticker run between them on the state it puts back after. */
    private static List<List<Draw>> samples(ClientLevel level, BlockEntity entity, BlockEntityRenderer<?, ?> renderer) {
        var camera = cameraAt(entity.getBlockPos(), false);
        long base = level.getGameTime();
        var samples = new ArrayList<List<Draw>>();
        var restore = fields(entity);
        sampling = true;
        try {
            for (int s = 0; s < SAMPLES; s++) {
                sampleTime(level, base + s);
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
            sampleTime(level, base);
            restore.forEach(Runnable::run);
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

    private static final java.util.Set<String> FAILED = java.util.concurrent.ConcurrentHashMap.newKeySet();

    private static void failedOnce(String what, Throwable t) {
        if (FAILED.add(String.valueOf(t))) {
            System.err.println("Horizon: " + what + " didn't draw in Java:");
            t.printStackTrace();
        }
    }

    /** `ItemStackRenderState.submit` of the stack as it is now: its pieces, each in its pose's frame. */
    private static List<Draw> submitItem(net.minecraft.client.renderer.item.ItemStackRenderState state, net.minecraft.world.item.ItemStack stack) {
        var stackPose = new PoseStack();
        OPS.clear();
        OPS.put(stackPose.last(), new ArrayList<>());
        var capture = new SubmitCapture(false);
        capture.foil = stack.hasFoil() ? net.minecraft.client.renderer.item.ItemStackRenderState.FoilType.STANDARD.ordinal() : 0;
        ACTIVE = capture;
        tracking = true;
        try {
            state.submit(stackPose, capture, 0xF000F0, net.minecraft.client.renderer.texture.OverlayTexture.NO_OVERLAY, 0);
            capture.geometry.flush();
        } finally {
            tracking = false;
            CURRENT = null;
            ACTIVE = null;
            OPS.clear();
        }
        capture.geometry.draws.removeIf(d -> d.quads.isEmpty());
        return capture.geometry.draws;
    }

    /** Whether the pipeline applies `minecraft_mix_light`: what doesn't is drawn at its full colour, in a GUI as anywhere. */
    private static boolean cardinalLit(RenderType type) {
        if (type == null) return true;
        var pipeline = type.pipeline();
        var vertex = pipeline.getShaders().get(com.mojang.renderpearl.api.pipeline.ShaderType.VERTEX);
        return LIT_SHADERS.contains(String.valueOf(vertex)) && !pipeline.getShaderDefines().flags().contains("NO_CARDINAL_LIGHTING");
    }

    private static final java.util.Set<String> LIT_SHADERS = java.util.Set.of("minecraft:core/entity", "minecraft:core/item");

    /** Each requested stack's model in one display context, as vanilla's `ItemStackRenderState` draws it: count -1 when it fails. It is sampled over ticks, as a block entity's renderer is, so Rust moves what moves. */
    static byte[] items(java.io.DataInputStream in) throws IOException {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        int n = in.readInt();
        out.writeInt(n);
        var minecraft = Minecraft.getInstance();
        var resolver = new net.minecraft.client.renderer.item.ItemModelResolver(minecraft.getModelManager());
        for (int i = 0; i < n; i++) {
            byte[] nbt = in.readNBytes(in.readInt());
            int context = in.readByte();
            var samples = new ArrayList<List<Draw>>();
            var state = new net.minecraft.client.renderer.item.ItemStackRenderState();
            try {
                var stack = GameBridge.decode(nbt);
                var display = net.minecraft.world.item.ItemDisplayContext.values()[context];
                var level = minecraft.level;
                resolver.updateForTopItem(state, stack, display, level, minecraft.player, 0);
                samples.add(submitItem(state, stack));
                // Not only `isAnimated`: a special model's motion comes from its argument, which isn't flagged.
                if (level != null) {
                    long base = level.getGameTime();
                    try {
                        for (int s = 1; s < SAMPLES; s++) {
                            sampleTime(level, base + s);
                            var later = new net.minecraft.client.renderer.item.ItemStackRenderState();
                            resolver.updateForTopItem(later, stack, display, level, minecraft.player, 0);
                            samples.add(submitItem(later, stack));
                        }
                    } finally {
                        sampleTime(level, base);
                    }
                }
            } catch (Throwable t) {
                failedOnce("an item's model", t);
                out.writeInt(-1);
                continue;
            }
            var first = samples.get(0);
            boolean aligned = true;
            for (var sample : samples) aligned &= sample.size() == first.size();
            out.writeInt(first.size());
            out.writeBoolean(state.usesBlockLight());
            out.writeBoolean(state.isOversizedInGui());
            for (int p = 0; p < first.size(); p++) {
                var d = first.get(p);
                var across = new ArrayList<Draw>();
                if (aligned) for (var sample : samples) across.add(sample.get(p));
                else across.add(d);
                writeMotion(out, across);
                out.writeByte(d.foil);
                boolean lit = cardinalLit(d.type);
                boolean cull = d.type == null || d.type.pipeline().isCull();
                out.writeInt(d.quads.size());
                for (var q : d.quads) {
                    byte[] name = q.texture().getBytes(java.nio.charset.StandardCharsets.UTF_8);
                    out.writeInt(name.length);
                    out.write(name);
                    for (int k = 0; k < 4; k++) {
                        for (int c = 0; c < 5; c++) out.writeFloat(q.xyzuv()[k * 5 + c]);
                        out.writeInt(q.color()[k]);
                    }
                    for (int c = 0; c < 3; c++) out.writeFloat(lit && q.normal() != null ? q.normal()[c] : 0);
                    out.writeBoolean(q.blended());
                    out.writeBoolean(cull);
                }
            }
        }
        return bytes.toByteArray();
    }

    /** What NeoForge's item decorators draw over each requested stack in a slot at the origin, as a screen's frame: length -1 when one fails. */
    static byte[] decorations(java.io.DataInputStream in) throws IOException {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        int n = in.readInt();
        out.writeInt(n);
        var minecraft = Minecraft.getInstance();
        for (int i = 0; i < n; i++) {
            byte[] nbt = in.readNBytes(in.readInt());
            try {
                var stack = GameBridge.decode(nbt);
                var recorder = new Recorder();
                net.neoforged.neoforge.client.ItemDecoratorHandler.of(stack).render(new net.minecraft.client.gui.GuiGraphicsExtractor(minecraft, recorder, 0, 0), minecraft.font, stack, 0, 0);
                byte[] frame = recorder.bytes();
                out.writeInt(frame.length);
                out.write(frame);
            } catch (Throwable t) {
                failedOnce("an item's decorator", t);
                out.writeInt(-1);
            }
        }
        return bytes.toByteArray();
    }

    /** Each changed position's pieces: count -1 when nothing draws there now. */
    static byte[] take(ClientLevel level) throws IOException {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        // The server thread waits on this JVM meanwhile: a batch stops after about 8 ms, the rest wait for the next take.
        long began = System.nanoTime();
        var taken = new java.util.ArrayList<Long>();
        var iterator = DIRTY.iterator();
        while (iterator.hasNext() && (taken.isEmpty() || System.nanoTime() - began < 8_000_000L)) {
            long at = iterator.nextLong();
            iterator.remove();
            taken.add(at);
            entry(level, at, out);
        }
        var whole = new ByteArrayOutputStream();
        var header = new DataOutputStream(whole);
        header.writeInt(taken.size());
        bytes.writeTo(whole);
        return whole.toByteArray();
    }

    /** One position's pieces, or -1 when nothing draws there now. */
    private static void entry(ClientLevel level, long at, DataOutputStream out) throws IOException {
        out.writeLong(at);
        var entity = ClientBridge.blockEntity(level, BlockPos.of(at));
        out.writeInt(entity == null ? -1 : Block.BLOCK_STATE_REGISTRY.getId(entity.getBlockState()));
        var renderer = entity == null ? null : renderers().get(entity.getType());
        List<List<Draw>> samples = null;
        if (renderer != null) {
            try {
                samples = samples(level, entity, renderer);
            } catch (Throwable t) {
                failedOnce("the block entity renderer at " + BlockPos.of(at), t);
            }
        }
        if (samples == null || samples.get(0).isEmpty()) {
            if (entity == null || renderer == null) DEPENDENT.remove(at);
            out.writeInt(-1);
            return;
        }
        var first = samples.get(0);
        PROBED.put(at, hash(first));
        try {
            var mirror = cameraAt(BlockPos.of(at), true);
            if (hash(sample(renderer, entity, mirror)) != hash(first)) DEPENDENT.add(at);
            else DEPENDENT.remove(at);
        } catch (Throwable t) {
            DEPENDENT.remove(at);
        } finally {
            ITEMS.clear();
        }
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
}
