package dev.horizonlab.javamods.boot;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.TreeSet;
import java.util.function.Consumer;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntitySpawnReason;
import net.minecraft.world.entity.EntitySpawnRequest;
import net.minecraft.world.entity.EntityType;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.registries.NeoForgeRegistries;

/** The install's entity analysis (`entity-facts.json`) confirmed against the booted mods, as `entities.bin`; see docs/java-mods-map.md. */
public final class EntityFacts {
    private EntityFacts() {}

    /** Bit i of a reads mask is GROUPS[i]. */
    static final String[] GROUPS = {"identity", "position", "motion", "health", "effects", "equipment", "fire", "air", "flags", "name", "item", "riding", "removal", "species", "attributes", "ai"};
    static final int VERSION = 2;

    /** What the booted JVM can say about the analysis. */
    interface World {
        /** Listeners the game bus would call for this event class (slash name); 0 when the class doesn't exist. */
        int listeners(String event);

        /** Entity event classes (slash names) the bus has listener lists for. */
        List<String> busEvents(String root, List<String> prefixes);

        /** The names of the target class's methods that end with `$handler`, with the mixin class each was merged from (null when unknown). */
        List<String[]> merged(String target, String handler);

        /** Whether this JVM is the client dist. */
        boolean client();
    }

    private static volatile byte[] bin = new byte[0];

    /** `entities.bin`, handed to Rust once: Java keeps no copy; empty when the install has no analysis. */
    static synchronized byte[] bin() {
        var out = bin;
        bin = new byte[0];
        return out;
    }

    /** Runs once in `Boot.main`, after the mods have loaded and `BootFacts.seal`. */
    static void confirm() throws Exception {
        String path = System.getProperty("horizon.java.analysis");
        if (path == null || path.isEmpty()) return;
        var facts = JsonParser.parseString(Files.readString(Path.of(path))).getAsJsonObject();
        bin = build(facts, new Live(), line -> System.err.println("Horizon: entity facts: " + line), serializedAttachments());
        reflectiveWarnings(facts);
    }

    private static void reflectiveWarnings(JsonObject facts) {
        for (var e : facts.getAsJsonObject("reflective_entity_access").entrySet()) {
            System.err.println("Horizon: entity facts: " + e.getKey() + " reaches entities by reflection or method handles in " + e.getValue() + " classes; that can't be analysed");
        }
    }

    // --- The model

    /** The analysis confirmed: throws for a pack that can't be analysed ahead or a route whose handler isn't merged. */
    static byte[] build(JsonObject facts, World world, Consumer<String> log, List<String> attachments) throws IOException {
        var opaque = facts.getAsJsonArray("opaque");
        if (!opaque.isEmpty()) {
            var why = new StringBuilder("Java mods can't start: a mod has a coremod or class transformer that can't be analysed ahead:");
            for (JsonElement o : opaque) why.append("\n  ").append(o.getAsString());
            throw new IllegalStateException(why.toString());
        }
        var methods = new LinkedHashMap<String, Integer>();
        var classes = new TreeMap<String, ClassFacts>();
        for (var e : facts.getAsJsonObject("reads").entrySet()) {
            int mask = 0;
            for (JsonElement g : e.getValue().getAsJsonArray()) mask |= 1 << groupIndex(g.getAsString());
            classes.computeIfAbsent(e.getKey(), k -> new ClassFacts()).reads = mask;
        }
        for (var e : facts.getAsJsonObject("routes").entrySet()) {
            var target = e.getKey();
            var route = e.getValue().getAsJsonObject();
            var into = classes.computeIfAbsent(target, k -> new ClassFacts());
            for (var edge : route.getAsJsonObject("edge").entrySet()) {
                var handlers = new ArrayList<Handler>();
                for (JsonElement h : edge.getValue().getAsJsonArray()) {
                    var handler = h.getAsJsonObject();
                    var name = handler.get("handler").getAsString();
                    var mixin = handler.get("mixin").getAsString();
                    if (otherDist(handler, world)) {
                        log.accept(mixin + "." + name + " is not merged into " + target + ": it is a " + handler.get("side").getAsString() + " mixin");
                        continue;
                    }
                    var found = mergedName(world, target, name, mixin);
                    if (found == null) {
                        if (handler.get("conditional").getAsBoolean()) {
                            log.accept(mixin + "." + name + " is not merged into " + target + ": its config plugin left it out");
                            continue;
                        }
                        throw new IllegalStateException("entity facts: " + mixin + "." + name + " is not merged into " + target + " or is ambiguous");
                    }
                    handlers.add(new Handler(found, "HEAD".equals(handler.get("at").getAsString()) ? 0 : 1, handler.get("cancellable").getAsBoolean()));
                }
                if (!handlers.isEmpty()) into.edge.put(index(methods, edge.getKey()), handlers);
            }
            var conditional = route.getAsJsonObject("body_conditional");
            for (JsonElement key : route.getAsJsonArray("body")) {
                var k = key.getAsString();
                if (conditional.has(k) && !anyMerged(world, target, conditional.getAsJsonArray(k))) {
                    log.accept("body routes of " + target + "#" + k + " are not merged: their config plugin left them out");
                    continue;
                }
                into.body.add(index(methods, k));
            }
        }
        var overrides = new TreeMap<String, List<Integer>>();
        for (var e : facts.getAsJsonObject("overrides").entrySet()) {
            var list = new ArrayList<Integer>();
            for (JsonElement k : e.getValue().getAsJsonArray()) list.add(index(methods, k.getAsString()));
            overrides.put(e.getKey(), list);
        }
        var events = confirmEvents(facts, world, log);
        var caps = new TreeMap<String, List<String>>();
        for (var e : facts.getAsJsonObject("caps").entrySet()) {
            var list = new ArrayList<String>();
            for (JsonElement c : e.getValue().getAsJsonArray()) list.add(c.getAsString());
            caps.put(e.getKey(), list);
        }
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(VERSION);
        out.writeInt(methods.size());
        for (String key : methods.keySet()) text(out, key);
        out.writeInt(classes.size());
        for (var e : classes.entrySet()) {
            text(out, e.getKey());
            var c = e.getValue();
            out.writeInt(c.reads);
            out.writeInt(c.edge.size());
            for (var edge : c.edge.entrySet()) {
                out.writeInt(edge.getKey());
                out.writeInt(edge.getValue().size());
                for (Handler h : edge.getValue()) {
                    text(out, h.name);
                    out.writeByte(h.at);
                    out.writeBoolean(h.cancellable);
                }
            }
            out.writeInt(c.body.size());
            for (int m : c.body) out.writeInt(m);
        }
        out.writeInt(overrides.size());
        for (var e : overrides.entrySet()) {
            text(out, e.getKey());
            out.writeInt(e.getValue().size());
            for (int m : e.getValue()) out.writeInt(m);
        }
        out.writeInt(events.size());
        for (var e : events.entrySet()) {
            text(out, e.getKey());
            var subjects = e.getValue();
            out.writeBoolean(subjects == null);
            out.writeInt(subjects == null ? 0 : subjects.size());
            if (subjects != null) {
                for (JsonObject f : subjects) {
                    out.writeByte(switch (f.get("side").getAsString()) { case "server" -> 1; case "client" -> 2; default -> 0; });
                    for (String field : new String[] {"require", "exclude", "types"}) {
                        var list = f.get(field);
                        var items = list == null || list.isJsonNull() ? new JsonArray() : list.getAsJsonArray();
                        out.writeInt(items.size());
                        for (JsonElement i : items) text(out, i.getAsString());
                    }
                }
            }
        }
        out.writeInt(caps.size());
        for (var e : caps.entrySet()) {
            text(out, e.getKey());
            out.writeInt(e.getValue().size());
            for (String c : e.getValue()) text(out, c);
        }
        out.writeInt(attachments.size());
        for (String a : attachments) text(out, a);
        var eventIndex = new ArrayList<>(events.keySet());
        var hooks = facts.getAsJsonObject("hooks");
        out.writeInt(hooks.size());
        for (var e : hooks.entrySet()) {
            text(out, e.getKey());
            var hook = e.getValue().getAsJsonObject();
            int mask = 0;
            for (JsonElement g : hook.getAsJsonArray("reads")) mask |= 1 << groupIndex(g.getAsString());
            out.writeInt(mask);
            var posted = new ArrayList<Integer>();
            for (JsonElement ev : hook.getAsJsonArray("events")) {
                int at = eventIndex.indexOf(ev.getAsString());
                if (at >= 0) posted.add(at);
            }
            out.writeInt(posted.size());
            for (int at : posted) out.writeInt(at);
        }
        return bytes.toByteArray();
    }

    private static final class ClassFacts {
        int reads;
        final Map<Integer, List<Handler>> edge = new TreeMap<>();
        final Set<Integer> body = new TreeSet<>();
    }

    private record Handler(String name, int at, boolean cancellable) {}

    private static int groupIndex(String group) {
        for (int i = 0; i < GROUPS.length; i++) if (GROUPS[i].equals(group)) return i;
        throw new IllegalStateException("entity facts: unknown state group " + group);
    }

    private static int index(Map<String, Integer> methods, String key) {
        return methods.computeIfAbsent(key, k -> methods.size());
    }

    private static String mergedName(World world, String target, String handler, String mixin) {
        return Routing.mergedName(world.merged(target, handler), mixin);
    }

    private static boolean otherDist(JsonObject handler, World world) {
        var side = handler.get("side");
        return side != null && !"common".equals(side.getAsString()) && !side.getAsString().equals(world.client() ? "client" : "server");
    }

    private static boolean anyMerged(World world, String target, JsonArray handlers) {
        for (JsonElement h : handlers) {
            var o = h.getAsJsonObject();
            if (otherDist(o, world)) continue;
            if (mergedName(world, target, o.get("handler").getAsString(), o.get("mixin").getAsString()) != null) return true;
        }
        return false;
    }

    /** Event name to its filters, or null for all; events nobody listens to at runtime are dropped, and ones only the bus knows are added as all. */
    private static Map<String, List<JsonObject>> confirmEvents(JsonObject facts, World world, Consumer<String> log) {
        var out = new TreeMap<String, List<JsonObject>>();
        var analysed = new LinkedHashSet<String>();
        for (var e : facts.getAsJsonObject("events").entrySet()) {
            var event = e.getKey();
            var row = e.getValue().getAsJsonObject();
            int expected = row.getAsJsonArray("listeners").size();
            int live = world.listeners(event);
            analysed.add(event);
            if (live == 0) {
                log.accept(event + " not listened at runtime");
                continue;
            }
            if (live > expected) {
                log.accept(event + " has " + live + " listeners at runtime, " + expected + " analysed: all subjects");
                out.put(event, null);
                continue;
            }
            if (live < expected) log.accept(event + " has " + live + " listeners at runtime, " + expected + " analysed");
            var subjects = row.get("subjects");
            if (subjects.isJsonPrimitive()) {
                out.put(event, null);
            } else {
                var filters = new ArrayList<JsonObject>();
                for (JsonElement f : subjects.getAsJsonArray()) {
                    var types = f.getAsJsonObject().get("types");
                    if (types != null && types.isJsonArray() && types.getAsJsonArray().isEmpty()) continue;
                    filters.add(f.getAsJsonObject());
                }
                out.put(event, filters);
            }
        }
        var prefixes = new ArrayList<String>();
        for (JsonElement p : facts.getAsJsonArray("event_prefixes")) prefixes.add(p.getAsString());
        var supers = facts.getAsJsonObject("event_supers");
        for (String event : world.busEvents(facts.get("event_root").getAsString(), prefixes)) {
            if (analysed.contains(event) || world.listeners(event) == 0) continue;
            log.accept(event + " listened at runtime only");
            out.put(event, null);
            for (var below : supers.entrySet()) {
                for (JsonElement s : below.getValue().getAsJsonArray()) {
                    if (event.equals(s.getAsString()) && !analysed.contains(below.getKey())) {
                        log.accept(below.getKey() + " listened at runtime only (through " + event + ")");
                        out.put(below.getKey(), null);
                    }
                }
            }
        }
        return out;
    }

    private static void text(DataOutputStream out, String value) throws IOException {
        byte[] bytes = value.getBytes(StandardCharsets.UTF_8);
        out.writeInt(bytes.length);
        out.write(bytes);
    }

    // --- The booted JVM

    private static final class Live implements World {
        private final Object bus = NeoForge.EVENT_BUS;

        @Override
        public int listeners(String event) {
            try {
                var type = Class.forName(event.replace('/', '.'), false, Boot.class.getClassLoader());
                return ((net.neoforged.bus.ListenerList) listenerList().invoke(bus, type)).getListeners().length;
            } catch (ClassNotFoundException | LinkageError e) {
                return 0;
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private static Method listenerList() throws ReflectiveOperationException {
            var method = net.neoforged.bus.EventBus.class.getDeclaredMethod("getListenerList", Class.class);
            method.setAccessible(true);
            return method;
        }

        @Override
        @SuppressWarnings("unchecked")
        public List<String> busEvents(String root, List<String> prefixes) {
            try {
                var lists = net.neoforged.bus.EventBus.class.getDeclaredField("listenerLists");
                lists.setAccessible(true);
                var helper = lists.get(bus);
                var map = helper.getClass().getDeclaredField("backingMap");
                map.setAccessible(true);
                var rootClass = Class.forName(root.replace('/', '.'), false, Boot.class.getClassLoader());
                var out = new ArrayList<String>();
                for (Class<?> c : List.copyOf(((Map<Class<?>, Object>) map.get(helper)).keySet())) {
                    var name = c.getName().replace('.', '/');
                    if (rootClass.isAssignableFrom(c) || prefixes.stream().anyMatch(name::startsWith)) out.add(name);
                }
                return out;
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        @Override
        public List<String[]> merged(String target, String handler) {
            return Routing.merged(target, handler);
        }

        @Override
        public boolean client() {
            return net.neoforged.fml.loading.FMLEnvironment.getDist().isClient();
        }
    }

    /** The attachment types with a serializer: the ones that save with an entity. */
    private static List<String> serializedAttachments() throws ReflectiveOperationException {
        var field = net.neoforged.neoforge.attachment.AttachmentType.class.getDeclaredField("serializer");
        field.setAccessible(true);
        var out = new ArrayList<String>();
        for (var entry : NeoForgeRegistries.ATTACHMENT_TYPES.entrySet()) {
            if (field.get(entry.getValue()) != null) out.add(entry.getKey().identifier().toString());
        }
        out.sort(null);
        return out;
    }

    // --- Entity types, at the first server start

    /** `entity-types.bin`: each registered type's class chain (the class, its superclasses, then every interface; slash names) from one instance built on a view level, then
     * the capabilities mods attached to types (`EntityCapability` providers). */
    static byte[] types(ServerLevel level) throws Exception {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        var chains = new TreeMap<String, List<String>>();
        for (var entry : BuiltInRegistries.ENTITY_TYPE.entrySet()) {
            EntityType<?> type = entry.getValue();
            Entity entity;
            try {
                entity = instance(type, level);
            } catch (Throwable t) {
                throw new IllegalStateException("entity facts: " + entry.getKey().identifier() + " can't be built on a view level", t);
            }
            if (entity == null) continue;
            chains.put(entry.getKey().identifier().toString(), chain(entity.getClass()));
        }
        out.writeInt(chains.size());
        for (var e : chains.entrySet()) {
            text(out, e.getKey());
            out.writeInt(e.getValue().size());
            for (String c : e.getValue()) text(out, c);
        }
        var caps = capabilities();
        out.writeInt(caps.size());
        for (var e : caps.entrySet()) {
            text(out, e.getKey());
            out.writeInt(e.getValue().size());
            for (String c : e.getValue()) text(out, c);
        }
        return bytes.toByteArray();
    }

    /** One instance whatever the world allows (difficulty, features), as a saved entity loads: the facts are cached across worlds. */
    static Entity instance(EntityType<?> type, net.minecraft.world.level.Level level) {
        return type.create(level, new EntitySpawnRequest(EntitySpawnReason.LOAD, true));
    }

    static List<String> chain(Class<?> type) {
        var out = new ArrayList<String>();
        var interfaces = new LinkedHashSet<String>();
        for (Class<?> c = type; c != null; c = c.getSuperclass()) {
            out.add(c.getName().replace('.', '/'));
            collectInterfaces(c, interfaces);
        }
        out.addAll(interfaces);
        return out;
    }

    private static void collectInterfaces(Class<?> c, Set<String> into) {
        for (Class<?> i : c.getInterfaces()) {
            into.add(i.getName().replace('.', '/'));
            collectInterfaces(i, into);
        }
    }

    /** Type id to the capability names providers were registered for, read from NeoForge's entity capabilities after `RegisterCapabilitiesEvent`. */
    @SuppressWarnings("unchecked")
    private static Map<String, List<String>> capabilities() throws ReflectiveOperationException {
        var providers = net.neoforged.neoforge.capabilities.EntityCapability.class.getDeclaredField("providers");
        providers.setAccessible(true);
        var out = new TreeMap<String, List<String>>();
        for (var capability : net.neoforged.neoforge.capabilities.EntityCapability.getAll()) {
            var byType = (Map<EntityType<?>, ?>) providers.get(capability);
            for (var type : byType.keySet()) {
                out.computeIfAbsent(BuiltInRegistries.ENTITY_TYPE.getKey(type).toString(), k -> new ArrayList<>()).add(capability.name().toString());
            }
        }
        out.values().forEach(list -> list.sort(null));
        return out;
    }
}
