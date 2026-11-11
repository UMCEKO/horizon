package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.api.commands.RenderPass;
import com.mojang.renderpearl.api.pipeline.BlendFactor;
import com.mojang.renderpearl.api.pipeline.CompareOp;
import com.mojang.renderpearl.api.pipeline.CompiledRenderPipeline;
import com.mojang.renderpearl.api.pipeline.DepthStencilState;
import com.mojang.renderpearl.api.pipeline.IndexType;
import com.mojang.renderpearl.api.pipeline.PolygonMode;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import dev.horizonlab.javamods.boot.BlockRenders.Draw;
import dev.horizonlab.javamods.boot.BlockRenders.Line;
import dev.horizonlab.javamods.boot.BlockRenders.Quad;
import dev.horizonlab.javamods.boot.RecordingDevice.RecordedDraw;
import dev.horizonlab.javamods.boot.mixin.CameraAccess;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import net.minecraft.client.Camera;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.DynamicGpuData;
import net.minecraft.client.renderer.LevelRenderer;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.culling.Frustum;
import net.minecraft.client.renderer.oit.OitStage;
import net.minecraft.client.renderer.state.OptionsRenderState;
import net.minecraft.client.renderer.state.level.LevelRenderState;
import net.minecraft.core.BlockPos;
import net.minecraft.resources.Identifier;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.bus.api.Event;
import net.neoforged.neoforge.client.IRenderableSection;
import net.neoforged.neoforge.client.event.ExtractLevelRenderStateEvent;
import net.neoforged.neoforge.client.event.PrepareRenderBuffersEvent;
import net.neoforged.neoforge.client.event.RenderLevelStageEvent;
import net.neoforged.neoforge.client.event.SubmitCustomGeometryEvent;
import net.neoforged.neoforge.common.NeoForge;
import org.joml.Matrix4d;
import org.joml.Matrix4f;
import org.joml.Matrix4fc;
import org.joml.Vector3f;

/** What the level's frame would draw for mods, recorded as world-space geometry: the real client's events, fired in its order against vanilla's state objects, with the submitted geometry and the draws on a recording GPU kept for Rust to draw every frame. */
public final class WorldFrame {
    private WorldFrame() {}

    public static final int SUBMIT = 0, AFTER_SKY = 1, AFTER_OPAQUE_BLOCKS = 2, AFTER_OPAQUE_FEATURES = 3, AFTER_TRANSLUCENT_FEATURES = 4, AFTER_TRANSLUCENT_BLOCKS = 5,
            AFTER_TRANSLUCENT_PARTICLES = 6, AFTER_WEATHER = 7, AFTER_LEVEL = 8, OIT = 9;

    /** Passes on the wire. */
    static final byte OPAQUE = 0, TRANSLUCENT = 1, AFTER_TRANSLUCENT = 2, ADDITIVE = 3, SEE_THROUGH = 4, LINES = 5, LINES_ON_TOP = 6;

    /** The recording device is the GPU while this is set, on the render thread only. */
    public static volatile boolean recording;
    /** The stage whose event is being posted: draws made then are tagged with it. */
    public static volatile int stage;
    public static final RecordingDevice DEVICE = new RecordingDevice();

    private static DynamicGpuData uniforms;
    private static final Map<RenderPipeline, RecordingPipeline> PIPELINES = new HashMap<>();

    public static synchronized DynamicGpuData uniforms() {
        if (uniforms == null) uniforms = Sprites.unallocated(DynamicGpuData.class);
        return uniforms;
    }

    public static GpuBufferSlice transform(DynamicGpuData.Transform transform) {
        return new GpuBufferSlice(new RecordingTransform(DEVICE, transform), 0, 0);
    }

    private static final Sprites.View STAND_IN = new Sprites.View(Identifier.fromNamespaceAndPath("horizon", "textures/recording.png"), 1, 1, false);
    private static net.minecraft.client.renderer.texture.OverlayTexture overlay;

    /** The overlay and lightmap a `RenderType.prepare` reads: textures the GPU would hold, which a recording only names. */
    public static synchronized net.minecraft.client.renderer.texture.OverlayTexture overlay() throws ReflectiveOperationException {
        if (overlay == null) {
            var dynamic = Sprites.unallocated(net.minecraft.client.renderer.texture.DynamicTexture.class);
            var view = net.minecraft.client.renderer.texture.AbstractTexture.class.getDeclaredField("textureView");
            view.setAccessible(true);
            view.set(dynamic, STAND_IN);
            overlay = Sprites.unallocated(net.minecraft.client.renderer.texture.OverlayTexture.class);
            var texture = net.minecraft.client.renderer.texture.OverlayTexture.class.getDeclaredField("texture");
            texture.setAccessible(true);
            texture.set(overlay, dynamic);
        }
        return overlay;
    }

    public static com.mojang.renderpearl.api.textures.GpuTextureView lightmap() {
        return STAND_IN;
    }

    public static CompiledRenderPipeline pipeline(RenderPipeline pipeline) {
        return PIPELINES.computeIfAbsent(pipeline, RecordingPipeline::new);
    }

    // --- Limits

    static final int MAX_QUADS = 262_144;
    static final int MAX_LINES = 65_536;
    static final long MAX_NEW_BYTES = 8L << 20;
    private static final Set<String> WARNED = new HashSet<>();

    private static void warnOnce(String what, String key) {
        if (WARNED.add(what + key)) System.err.println("Horizon: " + what + " " + key);
    }

    // --- Camera

    /** The window's camera for one recording: where it is, how it looks, and its lens. */
    public record Cam(double x, double y, double z, float yaw, float pitch, float fov, float far) {}

    private static Cam camera;
    private static int width = 854, height = 480;

    /** The window's camera, as the input events tell it; recordings are taken from it. */
    public static void camera(Cam cam) {
        camera = cam;
    }

    // --- Events, in the real client's order

    /** Everything an event of the frame is built from. */
    record Frame(LevelRenderer renderer, net.minecraft.client.renderer.extract.LevelExtractor extractor, LevelRenderState state, OptionsRenderState options, ClientLevel level, Camera camera,
            Frustum frustum, DeltaTracker delta, SubmitNodeCollector collector, Matrix4fc viewRotation, Iterable<IRenderableSection> sections, RenderPass pass, boolean sky, boolean oit) {}

    interface Sink {
        void post(String what, Event event);
    }

    /** `ExtractLevelRenderStateEvent`, `SubmitCustomGeometryEvent`, `PrepareRenderBuffersEvent`, then the stages' events; `FrameGraphSetupEvent` needs a frame graph and its targets, which this client has none of. */
    static void fire(Frame f, Sink sink) {
        stage = SUBMIT;
        sink.post("ExtractLevelRenderStateEvent", new ExtractLevelRenderStateEvent(f.extractor(), f.state(), f.options(), f.level(), f.camera(), f.frustum(), f.delta()));
        stage = SUBMIT;
        sink.post("SubmitCustomGeometryEvent", new SubmitCustomGeometryEvent(f.state(), f.collector(), new PoseStack(), f.sections()));
        sink.post("PrepareRenderBuffersEvent", new PrepareRenderBuffersEvent(f.renderer(), f.state(), f.options()));
        if (f.sky()) {
            stage = AFTER_SKY;
            sink.post("RenderLevelStageEvent.AfterSky", new RenderLevelStageEvent.AfterSky(f.renderer(), f.state(), new PoseStack(), f.viewRotation(), f.sections()));
        }
        stage = AFTER_OPAQUE_BLOCKS;
        sink.post("RenderLevelStageEvent.AfterOpaqueBlocks", new RenderLevelStageEvent.AfterOpaqueBlocks(f.renderer(), f.state(), new PoseStack(), f.viewRotation(), f.sections(), f.pass()));
        stage = AFTER_OPAQUE_FEATURES;
        sink.post("RenderLevelStageEvent.AfterOpaqueFeatures", new RenderLevelStageEvent.AfterOpaqueFeatures(f.renderer(), f.state(), new PoseStack(), f.viewRotation(), f.sections(), f.pass()));
        stage = AFTER_TRANSLUCENT_FEATURES;
        sink.post("RenderLevelStageEvent.AfterTranslucentFeatures", new RenderLevelStageEvent.AfterTranslucentFeatures(f.renderer(), f.state(), new PoseStack(), f.viewRotation(), f.sections(), f.pass()));
        stage = AFTER_TRANSLUCENT_BLOCKS;
        sink.post("RenderLevelStageEvent.AfterTranslucentBlocks", new RenderLevelStageEvent.AfterTranslucentBlocks(f.renderer(), f.state(), new PoseStack(), f.viewRotation(), f.sections(), f.pass()));
        stage = AFTER_TRANSLUCENT_PARTICLES;
        sink.post("RenderLevelStageEvent.AfterTranslucentParticles", new RenderLevelStageEvent.AfterTranslucentParticles(f.renderer(), f.state(), new PoseStack(), f.viewRotation(), f.sections(), f.pass()));
        stage = AFTER_WEATHER;
        sink.post("RenderLevelStageEvent.AfterWeather", new RenderLevelStageEvent.AfterWeather(f.renderer(), f.state(), new PoseStack(), f.viewRotation(), f.sections(), f.pass()));
        stage = AFTER_LEVEL;
        sink.post("RenderLevelStageEvent.AfterLevel", new RenderLevelStageEvent.AfterLevel(f.renderer(), f.state(), new PoseStack(), f.viewRotation(), f.sections()));
        if (f.oit()) {
            stage = OIT;
            for (OitStage oit : OitStage.values()) {
                sink.post("RenderLevelStageEvent.OitTranslucent", new RenderLevelStageEvent.OitTranslucent(f.renderer(), f.state(), oit, new PoseStack(), f.viewRotation(), f.sections(), f.pass()));
            }
        }
    }

    private static void safely(String what, Event event) {
        long start = Profile.ON ? System.nanoTime() : 0;
        try {
            NeoForge.EVENT_BUS.post(event);
        } catch (Throwable t) {
            System.err.println("Horizon: a Java mod threw during " + what);
            t.printStackTrace();
        }
        if (Profile.ON) Profile.event(what, System.nanoTime() - start);
    }

    // --- Passes: a pipeline's own state decides, never its name

    /** The native pass a piece of this pipeline's belongs in, from the pipeline's state and the stage that emitted it. */
    static byte classify(RenderPipeline pipeline, int emittedAt) {
        var topology = pipeline.getPrimitiveTopology();
        var depth = pipeline.getDepthStencilState();
        boolean always = depth == null || depth.depthTest() == CompareOp.ALWAYS_PASS;
        if (topology == PrimitiveTopology.LINES || topology == PrimitiveTopology.DEBUG_LINES || topology == PrimitiveTopology.DEBUG_LINE_STRIP) return always ? LINES_ON_TOP : LINES;
        if (always) return SEE_THROUGH;
        var blend = firstBlend(pipeline);
        if (blend == null) return OPAQUE;
        if (blend.color().destFactor() == BlendFactor.ONE && (blend.color().sourceFactor() == BlendFactor.ONE || blend.color().sourceFactor() == BlendFactor.SRC_ALPHA)) return ADDITIVE;
        return emittedAt >= AFTER_TRANSLUCENT_BLOCKS ? AFTER_TRANSLUCENT : TRANSLUCENT;
    }

    private static com.mojang.renderpearl.api.pipeline.BlendFunction firstBlend(RenderPipeline pipeline) {
        for (var target : pipeline.getColorTargetStates()) if (target != null && target.blendFunction().isPresent()) return target.blendFunction().get();
        return null;
    }

    /** What the native passes cannot draw like the pipeline does. */
    private static void unsupported(RenderPipeline pipeline) {
        var targets = pipeline.getColorTargetStates().stream().filter(java.util.Objects::nonNull).count();
        if (pipeline.getPolygonMode() == PolygonMode.WIREFRAME) warnOnce("a world recording's pipeline draws wireframe, which is drawn filled:", pipeline.getLocation().toString());
        else if (targets > 1) warnOnce("a world recording's pipeline has more than one color target, which is drawn to one:", pipeline.getLocation().toString());
    }

    // --- Decoding a draw

    /** Geometry in a mesh's own frame, sent once. */
    record Mesh(long id, List<Quad> quads, List<Line> lines) {}

    /** A mesh placed in the world: `origin + linear * local`. */
    record Piece(long mesh, byte pass, byte flags, double[] origin, float[] linear, float[] modulator) {}

    private static int u8(byte[] b, long at) {
        return b[(int) at] & 255;
    }

    private static float f32(byte[] b, long at) {
        return Float.intBitsToFloat((b[(int) at] & 255) | (b[(int) at + 1] & 255) << 8 | (b[(int) at + 2] & 255) << 16 | (b[(int) at + 3] & 255) << 24);
    }

    private static int s16(byte[] b, long at) {
        return (short) ((b[(int) at] & 255) | (b[(int) at + 1] & 255) << 8);
    }

    private static long indexAt(RecordedDraw d, int k) {
        long at = (long) k * d.indexType().bytes;
        return d.indexType() == IndexType.SHORT ? (u8(d.index(), at) | u8(d.index(), at + 1) << 8) : (u8(d.index(), at) | u8(d.index(), at + 1) << 8 | u8(d.index(), at + 2) << 16 | (long) u8(d.index(), at + 3) << 24);
    }

    /** The vertices a draw names, in the order its topology consumes them. */
    static int[] order(RecordedDraw d, PrimitiveTopology topology) {
        int[] out;
        if (d.index() != null) {
            int available = (int) Math.min(d.count(), d.index().length / d.indexType().bytes - d.first());
            if (available <= 0) return new int[0];
            int[] raw = new int[available];
            for (int k = 0; k < available; k++) raw[k] = (int) indexAt(d, d.first() + k) + d.base();
            if (topology == PrimitiveTopology.QUADS && available % 6 == 0) {
                out = new int[available / 6 * 4];
                for (int q = 0; q < available / 6; q++) {
                    out[q * 4] = raw[q * 6];
                    out[q * 4 + 1] = raw[q * 6 + 1];
                    out[q * 4 + 2] = raw[q * 6 + 2];
                    out[q * 4 + 3] = raw[q * 6 + 4];
                }
                return out;
            }
            if (topology == PrimitiveTopology.LINES && available % 6 == 0) {
                out = new int[available / 6 * 2];
                for (int q = 0; q < available / 6; q++) {
                    out[q * 2] = raw[q * 6];
                    out[q * 2 + 1] = raw[q * 6 + 2];
                }
                return out;
            }
            return raw;
        }
        int[] raw = new int[d.count()];
        for (int k = 0; k < raw.length; k++) raw[k] = d.first() + k;
        if (topology == PrimitiveTopology.LINES) {
            out = new int[raw.length / 4 * 2];
            for (int q = 0; q < raw.length / 4; q++) {
                out[q * 2] = raw[q * 4];
                out[q * 2 + 1] = raw[q * 4 + 2];
            }
            return out;
        }
        return raw;
    }

    /** A draw's geometry in its buffer's own frame, with the vertex format read by its elements. */
    static Mesh decode(RecordedDraw d, long id) {
        var pipeline = d.pipeline().pipeline();
        var format = pipeline.getVertexFormatBinding(0);
        var topology = pipeline.getPrimitiveTopology();
        if (format == null) return new Mesh(id, List.of(), List.of());
        var position = format.getElement("Position");
        var color = format.getElement("Color");
        var uv = format.getElement("UV0");
        var light = format.getElement("UV2");
        var width = format.getElement("LineWidth");
        int stride = format.getVertexSize();
        var capture = new SubmitCapture(true);
        var draw = new Draw();
        draw.matrix = new Matrix4f();
        draw.local = new Matrix4f();
        draw.normals = new org.joml.Matrix3f();
        var geometry = capture.geometry;
        geometry.begin(draw, topology, d.textured() ? d.texture() : null, firstBlend(pipeline) != null);
        byte[] v = d.vertex();
        for (int index : order(d, topology)) {
            long base = d.vertexOffset() + (long) index * stride;
            if (base + stride > v.length) break;
            geometry.addVertex(f32(v, base + position.offset()), f32(v, base + position.offset() + 4), f32(v, base + position.offset() + 8));
            if (color != null) {
                long at = base + color.offset();
                geometry.setColor(u8(v, at), u8(v, at + 1), u8(v, at + 2), u8(v, at + 3));
            }
            if (uv != null) geometry.setUv(f32(v, base + uv.offset()), f32(v, base + uv.offset() + 4));
            if (light != null) geometry.setUv2(s16(v, base + light.offset()), s16(v, base + light.offset() + 2));
            if (width != null) geometry.setLineWidth(f32(v, base + width.offset()));
        }
        geometry.finish();
        return new Mesh(id, draw.quads, draw.lines);
    }

    /** The matrix from a draw's buffer frame to the world, in double: `translate(camera) * inverse(viewRotation) * ModelViewMat * translate(ModelOffset)`. */
    static Matrix4d toWorld(DynamicGpuData.Transform transform, Matrix4fc viewRotation, double x, double y, double z) {
        var inverse = new Matrix4d(viewRotation).invert();
        var offset = transform.modelOffset();
        return new Matrix4d().translation(x, y, z).mul(inverse).mul(new Matrix4d(transform.modelView())).translate(offset.x(), offset.y(), offset.z());
    }

    private static double quantize(double v) {
        return Math.round(v * 1024.0) / 1024.0;
    }

    private static float linear(double v) {
        return (float) (Math.round(v * 65536.0) / 65536.0);
    }

    /** A piece's flags: 1 no cull, 2 writes depth. */
    static int flags(RenderPipeline pipeline) {
        return flags(pipeline.isCull(), pipeline.getDepthStencilState());
    }

    static int flags(boolean cull, DepthStencilState depth) {
        return (cull ? 0 : 1) | (depth != null && depth.writeDepth() ? 2 : 0);
    }

    private static Piece placed(long mesh, byte pass, int flags, Matrix4d toWorld, float[] modulator) {
        double[] origin = {quantize(toWorld.m30()), quantize(toWorld.m31()), quantize(toWorld.m32())};
        float[] linear = {linear(toWorld.m00()), linear(toWorld.m01()), linear(toWorld.m02()), linear(toWorld.m10()), linear(toWorld.m11()), linear(toWorld.m12()), linear(toWorld.m20()),
                linear(toWorld.m21()), linear(toWorld.m22())};
        return new Piece(mesh, pass, (byte) flags, origin, linear, modulator);
    }

    // --- Recording

    /** A recording's pieces, the meshes they name and what it costs on the wire. */
    record Recording(List<Piece> pieces, Map<Long, Mesh> meshes) {}

    private static final Map<Long, Mesh> DECODED = new HashMap<>();
    private static final Map<Long, long[]> MESH_BUFFERS = new HashMap<>();
    private static final Set<Long> SENT = new HashSet<>();
    private static byte[] lastPieces = new byte[0];
    private static int generation;

    private static long hash(long h, long v) {
        h ^= v;
        h *= 0x100000001b3L;
        return h ^ (h >>> 29);
    }

    private static long hash(long h, String s) {
        return hash(h, s.hashCode());
    }

    private static long meshKey(RecordedDraw d) {
        long h = 0xcbf29ce484222325L;
        for (long v : new long[] {d.vertexId(), d.vertexVersion(), d.vertexOffset(), d.first(), d.count(), d.base(), d.indexId(), d.indexVersion(), d.indexType().ordinal(), d.index() == null ? 0 : 1, d.textured() ? 1 : 0}) h = hash(h, v);
        h = hash(h, d.pipeline().pipeline().getLocation().toString());
        return hash(h, d.texture() == null ? "" : d.texture().toString());
    }

    private static long contentKey(List<Quad> quads, List<Line> lines) {
        long h = 0xcbf29ce484222325L;
        h = hash(h, quads.size());
        for (var q : quads) {
            h = hash(h, q.texture());
            h = hash(h, Arrays.hashCode(q.xyzuv()));
            h = hash(h, Arrays.hashCode(q.color()));
            h = hash(h, Arrays.hashCode(q.light()));
        }
        h = hash(h, lines.size());
        for (var l : lines) {
            h = hash(h, Arrays.hashCode(l.ends()));
            h = hash(h, l.color());
            h = hash(h, Float.floatToIntBits(l.width()));
        }
        return h;
    }

    /** Submitted geometry, in the level's frame (the camera's, as a renderer builds it), becomes world-space meshes grouped by pass: each is anchored at a block so equal geometry is equal wherever the camera is. */
    static void submitted(SubmitCapture capture, Cam cam, Recording out, List<Piece> pieces) {
        capture.geometry.flush();
        var groups = new LinkedHashMap<Integer, List<Draw>>();
        for (var draw : capture.geometry.draws) {
            if (draw.quads.isEmpty() && draw.lines.isEmpty()) continue;
            byte pass;
            int flags = 0;
            if (draw.type != null) {
                pass = classify(draw.type.pipeline(), SUBMIT);
                flags = flags(draw.type.pipeline());
                unsupported(draw.type.pipeline());
                if (draw.afterTerrain && pass == TRANSLUCENT) pass = AFTER_TRANSLUCENT;
            } else {
                pass = draw.quads.stream().anyMatch(Quad::blended) ? TRANSLUCENT : OPAQUE;
            }
            groups.computeIfAbsent(pass | flags << 8, k -> new ArrayList<>()).add(draw);
        }
        for (var group : groups.entrySet()) {
            byte pass = (byte) (group.getKey() & 255);
            int flags = group.getKey() >> 8;
            var quads = new ArrayList<Quad>();
            var lines = new ArrayList<Line>();
            for (var draw : group.getValue()) {
                quads.addAll(draw.quads);
                lines.addAll(draw.lines);
            }
            double[] anchor = null;
            var world = new ArrayList<Quad>();
            var worldLines = new ArrayList<Line>();
            for (var q : quads) {
                var p = new float[20];
                for (int k = 0; k < 4; k++) {
                    double[] w = {quantize(cam.x() + q.xyzuv()[k * 5]), quantize(cam.y() + q.xyzuv()[k * 5 + 1]), quantize(cam.z() + q.xyzuv()[k * 5 + 2])};
                    if (anchor == null) anchor = new double[] {Math.floor(w[0]), Math.floor(w[1]), Math.floor(w[2])};
                    p[k * 5] = (float) (w[0] - anchor[0]);
                    p[k * 5 + 1] = (float) (w[1] - anchor[1]);
                    p[k * 5 + 2] = (float) (w[2] - anchor[2]);
                    p[k * 5 + 3] = q.xyzuv()[k * 5 + 3];
                    p[k * 5 + 4] = q.xyzuv()[k * 5 + 4];
                }
                world.add(new Quad(q.texture(), p, q.color(), q.light(), q.normal(), q.blended()));
            }
            for (var l : lines) {
                var e = new float[6];
                for (int k = 0; k < 2; k++) {
                    double[] w = {quantize(cam.x() + l.ends()[k * 3]), quantize(cam.y() + l.ends()[k * 3 + 1]), quantize(cam.z() + l.ends()[k * 3 + 2])};
                    if (anchor == null) anchor = new double[] {Math.floor(w[0]), Math.floor(w[1]), Math.floor(w[2])};
                    e[k * 3] = (float) (w[0] - anchor[0]);
                    e[k * 3 + 1] = (float) (w[1] - anchor[1]);
                    e[k * 3 + 2] = (float) (w[2] - anchor[2]);
                }
                worldLines.add(new Line(e, l.color(), l.width()));
            }
            if (anchor == null) continue;
            long id = contentKey(world, worldLines);
            out.meshes().putIfAbsent(id, new Mesh(id, world, worldLines));
            pieces.add(new Piece(id, pass, (byte) flags, anchor, new float[] {1, 0, 0, 0, 1, 0, 0, 0, 1}, new float[] {1, 1, 1, 1}));
        }
    }

    /** The draws made on the recording device, placed in the world. */
    private static void drawn(Cam cam, Matrix4fc viewRotation, Recording out, List<Piece> pieces) {
        for (var d : DEVICE.draws) {
            var pipeline = d.pipeline().pipeline();
            long id = meshKey(d);
            var mesh = DECODED.get(id);
            if (mesh == null) {
                mesh = decode(d, id);
                DECODED.put(id, mesh);
                MESH_BUFFERS.put(id, new long[] {d.vertexId(), d.indexId()});
            }
            if (mesh.quads().isEmpty() && mesh.lines().isEmpty()) continue;
            unsupported(pipeline);
            byte pass = classify(pipeline, d.stage());
            out.meshes().putIfAbsent(id, mesh);
            var t = d.transform();
            var c = t.colorModulator();
            pieces.add(placed(id, pass, flags(pipeline), toWorld(t, viewRotation, cam.x(), cam.y(), cam.z()), new float[] {c.x(), c.y(), c.z(), c.w()}));
        }
    }

    /** Applies the caps: pieces past the limits are dropped, once per pipeline location a warning. */
    private static List<Piece> capped(List<Piece> pieces, Map<Long, Mesh> meshes) {
        int quads = 0, lines = 0;
        var kept = new ArrayList<Piece>(pieces.size());
        for (var piece : pieces) {
            var mesh = meshes.get(piece.mesh());
            int q = mesh.quads().size(), l = mesh.lines().size();
            if (quads + q > MAX_QUADS || lines + l > MAX_LINES) {
                warnOnce("a world recording is over its limit of " + MAX_QUADS + " quads or " + MAX_LINES + " lines; a piece is dropped, mesh", Long.toHexString(piece.mesh()));
                continue;
            }
            quads += q;
            lines += l;
            kept.add(piece);
        }
        return kept;
    }

    // --- Whether any mod needs a recording

    /** Whether a loaded mod listens to a level-frame event; decided once when the mods have loaded. Without one nothing runs. */
    static volatile boolean needed;
    /** How many times `record` ran. */
    static int records;

    private static final List<Class<? extends Event>> EVENTS = List.of(ExtractLevelRenderStateEvent.class, SubmitCustomGeometryEvent.class, PrepareRenderBuffersEvent.class,
            RenderLevelStageEvent.AfterSky.class, RenderLevelStageEvent.AfterOpaqueBlocks.class, RenderLevelStageEvent.AfterOpaqueFeatures.class,
            RenderLevelStageEvent.AfterTranslucentFeatures.class, RenderLevelStageEvent.AfterTranslucentBlocks.class, RenderLevelStageEvent.AfterTranslucentParticles.class,
            RenderLevelStageEvent.AfterWeather.class, RenderLevelStageEvent.AfterLevel.class, RenderLevelStageEvent.OitTranslucent.class);

    /** Whether the bus has a listener for any of the frame's events. */
    static boolean listened(net.neoforged.bus.api.IEventBus bus) {
        return EVENTS.stream().anyMatch(event -> Listeners.listened(bus, event));
    }

    /** Called once the mods have loaded. */
    static void decide() {
        needed = listened(NeoForge.EVENT_BUS);
        System.err.println("Horizon: " + (needed ? "a mod listens to the level frame's events; the world is recorded" : "no mod listens to the level frame's events; the world is not recorded"));
    }

    // --- Taking a recording

    /** Whether the world is changing: set by a recording that differs from the one before, ended by two alike. */
    public static boolean animating;
    private static int ticksSinceFrame;

    /** `HudAnimation`'s rule: two alike end it only if real time passed. */
    static void noted(boolean identical, float realtimeDeltaTicks) {
        boolean was = animating;
        if (!identical) animating = true;
        else if (HudAnimation.ends(true, realtimeDeltaTicks)) animating = false;
        if (animating && !was) ticksSinceFrame = 0;
    }

    /** A window frame's recording is in step with the window: no tick-owed frame follows it. */
    static void framed() {
        ticksSinceFrame = 0;
    }

    /** After a tick's recording: no frame recording for two ticks means the window isn't serving them. */
    static void ticked() {
        if (animating && ++ticksSinceFrame > 2) animating = false;
    }

    private static boolean pendingMeshes;

    private static Object stateOf(Class<?> owner, String field, Object target) throws ReflectiveOperationException {
        var f = owner.getDeclaredField(field);
        f.setAccessible(true);
        return f.get(target);
    }

    private static java.lang.reflect.Method extractOptions;

    /** The camera, state objects and frustum the events carry, from the window's camera. */
    private static Frame frame(Minecraft minecraft, ClientLevel level, Cam cam, float partial, DeltaTracker delta, SubmitCapture capture) throws ReflectiveOperationException {
        var renderer = minecraft.gameRenderer;
        var camera = renderer.mainCamera();
        var access = (CameraAccess) camera;
        access.horizon$position(new Vec3(cam.x(), cam.y(), cam.z()));
        access.horizon$rotation(cam.yaw(), cam.pitch());
        access.horizon$perspective(0.05F, cam.far(), cam.fov(), width, height);
        access.horizon$fov(cam.fov());
        access.horizon$hudFov(70.0F);
        access.horizon$far(cam.far());
        access.horizon$initialized(true);
        var viewRotation = camera.getViewRotationMatrix(new Matrix4f());
        var projection = new Matrix4f().perspective(cam.fov() * (float) (Math.PI / 180.0), (float) width / height, 0.05F, cam.far(), true);
        var frustum = new Frustum(viewRotation, projection);
        frustum.prepare(cam.x(), cam.y(), cam.z());
        access.horizon$frustum(frustum);
        var game = renderer.gameRenderState();
        var state = game.levelRenderState;
        camera.extractRenderState(state.cameraRenderState, delta);
        state.cameraRenderState.fogType = camera.getFluidInCamera();
        state.gameTime = level.getGameTime();
        state.worldPartialTicks = partial;
        if (extractOptions == null) {
            extractOptions = net.minecraft.client.renderer.GameRenderer.class.getDeclaredMethod("extractOptions");
            extractOptions.setAccessible(true);
        }
        extractOptions.invoke(renderer);
        var windowState = game.windowRenderState;
        windowState.width = width;
        windowState.height = height;
        var options = game.optionsRenderState;
        var camState = state.cameraRenderState;
        boolean sky = level.dimensionType().skybox() != DimensionType.Skybox.NONE && camState.fogType != net.minecraft.world.level.material.FogType.POWDER_SNOW
                && camState.fogType != net.minecraft.world.level.material.FogType.LAVA && !camState.entityRenderState.doesMobEffectBlockSky;
        return new Frame(levelRenderer(minecraft), minecraft.levelExtractor, state, options, level, camera, frustum, delta, capture, viewRotation, sections(frustum),
                new RecordingRenderPass(DEVICE), sky, options.improvedTransparency && !state.renderWireframeTerrain);
    }

    private static LevelRenderer levelRenderer(Minecraft minecraft) throws ReflectiveOperationException {
        if (minecraft.levelRenderer == null) {
            var f = Minecraft.class.getDeclaredField("levelRenderer");
            f.setAccessible(true);
            f.set(minecraft, Sprites.unallocated(LevelRenderer.class));
        }
        return minecraft.levelRenderer;
    }

    /** The loaded columns' sections in the frustum, made as a mod walks them. */
    private static Iterable<IRenderableSection> sections(Frustum frustum) {
        return () -> {
            var level = Minecraft.getInstance().level;
            var columns = ClientBridge.columns().iterator();
            int min = level.getMinSectionY(), max = level.getMaxSectionY();
            return new Iterator<>() {
                long column;
                int y = max;
                IRenderableSection next = advance();

                private IRenderableSection advance() {
                    while (true) {
                        if (y >= max) {
                            if (!columns.hasNext()) return null;
                            column = columns.nextLong();
                            y = min;
                        }
                        int cy = y++;
                        int x = net.minecraft.world.level.ChunkPos.getX(column) << 4, z = net.minecraft.world.level.ChunkPos.getZ(column) << 4;
                        var box = new AABB(x, cy << 4, z, x + 16, (cy << 4) + 16, z + 16);
                        if (frustum.isVisible(box)) {
                            var origin = new BlockPos(x, cy << 4, z);
                            return new IRenderableSection() {
                                @Override
                                public BlockPos getRenderOrigin() {
                                    return origin;
                                }

                                @Override
                                public AABB getBoundingBox() {
                                    return box;
                                }

                                @Override
                                public boolean isEmpty() {
                                    return false;
                                }
                            };
                        }
                    }
                }

                @Override
                public boolean hasNext() {
                    return next != null;
                }

                @Override
                public IRenderableSection next() {
                    var out = next;
                    next = advance();
                    return out;
                }
            };
        };
    }

    /** `LevelRenderer.render`'s events for one frame at the camera, taken into world-space pieces. Null when no level is up. */
    static Recording record(Cam cam, float partial, DeltaTracker delta, int windowWidth, int windowHeight) throws ReflectiveOperationException {
        records++;
        Minecraft minecraft = Minecraft.getInstance();
        ClientLevel level = minecraft.level;
        if (level == null || minecraft.player == null || cam == null) return null;
        width = Math.max(1, windowWidth);
        height = Math.max(1, windowHeight);
        var capture = new SubmitCapture(true);
        DEVICE.draws.clear();
        recording = true;
        var stack = RenderSystem.getModelViewStack();
        stack.pushMatrix();
        try {
            var frame = frame(minecraft, level, cam, partial, delta, capture);
            stack.mul(frame.viewRotation());
            fire(frame, WorldFrame::safely);
            var out = new Recording(new ArrayList<>(), new LinkedHashMap<>());
            var pieces = new ArrayList<Piece>();
            submitted(capture, cam, out, pieces);
            drawn(cam, frame.viewRotation(), out, pieces);
            return new Recording(capped(pieces, out.meshes()), out.meshes());
        } finally {
            stack.popMatrix();
            recording = false;
            stage = SUBMIT;
            DEVICE.draws.clear();
        }
    }

    // --- Wire

    /** Bytes of the pieces alone: what two recordings are compared by. */
    static byte[] bytes(List<Piece> pieces) throws IOException {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(pieces.size());
        for (var p : pieces) {
            out.writeLong(p.mesh());
            out.writeByte(p.pass());
            out.writeByte(p.flags());
            for (double v : p.origin()) out.writeDouble(v);
            for (float v : p.linear()) out.writeFloat(v);
            for (float v : p.modulator()) out.writeFloat(v);
        }
        return bytes.toByteArray();
    }

    private static long cost(Mesh mesh) {
        return mesh.quads().size() * 180L + mesh.lines().size() * 32L;
    }

    private static void write(DataOutputStream out, Mesh mesh) throws IOException {
        out.writeLong(mesh.id());
        out.writeInt(mesh.quads().size());
        for (var q : mesh.quads()) {
            byte[] name = q.texture().getBytes(java.nio.charset.StandardCharsets.UTF_8);
            out.writeInt(name.length);
            out.write(name);
            for (float v : q.xyzuv()) out.writeFloat(v);
            for (int v : q.color()) out.writeInt(v);
            for (int v : q.light()) out.writeInt(v);
            out.writeBoolean(q.normal() != null);
            if (q.normal() != null) for (float v : q.normal()) out.writeFloat(v);
        }
        out.writeInt(mesh.lines().size());
        for (var l : mesh.lines()) {
            for (float v : l.ends()) out.writeFloat(v);
            out.writeInt(l.color());
            out.writeFloat(l.width());
        }
    }

    /** The blob for a recording that differs from the last sent; null when it equals it and nothing is owed. Meshes Rust has and no longer needs are dropped, new ones sent up to the tick's budget. */
    static byte[] blob(Recording recording) throws IOException {
        byte[] pieces = bytes(recording.pieces());
        boolean same = Arrays.equals(pieces, lastPieces);
        // Buffers closed since: what was decoded from them is stale.
        for (long closed : DEVICE.closed) {
            for (var it = MESH_BUFFERS.entrySet().iterator(); it.hasNext(); ) {
                var e = it.next();
                if (e.getValue()[0] == closed || e.getValue()[1] == closed) {
                    DECODED.remove(e.getKey());
                    it.remove();
                }
            }
        }
        DEVICE.closed.clear();
        if (same && !pendingMeshes) return null;
        var referenced = new LinkedHashSet<Long>();
        for (var p : recording.pieces()) referenced.add(p.mesh());
        var dropped = new ArrayList<Long>();
        for (var it = SENT.iterator(); it.hasNext(); ) {
            long id = it.next();
            if (!referenced.contains(id)) {
                dropped.add(id);
                it.remove();
            }
        }
        var fresh = new ArrayList<Mesh>();
        long spent = 0;
        pendingMeshes = false;
        for (long id : referenced) {
            if (SENT.contains(id)) continue;
            var mesh = recording.meshes().get(id);
            if (spent > 0 && spent + cost(mesh) > MAX_NEW_BYTES) {
                pendingMeshes = true;
                continue;
            }
            spent += cost(mesh);
            fresh.add(mesh);
            SENT.add(id);
        }
        // A piece whose mesh waits for the next tick is held back with it.
        var visible = new ArrayList<Piece>();
        for (var p : recording.pieces()) if (SENT.contains(p.mesh())) visible.add(p);
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(++generation);
        out.writeInt(dropped.size());
        for (long id : dropped) out.writeLong(id);
        out.writeInt(fresh.size());
        for (var mesh : fresh) write(out, mesh);
        out.write(bytes(visible));
        lastPieces = pendingMeshes ? new byte[0] : pieces;
        return bytes.toByteArray();
    }

    /** What a fresh world or a fresh join starts from. */
    static void forget() {
        DECODED.clear();
        MESH_BUFFERS.clear();
        SENT.clear();
        lastPieces = new byte[0];
        pendingMeshes = false;
        animating = false;
        ticksSinceFrame = 0;
        camera = null;
    }

    /** The latest camera the window told. */
    static Cam camera() {
        return camera;
    }

    static int windowWidth() {
        return width;
    }

    static int windowHeight() {
        return height;
    }

    /** A recording of the world for one tick or one frame: its blob if it changed, with the animation bookkeeping. */
    static byte[] take(Cam cam, float partial, DeltaTracker delta, int windowWidth, int windowHeight, boolean frame) {
        if (!needed) return null;
        try {
            var recording = record(cam, partial, delta, windowWidth, windowHeight);
            if (recording == null) return null;
            var blob = blob(recording);
            noted(blob == null, delta.getRealtimeDeltaTicks());
            if (frame) framed();
            else ticked();
            return blob;
        } catch (Throwable t) {
            System.err.println("Horizon: the world recording failed");
            t.printStackTrace();
            return null;
        }
    }
}
