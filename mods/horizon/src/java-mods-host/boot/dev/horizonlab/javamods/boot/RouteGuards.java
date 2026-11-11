package dev.horizonlab.javamods.boot;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.EntityBlock;
import net.minecraft.world.level.block.state.BlockState;

/** A route's guards (`routing.json` `guards`) against the boot's classes and registries: the receivers, items and block states native may pass and still reach a handler. */
final class RouteGuards {
    private RouteGuards() {}

    private static final String[] NOT_MOD = {"net/minecraft/", "com/mojang/", "java/", "javax/"};

    /** Each field null when it admits any: absent in the file, or naming a class that doesn't load. */
    record Parsed(List<Class<?>> recv, List<Class<?>> items, List<Class<?>> blocks, List<Class<?>> blockEntities, boolean argNever) {
        static final Parsed ALL = new Parsed(null, null, null, null, false);
    }

    /** What one live route admits: `receivers` null for the family's default, `items` and `blocks` null for any; `inert` when nothing native passes reaches a handler. */
    record Admission(List<String> receivers, List<String> items, int[] blocks, boolean inert) {}

    static Parsed parse(JsonElement guards, ClassLoader loader) {
        if (guards == null || !guards.isJsonObject()) return Parsed.ALL;
        var g = guards.getAsJsonObject();
        var items = union(classes(g.get("recv_item"), loader), indexed(g.get("arg_item"), loader), g.get("recv_item"), g.get("arg_item"));
        var blocks = union(indexed(g.get("arg_block"), loader), classes(g.get("block_at"), loader), g.get("arg_block"), g.get("block_at"));
        var entities = classes(g.get("be_at"), loader);
        boolean blocksOpen = present(g.get("arg_block")) || present(g.get("block_at"));
        if (blocksOpen && blocks == null || present(g.get("be_at")) && entities == null) blocks = entities = null;
        boolean argNever = false;
        if (g.get("arg_class") instanceof JsonObject args) {
            for (var e : args.entrySet()) argNever |= modOnly(classes(e.getValue(), loader));
        }
        return new Parsed(classes(g.get("recv"), loader), items, blocks, entities, argNever);
    }

    private static boolean present(JsonElement raw) {
        return raw != null && !raw.isJsonNull();
    }

    /** Two fields that narrow the same list: any when both are absent or one fails to load, else both fields' classes. */
    private static List<Class<?>> union(List<Class<?>> a, List<Class<?>> b, JsonElement rawA, JsonElement rawB) {
        boolean hasA = present(rawA), hasB = present(rawB);
        if (!hasA && !hasB || hasA && a == null || hasB && b == null) return null;
        var out = new ArrayList<Class<?>>();
        if (a != null) out.addAll(a);
        if (b != null) out.addAll(b);
        return out;
    }

    private static List<Class<?>> indexed(JsonElement value, ClassLoader loader) {
        if (!(value instanceof JsonObject byIndex)) return null;
        var out = new ArrayList<Class<?>>();
        for (var e : byIndex.entrySet()) {
            var found = classes(e.getValue(), loader);
            if (found == null) return null;
            out.addAll(found);
        }
        return out;
    }

    private static List<Class<?>> classes(JsonElement value, ClassLoader loader) {
        if (value == null || !value.isJsonArray()) return null;
        var out = new ArrayList<Class<?>>();
        for (var name : value.getAsJsonArray()) {
            try {
                out.add(Class.forName(name.getAsString().replace('/', '.'), false, loader));
            } catch (ClassNotFoundException | LinkageError absent) {
                return null;
            }
        }
        return out;
    }

    /** Whether no object native holds can be an instance of any of these: each is a mod's class, which no vanilla class extends (an interface may be injected, so it never counts). */
    static boolean modOnly(List<Class<?>> classes) {
        if (classes == null || classes.isEmpty()) return false;
        for (var c : classes) {
            if (c.isInterface()) return false;
            var name = c.getName().replace('.', '/');
            for (var prefix : NOT_MOD) if (name.startsWith(prefix)) return false;
        }
        return true;
    }

    static boolean matches(List<Class<?>> classes, Class<?> type) {
        for (var c : classes) if (c.isAssignableFrom(type)) return true;
        return false;
    }

    /** The ids of the registry's items (id to the item's class) whose class is one of the guard's, mod items included. */
    static List<String> items(List<Class<?>> classes, Iterable<Map.Entry<String, Class<?>>> registry) {
        var out = new ArrayList<String>();
        for (var entry : registry) if (matches(classes, entry.getValue())) out.add(entry.getKey());
        return out;
    }

    static List<Map.Entry<String, Class<?>>> registeredItems() {
        var out = new ArrayList<Map.Entry<String, Class<?>>>();
        for (var entry : BuiltInRegistries.ITEM.entrySet()) out.add(Map.entry(entry.getKey().identifier().toString(), entry.getValue().getClass()));
        return out;
    }

    /** Java's ids of the block states whose block, or the block entity it makes, is one of the guard's classes; a block entity that can't be made admits its state. */
    static int[] blocks(List<Class<?>> blocks, List<Class<?>> blockEntities) {
        var out = new ArrayList<Integer>();
        for (BlockState state : Block.BLOCK_STATE_REGISTRY) {
            boolean hit = blocks != null && matches(blocks, state.getBlock().getClass());
            if (!hit && blockEntities != null && state.getBlock() instanceof EntityBlock maker) {
                try {
                    var made = maker.newBlockEntity(BlockPos.ZERO, state);
                    hit = made != null && matches(blockEntities, made.getClass());
                } catch (RuntimeException | LinkageError unmade) {
                    hit = true;
                }
            }
            if (hit) out.add(Block.BLOCK_STATE_REGISTRY.getId(state));
        }
        return out.stream().mapToInt(Integer::intValue).toArray();
    }

    /** One live route's admission: `admitted` is the family's receiver list already narrowed by `recv`, null for a family without one; `registry` is read only for an item guard. */
    static Admission admission(String family, List<String> admitted, Parsed guards, Supplier<List<Map.Entry<String, Class<?>>>> registry) {
        boolean listless = admitted == null && !family.equals("entity") && !family.equals("block_entity");
        boolean inert = guards.argNever() || admitted != null && admitted.isEmpty() || listless && modOnly(guards.recv());
        var items = guards.items() == null ? null : items(guards.items(), registry.get());
        boolean anyBlocks = guards.blocks() == null && guards.blockEntities() == null;
        var blocks = anyBlocks ? null : blocks(guards.blocks(), guards.blockEntities());
        inert |= items != null && items.isEmpty() || blocks != null && blocks.length == 0;
        return new Admission(admitted, items, blocks, inert);
    }
}
