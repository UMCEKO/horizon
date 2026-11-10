package dev.horizonlab.javamods.parity;

import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HexFormat;
import java.util.List;
import java.util.Map;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.LevelChunkSection;
import net.minecraft.world.level.chunk.ProtoChunk;
import net.minecraft.world.level.chunk.UpgradeData;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.blending.Blender;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructurePiece;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.server.ServerStartedEvent;

/** Dev-only: dumps what the real loader generates (tools/worldgen_compare.py), then halts the server. */
@Mod("horizon_wgdump")
public final class WorldgenDump {
    private static final int[] BIOME_Y = {-32, 64, 160};
    private static final Heightmap.Types[] HEIGHTMAPS = {
        Heightmap.Types.WORLD_SURFACE,
        Heightmap.Types.OCEAN_FLOOR,
        Heightmap.Types.MOTION_BLOCKING,
        Heightmap.Types.MOTION_BLOCKING_NO_LEAVES
    };

    public WorldgenDump(IEventBus modBus) {
        NeoForge.EVENT_BUS.addListener(this::started);
    }

    private void started(ServerStartedEvent event) {
        MinecraftServer server = event.getServer();
        try {
            Path request = Path.of(System.getProperty("horizon.wg.request"));
            Path out = Path.of(System.getProperty("horizon.wg.out"));
            JsonObject req = new Gson().fromJson(Files.readString(request), JsonObject.class);
            Files.writeString(out, new Gson().toJson(dump(server, req)), StandardCharsets.UTF_8);
            System.out.println("horizon-wgdump: wrote " + out);
        } catch (Throwable e) {
            e.printStackTrace();
            System.err.flush();
            Runtime.getRuntime().halt(3);
        }
        server.halt(false);
    }

    private static int num(JsonObject req, String key, int fallback) {
        return req.has(key) ? req.get(key).getAsInt() : fallback;
    }

    /** The first chunk coordinate of a square of `side` chunks centred on 0,0. */
    private static int low(int side) {
        return -(side / 2);
    }

    private static JsonObject dump(MinecraftServer server, JsonObject req) throws Exception {
        List<String> ids = new ArrayList<>();
        if (req.has("levels")) {
            req.getAsJsonArray("levels").forEach(e -> ids.add(e.getAsString()));
        } else {
            ids.add("minecraft:overworld");
        }
        JsonObject root = new JsonObject();
        root.addProperty("seed", server.getWorldGenSettings().options().seed());
        JsonArray stems = new JsonArray();
        for (ServerLevel level : server.getAllLevels()) {
            JsonObject stem = new JsonObject();
            stem.addProperty("id", level.dimension().identifier().toString());
            stem.addProperty("generator", level.getChunkSource().getGenerator().getClass().getName());
            stem.addProperty(
                    "biome_source",
                    level.getChunkSource().getGenerator().getBiomeSource().getClass().getName());
            stems.add(stem);
        }
        root.add("levels", stems);
        JsonObject dimensions = new JsonObject();
        for (String id : ids) {
            ServerLevel level = server.getLevel(ResourceKey.create(Registries.DIMENSION, Identifier.parse(id)));
            if (level == null) {
                continue;
            }
            JsonObject one = new JsonObject();
            one.add("biomes", biomes(level, num(req, "square", 2048), num(req, "step", 4)));
            one.add("starts", starts(server, level, num(req, "starts", 64)));
            one.add("chunks", chunks(server, level, num(req, "chunks", 5)));
            dimensions.add(id, one);
        }
        root.add("dimensions", dimensions);
        return root;
    }

    private static JsonArray biomes(ServerLevel level, int squareBlocks, int step) {
        int side = squareBlocks / 16;
        int lo = low(side);
        JsonArray out = new JsonArray();
        var cache = level.getChunkSource();
        for (int cz = lo; cz < lo + side; cz += step) {
            for (int cx = lo; cx < lo + side; cx += step) {
                ProtoChunk chunk = new ProtoChunk(
                        new ChunkPos(cx, cz), UpgradeData.EMPTY, level, level.palettedContainerFactory(), null);
                cache.getGenerator()
                        .createBiomes(cache.randomState(), Blender.empty(), level.structureManager(), chunk)
                        .join();
                chunk.setPersistedStatus(ChunkStatus.BIOMES);
                for (int y : BIOME_Y) {
                    var holder = chunk.getNoiseBiome(cx * 4, y >> 2, cz * 4);
                    JsonArray row = new JsonArray();
                    row.add(cx * 16);
                    row.add(y);
                    row.add(cz * 16);
                    row.add(holder.unwrapKey().map(k -> k.identifier().toString()).orElse("?"));
                    out.add(row);
                }
            }
        }
        return out;
    }

    private static JsonArray box(BoundingBox b) {
        JsonArray a = new JsonArray();
        a.add(b.minX());
        a.add(b.minY());
        a.add(b.minZ());
        a.add(b.maxX());
        a.add(b.maxY());
        a.add(b.maxZ());
        return a;
    }

    private static JsonArray starts(MinecraftServer server, ServerLevel level, int side) {
        int lo = low(side);
        var cache = level.getChunkSource();
        Registry<Structure> structures = level.registryAccess().lookupOrThrow(Registries.STRUCTURE);
        List<JsonObject> found = new ArrayList<>();
        for (int cz = lo; cz < lo + side; cz++) {
            for (int cx = lo; cx < lo + side; cx++) {
                ProtoChunk chunk = new ProtoChunk(
                        new ChunkPos(cx, cz), UpgradeData.EMPTY, level, level.palettedContainerFactory(), null);
                cache.getGenerator()
                        .createStructures(
                                level.registryAccess(),
                                cache.getGeneratorState(),
                                level.structureManager(),
                                chunk,
                                server.getStructureTemplateManager(),
                                level.dimension());
                for (Map.Entry<Structure, StructureStart> e : chunk.getAllStarts().entrySet()) {
                    StructureStart start = e.getValue();
                    if (!start.isValid()) {
                        continue;
                    }
                    JsonObject o = new JsonObject();
                    o.addProperty("structure", String.valueOf(structures.getKey(e.getKey())));
                    JsonArray at = new JsonArray();
                    at.add(cx);
                    at.add(cz);
                    o.add("chunk", at);
                    o.add("box", box(start.getBoundingBox()));
                    JsonArray pieces = new JsonArray();
                    for (StructurePiece piece : start.getPieces()) {
                        JsonArray p = new JsonArray();
                        p.add(String.valueOf(BuiltInRegistries.STRUCTURE_PIECE.getKey(piece.getType())));
                        p.add(box(piece.getBoundingBox()));
                        pieces.add(p);
                    }
                    o.add("pieces", pieces);
                    found.add(o);
                }
            }
        }
        found.sort(Comparator.comparing((JsonObject o) -> o.get("structure").getAsString())
                .thenComparing(o -> o.get("chunk").toString()));
        JsonArray out = new JsonArray();
        found.forEach(out::add);
        return out;
    }

    private static <T extends Comparable<T>> String value(BlockState state, Property<T> property) {
        return property.getName(state.getValue(property));
    }

    private static String text(BlockState state) {
        StringBuilder s = new StringBuilder(String.valueOf(BuiltInRegistries.BLOCK.getKey(state.getBlock())));
        List<Property<?>> properties = new ArrayList<>(state.getProperties());
        if (!properties.isEmpty()) {
            properties.sort(Comparator.comparing(Property::getName));
            s.append('[');
            for (int i = 0; i < properties.size(); i++) {
                s.append(i == 0 ? "" : ",")
                        .append(properties.get(i).getName())
                        .append('=')
                        .append(value(state, properties.get(i)));
            }
            s.append(']');
        }
        return s.toString();
    }

    private static JsonObject hash(LevelChunk chunk) throws Exception {
        JsonObject o = new JsonObject();
        o.addProperty("x", chunk.getPos().x());
        o.addProperty("z", chunk.getPos().z());
        JsonArray sections = new JsonArray();
        for (LevelChunkSection section : chunk.getSections()) {
            MessageDigest sha = MessageDigest.getInstance("SHA-1");
            for (int i = 0; i < 4096; i++) {
                BlockState state = section.getBlockState(i & 15, i >> 8, (i >> 4) & 15);
                sha.update((text(state) + "\n").getBytes(StandardCharsets.UTF_8));
            }
            sections.add(HexFormat.of().formatHex(sha.digest()));
        }
        o.add("sections", sections);
        JsonObject heightmaps = new JsonObject();
        for (Heightmap.Types type : HEIGHTMAPS) {
            JsonArray a = new JsonArray();
            for (int i = 0; i < 256; i++) {
                a.add(chunk.getHeight(type, i & 15, i >> 4) + 1); // one above the highest block, as the game side
            }
            heightmaps.add(type.name(), a);
        }
        o.add("heightmaps", heightmaps);
        List<JsonArray> entities = new ArrayList<>();
        for (BlockPos pos : chunk.getBlockEntitiesPos()) {
            BlockEntity be = chunk.getBlockEntity(pos);
            if (be == null) {
                continue;
            }
            JsonArray a = new JsonArray();
            a.add(pos.getX());
            a.add(pos.getY());
            a.add(pos.getZ());
            a.add(String.valueOf(BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(be.getType())));
            entities.add(a);
        }
        entities.sort(Comparator.comparing(JsonArray::toString));
        JsonArray list = new JsonArray();
        entities.forEach(list::add);
        o.add("block_entities", list);
        return o;
    }

    private static JsonArray chunks(MinecraftServer server, ServerLevel level, int side) throws Exception {
        int lo = low(side);
        server.getCommands()
                .performPrefixedCommand(
                        server.createCommandSourceStack().withLevel(level).withSuppressedOutput(),
                        "forceload add " + lo * 16 + " " + lo * 16 + " " + (lo + side) * 16 + " " + (lo + side) * 16);
        for (int cz = lo; cz < lo + side; cz++) {
            for (int cx = lo; cx < lo + side; cx++) {
                level.getChunk(cx, cz, ChunkStatus.FULL, true);
            }
        }
        JsonArray out = new JsonArray();
        for (int cz = -1; cz <= 1; cz++) {
            for (int cx = -1; cx <= 1; cx++) {
                out.add(hash((LevelChunk) level.getChunk(cx, cz, ChunkStatus.FULL, true)));
            }
        }
        return out;
    }
}
