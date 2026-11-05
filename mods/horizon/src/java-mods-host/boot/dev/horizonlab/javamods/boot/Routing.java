package dev.horizonlab.javamods.boot;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import dev.horizonlab.javamods.host.Host;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.TreeMap;
import java.util.TreeSet;
import net.minecraft.core.registries.BuiltInRegistries;

/** The install's routing (`routing.json`) confirmed against the mixins merged at boot, as `routes.bin`; see docs/java-mods-map.md. */
public final class Routing {
    private Routing() {}

    static final int VERSION = 1;

    /** One method handle per route, by its place in `routes.bin`; null for a route that is off. */
    public static volatile MethodHandle[] HANDLES = new MethodHandle[0];

    private static volatile byte[] bin = new byte[0];

    private static final ThreadLocal<boolean[]> JAVA_EARLY = ThreadLocal.withInitial(() -> new boolean[1]);

    private static volatile java.util.Map<String, Integer> ids = java.util.Map.of();
    private static volatile boolean[] live = new boolean[0];
    private static volatile boolean[] edges = new boolean[0];

    /** The route stub's question after a native body returned: whether it was one of vanilla's early returns. */
    public static boolean early() {
        boolean java = earlyJava();
        return Host.earlyNative() | java;
    }

    /** The same after a Java body (a GUI bridge), which costs no JNI call. */
    public static boolean earlyJava() {
        boolean[] flag = JAVA_EARLY.get();
        boolean was = flag[0];
        flag[0] = false;
        return was;
    }

    /** A Java body's early return, for the route stub. */
    public static void markEarly() {
        JAVA_EARLY.get()[0] = true;
    }

    /** The route's id, -1 when the key is unknown or the route is off. */
    public static int route(String key) {
        Integer id = ids.get(key);
        return id != null && live[id] ? id : -1;
    }

    public static boolean edge(int id) {
        return edges[id];
    }

    public static boolean on(int id) {
        return live[id];
    }

    /** `routes.bin`, empty when the install has no routing. */
    static byte[] bin() {
        return bin;
    }

    /** Runs once in `Boot.main`, after `EntityFacts.confirm`. */
    static void confirm() throws Exception {
        String path = System.getProperty("horizon.java.analysis");
        if (path == null || path.isEmpty()) return;
        Path file = Path.of(path).resolveSibling("routing.json");
        if (!Files.isRegularFile(file)) return;
        long began = System.nanoTime();
        var routes = JsonParser.parseString(Files.readString(file)).getAsJsonObject().getAsJsonObject("routes");
        var loader = Boot.class.getClassLoader();
        var ordered = new TreeMap<String, JsonObject>();
        for (var e : routes.entrySet()) ordered.put(e.getKey(), e.getValue().getAsJsonObject());
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(VERSION);
        out.writeInt(ordered.size());
        var handles = new MethodHandle[ordered.size()];
        var idOf = new java.util.HashMap<String, Integer>();
        var liveFlags = new boolean[ordered.size()];
        var edgeFlags = new boolean[ordered.size()];
        boolean server = !net.neoforged.fml.loading.FMLEnvironment.getDist().isClient();
        int id = 0, on = 0, dropped = 0;
        for (var e : ordered.entrySet()) {
            String key = e.getKey();
            JsonObject route = e.getValue();
            String owner = key.substring(0, key.indexOf('#'));
            String name = key.substring(key.indexOf('#') + 1, key.indexOf('('));
            String desc = key.substring(key.indexOf('('));
            Class<?> type = null;
            try {
                if (server && owner.startsWith("net/minecraft/client/")) throw new ClassNotFoundException(owner);
                type = Class.forName(owner.replace('/', '.'), false, loader);
            } catch (ClassNotFoundException | NoClassDefFoundError absent) {
                // a client class on a server, or a mod class that is not loaded
            }
            var mods = new TreeSet<String>();
            int applied = 0;
            for (JsonElement c : route.getAsJsonArray("changes")) {
                var change = c.getAsJsonObject();
                mods.add(change.get("mod").getAsString());
                var mixin = change.get("mixin").getAsString();
                var handler = change.get("handler").getAsString();
                if (applied(type == null ? List.of() : merged(owner, handler), mixin)) {
                    applied++;
                } else {
                    dropped++;
                    System.err.println("Horizon: route " + key + ": " + change.get("mod").getAsString() + " " + mixin + "." + handler + " didn't apply (a config plugin, require = 0 or an unloaded target)");
                }
            }
            boolean live = applied > 0;
            if (live) {
                on++;
                handles[id] = handle(type, key, name, desc, route.get("static").getAsBoolean(), loader);
            }
            boolean body = route.get("kind").getAsString().equals("body");
            idOf.put(key, id);
            liveFlags[id] = live;
            edgeFlags[id] = !body;
            String family = route.get("family").getAsString();
            text(out, key);
            out.writeByte(body ? 1 : 0);
            text(out, family);
            out.writeBoolean(live);
            out.writeBoolean(route.get("hot").getAsBoolean());
            if (!live) {
                out.writeInt(-1);
            } else {
                var admitted = admitted(family, type, owner, name, desc, route.getAsJsonArray("overriders"));
                if (admitted == null) {
                    out.writeInt(family.equals("entity") || family.equals("block_entity") ? -2 : -1);
                } else {
                    out.writeInt(admitted.size());
                    for (String a : admitted) text(out, a);
                }
            }
            System.err.println("Horizon: route " + key + " " + (body ? "body" : "edge") + " " + (live ? "on" : "off") + " (" + String.join(", ", mods) + ")");
            id++;
        }
        HANDLES = handles;
        ids = idOf;
        Routing.live = liveFlags;
        edges = edgeFlags;
        bin = bytes.toByteArray();
        System.err.println("Horizon: routes: " + on + " on, " + (ordered.size() - on) + " off, " + dropped + " changes dropped, confirmed in " + (System.nanoTime() - began) / 1_000_000 + " ms");
    }

    /** Whether the mixin's handler is merged into the target: Mixin's own applied list is empty by the time the boot looks, but the merged method names its mixin. A predicted mixin that is not applied is off, not an error: plugins, require = 0 and unloaded targets leave mixins out. */
    static boolean applied(List<String[]> merged, String mixin) {
        return merged.stream().anyMatch(m -> mixin.equals(m[1]));
    }

    private static MethodHandle handle(Class<?> owner, String key, String name, String desc, boolean isStatic, ClassLoader loader) throws Exception {
        try {
            var lookup = MethodHandles.privateLookupIn(owner, MethodHandles.lookup());
            var type = MethodType.fromMethodDescriptorString(desc, loader);
            return isStatic ? lookup.findStatic(owner, name, type) : lookup.findVirtual(owner, name, type);
        } catch (ReflectiveOperationException | IllegalArgumentException e) {
            throw new IllegalStateException("route " + key + ": no method handle", e);
        }
    }

    /** The registry names the route's family admits, or null for a family without a list (all of it, or decided by the spec that adds its receivers). */
    private static List<String> admitted(String family, Class<?> owner, String ownerName, String name, String desc, JsonArray overriders) {
        var registry = switch (family) {
            case "block" -> BuiltInRegistries.BLOCK;
            case "item" -> BuiltInRegistries.ITEM;
            default -> null;
        };
        if (registry == null) return null;
        var out = new ArrayList<String>();
        var members = new ArrayList<Class<?>>();
        for (var entry : registry.entrySet()) members.add(entry.getValue().getClass());
        // A state's methods (owner is not a supertype of the registry's values) reach every entry
        if (members.stream().noneMatch(owner::isAssignableFrom)) return null;
        for (var entry : registry.entrySet()) {
            if (reaches(entry.getValue().getClass(), ownerName, name, desc, overriders)) out.add(entry.getKey().identifier().toString());
        }
        return out;
    }

    /** Whether a virtual call of the method on `type` reaches the body of `owner`, which a mod's mixin changes. */
    static boolean reaches(Class<?> type, String owner, String name, String desc, JsonArray overriders) {
        for (Class<?> c = type; c != null; c = c.getSuperclass()) {
            if (!declares(c, name, desc)) continue;
            String found = c.getName().replace('.', '/');
            if (found.equals(owner)) return true;
            if (!found.startsWith("net/minecraft/")) return false;
            for (JsonElement o : overriders) {
                var over = o.getAsJsonObject();
                if (over.get("class").getAsString().equals(found) && over.get("super_call").getAsBoolean()) return true;
            }
            return false;
        }
        return false;
    }

    private static boolean declares(Class<?> c, String name, String desc) {
        for (Method m : c.getDeclaredMethods()) {
            if (m.getName().equals(name) && MethodType.methodType(m.getReturnType(), m.getParameterTypes()).toMethodDescriptorString().equals(desc)) return true;
        }
        return false;
    }

    /** The names of the target class's methods that end with `$handler`, with the mixin class each was merged from (null when unknown). */
    static List<String[]> merged(String target, String handler) {
        try {
            var out = new ArrayList<String[]>();
            for (Method m : Class.forName(target.replace('/', '.'), false, Boot.class.getClassLoader()).getDeclaredMethods()) {
                if (!m.getName().equals(handler) && !m.getName().endsWith("$" + handler)) continue;
                var from = m.getAnnotation(org.spongepowered.asm.mixin.transformer.meta.MixinMerged.class);
                out.add(new String[] {m.getName(), from == null ? null : from.mixin()});
            }
            return out;
        } catch (ClassNotFoundException e) {
            return List.of();
        }
    }

    /** The merged method of a handler: the one name ending `$handler`, told apart by the mixin it came from when several end alike. */
    static String mergedName(List<String[]> found, String mixin) {
        if (found.size() > 1) found = found.stream().filter(m -> mixin.equals(m[1])).toList();
        return found.size() == 1 ? found.get(0)[0] : null;
    }

    private static void text(DataOutputStream out, String value) throws IOException {
        byte[] bytes = value.getBytes(StandardCharsets.UTF_8);
        out.writeInt(bytes.length);
        out.write(bytes);
    }
}
