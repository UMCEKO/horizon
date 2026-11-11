package dev.horizonlab.javamods.boot;

import com.google.gson.stream.JsonWriter;
import com.mojang.serialization.JsonOps;
import dev.horizonlab.facts.FactsWriter;
import java.io.IOException;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.Item;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.storage.loot.LootTable;

/** What the mods registered, as the entries added after vanilla's bootstrap, and the facts Horizon's tables are built from. */
public final class BootFacts {
    private BootFacts() {}

    private static final List<Registry<?>> REGISTRIES = List.of(BuiltInRegistries.BLOCK, BuiltInRegistries.ITEM, BuiltInRegistries.BLOCK_ENTITY_TYPE, BuiltInRegistries.DATA_COMPONENT_TYPE, BuiltInRegistries.FLUID);
    private static final String[] CLIENT_MAPPERS = {
        "net.minecraft.client.renderer.item.ItemModels",
        "net.minecraft.client.renderer.special.SpecialModelRenderers",
        "net.minecraft.client.color.item.ItemTintSources",
        "net.minecraft.client.renderer.item.properties.select.SelectItemModelProperties",
        "net.minecraft.client.renderer.item.properties.conditional.ConditionalItemModelProperties",
        "net.minecraft.client.renderer.item.properties.numeric.RangeSelectItemModelProperties",
    };
    private static final Map<Registry<?>, Map<Identifier, Object>> VANILLA = new IdentityHashMap<>();
    private static final Map<Registry<?>, Set<Identifier>> OWNED_IDS = new IdentityHashMap<>();
    private static final Set<Object> OWNED = Collections.newSetFromMap(new IdentityHashMap<>());
    private static final Map<Class<?>, Set<Object>> VANILLA_MAPPED = new HashMap<>();
    private static boolean sealed;

    private static boolean client;
    private static String mapperFailure;

    /** Vanilla's entries, once its bootstrap ran and before any mod is constructed. */
    static void snapshot(boolean client) {
        BootFacts.client = client;
        for (Registry<?> registry : REGISTRIES) {
            Map<Identifier, Object> entries = new HashMap<>();
            for (Identifier id : registry.keySet()) entries.put(id, registry.getValue(id));
            VANILLA.put(registry, entries);
        }
    }

    /** A client id mapper's entries as its bootstrap put them, before the mods' registration event (`MapperBootstrapMixin`). */
    public static void snapshotMapper(Class<?> holder) {
        // Runs inside vanilla's bootstrap on every boot: a failure here is only reported by the facts export.
        try {
            Set<Object> values = Collections.newSetFromMap(new IdentityHashMap<>());
            values.addAll(mapped(holder).values());
            VANILLA_MAPPED.putIfAbsent(holder, values);
        } catch (Throwable e) {
            if (mapperFailure == null) mapperFailure = holder.getName() + ": " + e;
        }
    }

    @SuppressWarnings("unchecked")
    private static Map<Object, Object> mapped(Class<?> holder) throws ReflectiveOperationException {
        var mapper = holder.getDeclaredField("ID_MAPPER");
        mapper.setAccessible(true);
        var ids = net.minecraft.util.ExtraCodecs.LateBoundIdMapper.class.getDeclaredField("idToValue");
        ids.setAccessible(true);
        return (Map<Object, Object>) ids.get(mapper.get(null));
    }

    /** The mods' entries: added since the snapshot, or put in place of a vanilla one. */
    static void seal() {
        if (VANILLA.isEmpty()) throw new IllegalStateException("no snapshot of vanilla's registries was taken");
        for (Registry<?> registry : REGISTRIES) {
            Map<Identifier, Object> vanilla = VANILLA.get(registry);
            Set<Identifier> ids = new HashSet<>();
            for (Identifier id : registry.keySet()) {
                Object value = registry.getValue(id);
                if (vanilla.get(id) != value) {
                    ids.add(id);
                    OWNED.add(value);
                }
            }
            OWNED_IDS.put(registry, ids);
        }
        sealed = true;
        System.err.println("Horizon: the mods registered " + OWNED_IDS.get(BuiltInRegistries.BLOCK).size() + " blocks, " + OWNED_IDS.get(BuiltInRegistries.ITEM).size()
                + " items and " + OWNED_IDS.get(BuiltInRegistries.BLOCK_ENTITY_TYPE).size() + " block entity types");
    }

    private static void check() {
        if (!sealed) throw new IllegalStateException("asked what the mods own before their registration ended");
    }

    static boolean ownsBlock(Block block) {
        check();
        return OWNED.contains(block);
    }

    static boolean ownsItem(Item item) {
        check();
        return OWNED.contains(item);
    }

    static boolean ownsBlockEntityType(BlockEntityType<?> type) {
        check();
        return OWNED.contains(type);
    }

    static boolean owns(Registry<?> registry, Identifier id) {
        check();
        Set<Identifier> ids = OWNED_IDS.get(registry);
        if (ids == null) throw new IllegalArgumentException("no ownership kept for " + registry.key());
        return ids.contains(id);
    }

    /** Whether vanilla's client bootstrap put this value in the holder's id mapper. */
    static boolean vanillaMapped(Class<?> holder, Object value) {
        Set<Object> values = VANILLA_MAPPED.get(holder);
        if (values == null) throw new IllegalArgumentException("no snapshot of " + holder.getName());
        return values.contains(value);
    }

    /** Writes the facts to `path`, once the registries and data packs are loaded. */
    static String export(String path) throws IOException {
        check();
        if (mapperFailure != null) throw new IllegalStateException("vanilla's item model mappers couldn't be read: " + mapperFailure);
        if (client && VANILLA_MAPPED.size() != CLIENT_MAPPERS.length) throw new IllegalStateException("vanilla's item model mappers weren't all seen before the mods' events: " + VANILLA_MAPPED.keySet());
        var registries = GameBridge.registries();
        if (registries == null) throw new IllegalStateException("the facts need the loaded registries");
        var ops = registries.createSerializationContext(JsonOps.INSTANCE);
        var loot = GameBridge.server().reloadableRegistries();
        var scope = new FactsWriter.Scope() {
            public boolean mod(Registry<?> registry, Identifier id) {
                return owns(registry, id);
            }

            public boolean vanilla(Registry<?> registry, Identifier id) {
                return !owns(registry, id);
            }

            public boolean wholeTag(Identifier tag) {
                return true;
            }
        };
        Path out = Path.of(path);
        System.setProperty("horizon.facts", out.toString());
        Path partial = out.resolveSibling(out.getFileName() + ".partial");
        try (Writer w = Files.newBufferedWriter(partial, StandardCharsets.UTF_8); JsonWriter json = new JsonWriter(w)) {
            new FactsWriter(json, ops, scope) {
                @Override
                protected void blockExtras(Block block) throws IOException {
                    var key = block.getLootTable();
                    if (key.isEmpty()) return;
                    LootTable table = loot.getLootTable(key.get());
                    if (table == LootTable.EMPTY) return;
                    var encoded = LootTable.DIRECT_CODEC.encodeStart(ops, table);
                    json.name("loot_table_id").value(key.get().identifier().toString());
                    if (encoded.result().isPresent()) {
                        json.name("loot_table");
                        json.jsonValue(encoded.result().get().toString());
                    } else {
                        json.name("loot_table_error").value(encoded.error().map(e -> e.message()).orElse(""));
                    }
                }

                void run() throws IOException {
                    json.beginObject();
                    json.name("schema").value(4);
                    json.name("minecraft").value(net.minecraft.SharedConstants.getCurrentVersion().name());
                    owned();
                    blocks();
                    fluids();
                    vanillaItems();
                    items();
                    dataComponentTypes();
                    tags();
                    json.endObject();
                }
            }.run();
        }
        Files.move(partial, out, java.nio.file.StandardCopyOption.REPLACE_EXISTING, java.nio.file.StandardCopyOption.ATOMIC_MOVE);
        return "ok";
    }
}
