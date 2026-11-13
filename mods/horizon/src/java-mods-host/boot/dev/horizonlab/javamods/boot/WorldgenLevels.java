package dev.horizonlab.javamods.boot;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import net.minecraft.core.registries.Registries;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ChunkMap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.Util;
import net.minecraft.util.thread.BlockableEventLoop;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.StructureManager;
import net.minecraft.world.level.TicketStorage;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.NoiseRouter;
import net.minecraft.world.level.levelgen.RandomState;
import net.minecraft.world.level.levelgen.structure.StructureCheck;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplateManager;
import net.minecraft.world.level.storage.LevelStorageSource;

/** Each level's generation objects, built at world start as `ServerLevel.<init>` and `ChunkMap.<init>` build them, so mods' patches of that construction run. */
final class WorldgenLevels {
    private WorldgenLevels() {}

    /** A level whose generation context threw: the world doesn't open. */
    static final class Failed extends Exception {
        Failed(String message, Throwable cause) {
            super(message, cause);
        }
    }

    /** A level's view and its real `ChunkMap`; the view keeps its own map. */
    record Context(ServerLevel level, ChunkMap map) {}

    /** Each level's context by dimension id, for the `wg_*` ops. */
    private static final Map<String, Context> MAPS = new ConcurrentHashMap<>();
    /** Scratch storage the real maps derive paths from; native saves, nothing is written through it. */
    private static LevelStorageSource.LevelStorageAccess storage;
    /** The template manager built for the open world, when the server had none. */
    private static StructureTemplateManager built;

    static Context of(String level) {
        return MAPS.get(level);
    }

    /** `server_about_to_start`, after `ServerAboutToStart`: every level's generator, random state, structure state and structure check. */
    static void open(MinecraftServer server, List<ServerLevel> levels, Path worldFolder) throws Failed {
        close();
        try {
            storage = LevelStorageSource.createDefault(worldFolder.resolve("horizon_java")).createAccess("horizon_java_gen");
        } catch (IOException e) {
            throw failed("the scratch save", e);
        }
        StructureTemplateManager templates;
        try {
            templates = templates(server);
        } catch (Throwable t) {
            throw failed("the structure templates", t);
        }
        for (ServerLevel level : levels) {
            String id = level.dimension().identifier().toString();
            try {
                build(server, level, templates);
            } catch (Throwable t) {
                throw failed(id, t);
            }
        }
    }

    private static Failed failed(String what, Throwable t) {
        String message = "Horizon: Java worldgen context for " + what + " failed: " + t;
        System.err.println(message);
        t.printStackTrace();
        return new Failed(message, t);
    }

    /** The server's `StructureTemplateManager`, or one made as `MinecraftServer.<init>` makes it. */
    private static StructureTemplateManager templates(MinecraftServer server) throws ReflectiveOperationException {
        var current = server.getStructureTemplateManager();
        if (current != null && current != built) return current;
        var blocks = server.registryAccess().lookupOrThrow(Registries.BLOCK).filterFeatures(server.getWorldData().enabledFeatures());
        built = new StructureTemplateManager(server.getResourceManager(), storage, server.getFixerUpper(), blocks);
        ServerView.set(MinecraftServer.class, server, "structureTemplateManager", built);
        return built;
    }

    @SuppressWarnings("unchecked")
    private static void build(MinecraftServer server, ServerLevel level, StructureTemplateManager templates) throws ReflectiveOperationException {
        String id = level.dimension().identifier().toString();
        var stem = server.registryAccess().lookupOrThrow(Registries.LEVEL_STEM).getValue(Registries.levelToLevelStem(level.dimension()));
        if (stem == null) throw new IllegalStateException("no level stem " + id);
        var generator = stem.generator();
        var chunks = level.getChunkSource();
        var mainThread = (BlockableEventLoop<Runnable>) (BlockableEventLoop<?>) server;
        var real = new ChunkMap(level, storage, server.getFixerUpper(), templates, Util.backgroundExecutor(), mainThread, chunks, generator,
                (pos, status) -> {}, new TicketStorage(), server.getPlayerList().getViewDistance(), false);
        // The view's own map stays: `Tracking` and `EntityViews` hold it.
        for (String field : List.of("randomState", "chunkGeneratorState", "worldGenContext")) {
            ServerView.set(ChunkMap.class, chunks.chunkMap, field, get(ChunkMap.class, real, field));
        }
        chunks.getGeneratorState().ensureStructuresGenerated();
        var options = server.getWorldGenSettings().options();
        var check = new StructureCheck(new NativeChunkScan(id), level.registryAccess(), templates, level.dimension(), generator, chunks.randomState(), level,
                generator.getBiomeSource(), options.seed(), server.getFixerUpper());
        ServerView.set(ServerLevel.class, level, "structureCheck", check);
        ServerView.set(ServerLevel.class, level, "structureManager", new StructureManager(level, options, check));
        MAPS.put(id, new Context(level, real));
    }

    private static Object get(Class<?> owner, Object target, String field) throws ReflectiveOperationException {
        Field f = owner.getDeclaredField(field);
        f.setAccessible(true);
        return f.get(target);
    }

    /** The world closed: its maps go, and the scratch save's lock is let go. */
    static void close() {
        MAPS.clear();
        if (storage == null) return;
        try {
            storage.close();
        } catch (IOException e) {
            System.err.println("Horizon: closing the Java worldgen scratch save: " + e);
        }
        storage = null;
    }

    /** `wg_probe`: a level id and block points; the generator's classes, then each point's `WORLD_SURFACE_WG` height and noise biome. */
    static byte[] probe(byte[] input) throws IOException {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        String id = text(in);
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        var context = MAPS.get(id);
        if (context == null) {
            out.writeByte(1);
            text(out, "no Java generation context for " + id);
            return bytes.toByteArray();
        }
        var level = context.level();
        var chunks = level.getChunkSource();
        var generator = chunks.getGenerator();
        RandomState random = chunks.randomState();
        NoiseRouter router;
        try {
            router = (NoiseRouter) get(RandomState.class, random, "router");
        } catch (ReflectiveOperationException e) {
            throw new IOException(e);
        }
        var type = level.dimensionType();
        var heights = LevelHeightAccessor.create(type.minY(), type.height());
        var biomes = generator.getBiomeSource().createUncachedResolver(random);
        out.writeByte(0);
        text(out, generator.getClass().getName());
        text(out, generator.getBiomeSource().getClass().getName());
        text(out, router.finalDensity().getClass().getName());
        int count = in.readInt();
        for (int i = 0; i < count; i++) {
            int x = in.readInt();
            int y = in.readInt();
            int z = in.readInt();
            out.writeInt(generator.getBaseHeight(x, z, Heightmap.Types.WORLD_SURFACE_WG, heights, random));
            var biome = biomes.getNoiseBiome(x >> 2, y >> 2, z >> 2);
            text(out, biome.unwrapKey().map(key -> key.identifier().toString()).orElse("?"));
        }
        return bytes.toByteArray();
    }

    private static String text(DataInputStream in) throws IOException {
        return new String(in.readNBytes(in.readInt()), StandardCharsets.UTF_8);
    }

    private static void text(DataOutputStream out, String value) throws IOException {
        byte[] bytes = value.getBytes(StandardCharsets.UTF_8);
        out.writeInt(bytes.length);
        out.write(bytes);
    }
}
