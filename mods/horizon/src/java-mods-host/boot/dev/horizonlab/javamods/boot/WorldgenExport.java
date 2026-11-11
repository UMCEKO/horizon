package dev.horizonlab.javamods.boot;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.mojang.serialization.Codec;
import com.mojang.serialization.JsonOps;
import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.Collections;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.TreeMap;
import java.util.TreeSet;
import net.minecraft.commands.Commands;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderSet;
import net.minecraft.core.Registry;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryDataLoader;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.WorldLoader;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.permissions.LevelBasedPermissionSet;
import net.minecraft.util.Unit;
import net.minecraft.util.Util;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.levelgen.NoiseGeneratorSettings;
import net.minecraft.world.level.levelgen.material.rule.MaterialRule;
import net.minecraft.world.level.levelgen.presets.WorldPresets;

/** The final worldgen registries, as the real loader and the mods' modifiers left them, written for native: what differs from vanilla's, and the mod-owned types inside each entry. */
public final class WorldgenExport {
    private WorldgenExport() {}

    private static final int DEPTH = 64;
    private static final List<RegistryDataLoader.RegistryData<?>> LOADED = java.util.stream.Stream.concat(RegistryDataLoader.WORLD_REGISTRIES.stream(), RegistryDataLoader.DIMENSION_REGISTRIES.stream()).toList();
    private static RegistryAccess.Frozen vanilla;

    /** A value's type, as its type registry names it: the interface, the registry, and the method that gives the registry's value. */
    private record Typed(Class<?> type, Registry<?> registry, String method) {}

    private static final List<Typed> TYPED = List.of(
            typed("net.minecraft.world.level.levelgen.feature.Feature", BuiltInRegistries.FEATURE_TYPE, "codec"),
            typed("net.minecraft.world.level.levelgen.placement.PlacementModifier", BuiltInRegistries.PLACEMENT_MODIFIER_TYPE, "codec"),
            typed("net.minecraft.world.level.levelgen.feature.stateproviders.BlockStateProvider", BuiltInRegistries.BLOCK_STATE_PROVIDER_TYPE, "codec"),
            typed("net.minecraft.world.level.levelgen.feature.trunkplacers.TrunkPlacer", BuiltInRegistries.TRUNK_PLACER_TYPE, "type"),
            typed("net.minecraft.world.level.levelgen.feature.foliageplacers.FoliagePlacer", BuiltInRegistries.FOLIAGE_PLACER_TYPE, "type"),
            typed("net.minecraft.world.level.levelgen.feature.rootplacers.RootPlacer", BuiltInRegistries.ROOT_PLACER_TYPE, "type"),
            typed("net.minecraft.world.level.levelgen.feature.treedecorators.TreeDecorator", BuiltInRegistries.TREE_DECORATOR_TYPE, "type"),
            typed("net.minecraft.world.level.levelgen.feature.featuresize.FeatureSize", BuiltInRegistries.FEATURE_SIZE_TYPE, "type"),
            typed("net.minecraft.world.level.levelgen.blockpredicates.BlockPredicate", BuiltInRegistries.BLOCK_PREDICATE_TYPE, "type"),
            typed("net.minecraft.world.level.levelgen.densityfunction.DensityFunction", BuiltInRegistries.DENSITY_FUNCTION_TYPE, "codec"),
            typed("net.minecraft.world.level.levelgen.material.rule.MaterialRule", BuiltInRegistries.MATERIAL_RULE_TYPE, "codec"),
            typed("net.minecraft.world.level.levelgen.material.condition.MaterialCondition", BuiltInRegistries.MATERIAL_CONDITION_TYPE, "codec"),
            typed("net.minecraft.world.level.biome.BiomeSource", BuiltInRegistries.BIOME_SOURCE, "codec"),
            typed("net.minecraft.world.level.chunk.ChunkGenerator", BuiltInRegistries.CHUNK_GENERATOR, "codec"),
            typed("net.minecraft.world.level.levelgen.structure.Structure", BuiltInRegistries.STRUCTURE_TYPE, "type"),
            typed("net.minecraft.world.level.levelgen.structure.pools.StructurePoolElement", BuiltInRegistries.STRUCTURE_POOL_ELEMENT, "getType"),
            typed("net.minecraft.world.level.levelgen.structure.templatesystem.StructureProcessor", BuiltInRegistries.STRUCTURE_PROCESSOR, "codec"),
            typed("net.minecraft.world.level.levelgen.structure.pools.alias.PoolAliasBinding", BuiltInRegistries.POOL_ALIAS_BINDING_TYPE, "codec"),
            typed("net.minecraft.world.level.levelgen.structure.placement.StructurePlacement", BuiltInRegistries.STRUCTURE_PLACEMENT, "codec"),
            typed("net.minecraft.world.level.levelgen.carver.WorldCarver", BuiltInRegistries.CARVER_TYPE, "codec"),
            typed("net.minecraft.util.valueproviders.IntProvider", BuiltInRegistries.INT_PROVIDER_TYPE, "codec"),
            typed("net.minecraft.util.valueproviders.FloatProvider", BuiltInRegistries.FLOAT_PROVIDER_TYPE, "codec"),
            typed("net.minecraft.world.level.levelgen.heightproviders.HeightProvider", BuiltInRegistries.HEIGHT_PROVIDER_TYPE, "getType"),
            typed("net.minecraft.world.level.levelgen.structure.templatesystem.RuleTest", BuiltInRegistries.RULE_TEST, "getType"),
            typed("net.minecraft.world.level.levelgen.structure.templatesystem.PosRuleTest", BuiltInRegistries.POS_RULE_TEST, "getType"),
            typed("net.minecraft.world.level.levelgen.structure.templatesystem.rule.blockentity.RuleBlockEntityModifier", BuiltInRegistries.RULE_BLOCK_ENTITY_MODIFIER, "getType"));

    private static Typed typed(String type, Registry<?> registry, String method) {
        try {
            return new Typed(Class.forName(type), registry, method);
        } catch (ClassNotFoundException e) {
            throw new IllegalStateException("26.3 has no " + type, e);
        }
    }

    /** The type registries owners are named by: `BootFacts` keeps which of their entries the mods registered. */
    static List<Registry<?>> typeRegistries() {
        return TYPED.stream().map(Typed::registry).distinct().<Registry<?>>map(r -> r).toList();
    }

    /** Writes the export to `path`: a temp file renamed over it. */
    public static void write(String path) throws IOException {
        var registries = GameBridge.registries();
        if (registries == null) throw new IllegalStateException("the worldgen export needs the loaded registries");
        var base = vanilla();
        var entries = new JsonObject();
        var owners = new JsonObject();
        var warnings = new ArrayList<String>();
        for (var data : LOADED) GameBridge.safely("worldgen export " + data.key().identifier(), () -> registry(data, registries, base, entries, owners, warnings));
        var tags = new JsonObject();
        GameBridge.safely("worldgen export tags", () -> biomeTags(registries, base, tags));
        var root = new JsonObject();
        root.addProperty("format", 1);
        root.add("entries", entries);
        root.add("tags", tags);
        root.add("owners", owners);
        var notes = new JsonArray();
        warnings.forEach(notes::add);
        root.add("warnings", notes);
        var target = Path.of(path);
        var temp = target.resolveSibling(target.getFileName() + ".tmp");
        Files.writeString(temp, new GsonBuilder().serializeNulls().create().toJson(root), StandardCharsets.UTF_8);
        Files.move(temp, target, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
    }

    /** Vanilla's own entries: a second load of the vanilla pack alone, kept for the next export. */
    private static synchronized RegistryAccess.Frozen vanilla() {
        if (vanilla != null) return vanilla;
        var repository = ServerPacksSource.createVanillaTrustedRepository();
        var packs = new WorldLoader.PackConfig(repository, WorldDataConfiguration.DEFAULT, false, true);
        var config = new WorldLoader.InitConfig(packs, Commands.CommandSelection.DEDICATED, LevelBasedPermissionSet.GAMEMASTER);
        try {
            vanilla = Util.blockUntilDone(executor -> WorldLoader.load(
                    config,
                    context -> {
                        var dimensions = context.datapackWorldRegistries().lookupOrThrow(Registries.WORLD_PRESET).getOrThrow(WorldPresets.NORMAL).value().createWorldDimensions();
                        var complete = dimensions.bake(context.datapackDimensions().lookupOrThrow(Registries.LEVEL_STEM));
                        return new WorldLoader.DataLoadOutput<>(Unit.INSTANCE, complete.dimensionsRegistryAccess());
                    },
                    (manager, resources, layers, unit) -> {
                        manager.close();
                        return layers.compositeAccess();
                    },
                    Util.backgroundExecutor(),
                    executor)).get();
        } catch (Exception e) {
            throw new IllegalStateException("vanilla's registries couldn't be loaded for the worldgen export", e);
        }
        return vanilla;
    }

    private static <T> void registry(RegistryDataLoader.RegistryData<T> data, RegistryAccess.Frozen registries, RegistryAccess.Frozen base, JsonObject entries, JsonObject owners, List<String> warnings) {
        Optional<Registry<T>> live = registries.lookup(data.key());
        if (live.isEmpty()) return;
        Optional<Registry<T>> vanillaRegistry = base.lookup(data.key());
        String folder = data.key().identifier().getPath();
        var ops = RegistryOps.create(JsonOps.INSTANCE, registries);
        var vanillaOps = RegistryOps.create(JsonOps.INSTANCE, base);
        var written = new TreeMap<String, JsonElement>();
        var ownerLists = new TreeMap<String, List<String>>();
        for (var entry : live.get().entrySet()) {
            Identifier id = entry.getKey().identifier();
            T value = entry.getValue();
            JsonElement json = encode(data.elementCodec(), ops, value, folder, id);
            if (json instanceof JsonObject object) {
                if (value instanceof Biome biome) biome(object, biome, id, warnings);
                if (value instanceof NoiseGeneratorSettings settings) noise(object, settings, ops, id);
            }
            if (json != null && !json.isJsonNull() && vanillaRegistry.isPresent()) {
                var builtIn = vanillaRegistry.get().getOptional(id);
                if (builtIn.isPresent() && json.equals(encodeQuietly(data.elementCodec(), vanillaOps, builtIn.get()))) continue;
            }
            written.put(id.toString(), json == null ? JsonNull.INSTANCE : json);
            List<String> mods = new Walker().owners(value);
            if (!mods.isEmpty()) ownerLists.put(id.toString(), mods);
        }
        if (written.isEmpty()) return;
        var object = new JsonObject();
        written.forEach(object::add);
        entries.add(folder, object);
        if (ownerLists.isEmpty()) return;
        var ownerObject = new JsonObject();
        ownerLists.forEach((id, types) -> {
            var array = new JsonArray();
            types.forEach(array::add);
            ownerObject.add(id, array);
        });
        owners.add(folder, ownerObject);
    }

    private static <T> JsonElement encode(Codec<T> codec, RegistryOps<JsonElement> ops, T value, String folder, Identifier id) {
        try {
            return codec.encodeStart(ops, value).getOrThrow();
        } catch (Throwable t) {
            System.err.println("Horizon: worldgen export: " + folder + " " + id + " (" + value.getClass().getName() + ") can't be encoded: " + t);
            return null;
        }
    }

    private static <T> JsonElement encodeQuietly(Codec<T> codec, RegistryOps<JsonElement> ops, T value) {
        return codec.encodeStart(ops, value).result().orElse(null);
    }

    /** The codec writes the biome's generation settings from its field; NeoForge's modifiers act through the getters. */
    private static void biome(JsonObject json, Biome biome, Identifier id, List<String> warnings) {
        var settings = biome.getGenerationSettings();
        var features = new JsonArray();
        for (HolderSet<?> step : settings.features()) features.add(ids(step.stream().toList()));
        var carvers = new ArrayList<Holder<?>>();
        settings.getCarvers().forEach(carvers::add);
        var carverIds = ids(carvers);
        if (!features.equals(json.get("features")) || !carverIds.equals(json.get("carvers"))) {
            String note = "worldgen export: " + id + " modified by biome modifiers";
            System.err.println("Horizon: " + note);
            warnings.add(note);
        }
        json.add("features", features);
        json.add("carvers", carverIds);
    }

    private static JsonArray ids(List<? extends Holder<?>> holders) {
        var array = new JsonArray();
        for (Holder<?> holder : holders) {
            var key = holder.unwrapKey();
            if (key.isPresent()) array.add(key.get().identifier().toString());
            else System.err.println("Horizon: worldgen export: an inline " + holder.value().getClass().getName() + " in a biome's generation settings has no id");
        }
        return array;
    }

    /** `material_rule` is TerraBlender's to replace, through the getter. */
    private static void noise(JsonObject json, NoiseGeneratorSettings settings, RegistryOps<JsonElement> ops, Identifier id) {
        var rule = encode(MaterialRule.HOLDER_CODEC, ops, settings.materialRule(), "worldgen/noise_settings", id);
        if (rule != null) json.add("material_rule", rule);
    }

    private static void biomeTags(RegistryAccess.Frozen registries, RegistryAccess.Frozen base, JsonObject tags) {
        var live = registries.lookupOrThrow(Registries.BIOME);
        var builtIn = base.lookupOrThrow(Registries.BIOME);
        var out = new TreeMap<String, List<String>>();
        live.getTags().forEach(named -> {
            var values = new ArrayList<String>();
            named.stream().forEach(holder -> holder.unwrapKey().ifPresent(key -> values.add(key.identifier().toString())));
            var vanillaValues = builtIn.get(named.key()).map(set -> set.stream().flatMap(h -> h.unwrapKey().stream()).map(k -> k.identifier().toString()).toList());
            if (vanillaValues.isEmpty() || !new TreeSet<>(vanillaValues.get()).equals(new TreeSet<>(values))) out.put(named.key().location().toString(), values);
        });
        if (out.isEmpty()) return;
        var object = new JsonObject();
        out.forEach((id, values) -> {
            var tag = new JsonObject();
            tag.addProperty("replace", false);
            var array = new JsonArray();
            values.forEach(array::add);
            tag.add("values", array);
            object.add(id, tag);
        });
        tags.add("tags/worldgen/biome", object);
    }

    /** The mod-owned types inside one entry's live object graph, found by reflection. */
    private static final class Walker {
        private final Set<Object> seen = Collections.newSetFromMap(new IdentityHashMap<>());
        private final Set<String> found = new TreeSet<>();

        List<String> owners(Object root) {
            walk(root, 0);
            return List.copyOf(found);
        }

        private void walk(Object value, int depth) {
            if (value == null || depth > DEPTH || !seen.add(value)) return;
            if (value instanceof Holder<?> holder) {
                if (holder instanceof Holder.Direct<?> direct) walk(direct.value(), depth + 1);
                return;
            }
            if (value instanceof HolderSet.Named<?>) return;
            if (value instanceof Iterable<?> items) {
                for (Object item : items) walk(item, depth + 1);
                return;
            }
            if (value instanceof Map<?, ?> map) {
                for (var entry : map.entrySet()) {
                    walk(entry.getKey(), depth + 1);
                    walk(entry.getValue(), depth + 1);
                }
                return;
            }
            if (value instanceof Optional<?> optional) {
                optional.ifPresent(inner -> walk(inner, depth + 1));
                return;
            }
            if (value instanceof Object[] array) {
                for (Object item : array) walk(item, depth + 1);
                return;
            }
            Class<?> type = value.getClass();
            if (!descend(type)) return;
            record(value, type);
            for (Class<?> c = type; c != null && descend(c); c = c.getSuperclass()) {
                for (Field field : c.getDeclaredFields()) {
                    if (Modifier.isStatic(field.getModifiers()) || field.getType().isPrimitive()) continue;
                    try {
                        field.setAccessible(true);
                        walk(field.get(value), depth + 1);
                    } catch (ReflectiveOperationException | RuntimeException e) {
                        // A field the platform seals holds nothing a mod put there.
                    }
                }
            }
        }

        private static boolean descend(Class<?> type) {
            String name = type.getName();
            if (type.isSynthetic() || type.isHidden() || type.isEnum() || type.isArray()) return false;
            if (name.startsWith("java.") || name.startsWith("javax.") || name.startsWith("com.mojang.") || name.startsWith("com.google.") || name.startsWith("it.unimi.") || name.startsWith("org.slf4j")) return false;
            return !(net.minecraft.world.level.block.Block.class.isAssignableFrom(type)
                    || net.minecraft.world.level.block.state.BlockState.class.isAssignableFrom(type)
                    || net.minecraft.world.item.Item.class.isAssignableFrom(type)
                    || net.minecraft.world.level.material.Fluid.class.isAssignableFrom(type)
                    || net.minecraft.world.entity.EntityType.class.isAssignableFrom(type)
                    || net.minecraft.world.level.Level.class.isAssignableFrom(type)
                    || net.minecraft.resources.Identifier.class.isAssignableFrom(type));
        }

        private void record(Object value, Class<?> type) {
            for (Typed typed : TYPED) {
                if (!typed.type.isAssignableFrom(type)) continue;
                Object registered;
                try {
                    registered = call(value, typed.method);
                } catch (ReflectiveOperationException | RuntimeException e) {
                    found.add("unregistered:" + type.getName());
                    continue;
                }
                @SuppressWarnings("unchecked")
                Identifier key = registered == null ? null : ((Registry<Object>) typed.registry).getKey(registered);
                if (key == null) {
                    // A vanilla wrapper (a holder holder) has no entry of its own; a mod's is a mixin's installation.
                    if (!type.getName().startsWith("net.minecraft.")) found.add("unregistered:" + type.getName());
                } else if (BootFacts.owns(typed.registry, key)) {
                    found.add(typed.registry.key().identifier() + ":" + key);
                }
            }
        }

        private static Object call(Object value, String name) throws ReflectiveOperationException {
            for (Class<?> c = value.getClass(); c != null; c = c.getSuperclass()) {
                for (Method method : c.getDeclaredMethods()) {
                    if (method.getName().equals(name) && method.getParameterCount() == 0 && !Modifier.isStatic(method.getModifiers())) {
                        method.setAccessible(true);
                        return method.invoke(value);
                    }
                }
            }
            for (Class<?> iface : value.getClass().getInterfaces()) {
                var method = iface.getMethod(name);
                return method.invoke(value);
            }
            throw new NoSuchMethodException(value.getClass().getName() + "." + name);
        }
    }
}
