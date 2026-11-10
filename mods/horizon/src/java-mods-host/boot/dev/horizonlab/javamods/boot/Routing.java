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

    private static volatile String[] KEYS = new String[0];
    private static volatile String[] DESCS = new String[0];
    private static volatile String[] OWNERS = new String[0];
    private static volatile boolean[] STATICS = new boolean[0];
    private static volatile java.util.Map<String, String> MODS = java.util.Map.of();
    private static final java.util.Set<String> TOLD = java.util.concurrent.ConcurrentHashMap.newKeySet();

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

    /** Whether a call's receiver is a server player's inventory, which is all Horizon's native serves; the Java client's own is not. */
    public static boolean serverInventory(Object receiver) {
        return receiver instanceof net.minecraft.world.entity.player.Inventory inventory && inventory.player instanceof net.minecraft.server.level.ServerPlayer;
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
        var keys = new String[ordered.size()];
        var descs = new String[ordered.size()];
        var statics = new boolean[ordered.size()];
        var owners = new String[ordered.size()];
        var idOf = new java.util.HashMap<String, Integer>();
        var liveFlags = new boolean[ordered.size()];
        var edgeFlags = new boolean[ordered.size()];
        boolean server = !net.neoforged.fml.loading.FMLEnvironment.getDist().isClient();
        var gates = gates(Routing.class.getResourceAsStream("/route_gates.tsv"));
        var ungated = new ArrayList<String>();
        int id = 0, on = 0, dropped = 0;
        var modOf = new java.util.HashMap<String, String>();
        for (var r : ordered.values()) {
            for (JsonElement c : r.getAsJsonArray("changes")) modOf.put(c.getAsJsonObject().get("mixin").getAsString(), c.getAsJsonObject().get("mod").getAsString());
        }
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
            if (type != null) {
                var clash = contradiction(key, name, MethodType.fromMethodDescriptorString(desc, loader).parameterCount(), declaredNamed(type, name) > 1, route.getAsJsonArray("changes"), shapes(type), modOf);
                if (clash != null) throw new IllegalStateException(clash);
            }
            boolean live = applied > 0;
            if (live) {
                on++;
                handles[id] = handle(type, key, name, desc, route.get("static").getAsBoolean(), loader);
            }
            keys[id] = key;
            descs[id] = desc;
            owners[id] = owner;
            statics[id] = route.get("static").getAsBoolean();
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
            System.err.println(line(key, body, live, gates.get(key), mods));
            if (live && !gates.containsKey(key)) for (String mod : mods) ungated.add(mod + ": " + simple(owner) + "." + name);
            id++;
        }
        if (!ungated.isEmpty()) System.err.println(summary(ungated));
        HANDLES = handles;
        KEYS = keys;
        DESCS = descs;
        STATICS = statics;
        OWNERS = owners;
        MODS = modOf;
        ids = idOf;
        Routing.live = liveFlags;
        edges = edgeFlags;
        bin = bytes.toByteArray();
        System.err.println("Horizon: routes: " + on + " on, " + (ordered.size() - on) + " off, " + dropped + " changes dropped, confirmed in " + (System.nanoTime() - began) / 1_000_000 + " ms");
    }

    /** The routes this build gates, from `route_gates.tsv`: key to the number of native sites that still skip the gate; empty when the stream is null. */
    static java.util.Map<String, Integer> gates(java.io.InputStream stream) throws IOException {
        var out = new java.util.HashMap<String, Integer>();
        if (stream == null) return out;
        try (stream) {
            for (String row : new String(stream.readAllBytes(), StandardCharsets.UTF_8).split("\n")) {
                if (row.isBlank() || row.startsWith("#")) continue;
                String[] cells = row.split("\t");
                out.put(cells[0], cells.length > 4 ? Integer.parseInt(cells[4]) : 0);
            }
        }
        return out;
    }

    /** One route's boot line; `partial` is null for a route without a gate. */
    static String line(String key, boolean body, boolean live, Integer partial, java.util.Collection<String> mods) {
        String gate = partial == null ? "UNGATED" : partial == 0 ? "gated" : "gated (partial: " + partial + " sites)";
        return "Horizon: route " + key + " " + (body ? "body" : "edge") + " " + (live ? "on" : "off") + " " + gate + " (" + String.join(", ", mods) + ")";
    }

    /** The loud line for patches that are on but have no gate; the items are `mod: Class.method`. */
    static String summary(List<String> ungated) {
        return "Horizon: " + ungated.size() + " behaviours mods change won't take effect (native runs them without the mods' patches): " + String.join(", ", ungated);
    }

    private static String simple(String owner) {
        return owner.substring(owner.lastIndexOf('/') + 1);
    }

    /** Whether the mixin's handler is merged into the target: Mixin's own applied list is empty by the time the boot looks, but the merged method names its mixin. A predicted mixin that is not applied is off, not an error: plugins, require = 0 and unloaded targets leave mixins out. */
    static boolean applied(List<String[]> merged, String mixin) {
        return merged.stream().anyMatch(m -> mixin.equals(m[1]));
    }

    /** A merged handler as it stands in the target class: the annotation Mixin kept names what it injects into and where. */
    record Applied(String name, String mixin, String kind, List<String> at, boolean argsOnly, int params, List<String> targets) {}

    private static int declaredNamed(Class<?> type, String name) {
        int n = 0;
        for (Method m : type.getDeclaredMethods()) if (m.getName().equals(name)) n++;
        return n;
    }

    /** The merged handlers of `type` whose injection annotation is readable; one without it can't contradict anything. */
    static List<Applied> shapes(Class<?> type) {
        var out = new ArrayList<Applied>();
        for (Method m : type.getDeclaredMethods()) {
            var from = m.getAnnotation(org.spongepowered.asm.mixin.transformer.meta.MixinMerged.class);
            if (from == null) continue;
            for (var a : m.getAnnotations()) {
                try {
                    var kind = snake(a.annotationType().getSimpleName());
                    var targets = a.annotationType().getMethod("method").invoke(a);
                    if (!(targets instanceof String[] names)) continue;
                    var at = new ArrayList<String>();
                    var raw = a.annotationType().getMethod("at").invoke(a);
                    for (Object one : raw instanceof Object[] many ? many : new Object[] {raw}) {
                        at.add((String) one.getClass().getMethod("value").invoke(one));
                    }
                    boolean argsOnly = false;
                    try {
                        argsOnly = (boolean) a.annotationType().getMethod("argsOnly").invoke(a);
                    } catch (NoSuchMethodException none) {
                        // only ModifyVariable has it
                    }
                    out.add(new Applied(m.getName(), from.mixin(), kind, at, argsOnly, m.getParameterCount(), List.of(names)));
                } catch (ReflectiveOperationException | LinkageError | ClassCastException unreadable) {
                    // not an injector annotation
                }
            }
        }
        return out;
    }

    private static String snake(String simple) {
        return simple.replaceAll("([a-z])([A-Z])", "$1_$2").toLowerCase(java.util.Locale.ROOT);
    }

    /** Whether the change only surrounds the method's body (true), reaches into it (false), or can't be told from what the analysis kept (null). */
    static Boolean edge(String kind, List<String> at, boolean locals) {
        var firsts = at.stream().map(a -> a.contains("(") ? a.substring(0, a.indexOf('(')) : a).toList();
        return switch (kind) {
            case "inject" -> !firsts.isEmpty() && firsts.stream().allMatch(a -> a.equals("HEAD") || a.equals("RETURN") || a.equals("TAIL")) && !locals;
            case "modify_return_value" -> firsts.stream().allMatch(a -> a.equals("RETURN"));
            case "wrap_method" -> true;
            case "modify_variable" -> null;
            default -> false;
        };
    }

    /** What the merged handlers say against the analysis for one route: a mixin that applied unpredicted, or one whose kind isn't the predicted kind. Null when they agree. */
    static String contradiction(String key, String name, int targetParams, boolean overloaded, JsonArray changes, List<Applied> applied, java.util.Map<String, String> modOf) {
        String desc = key.substring(key.indexOf('('));
        for (var a : applied) {
            if (a.targets().stream().noneMatch(t -> targets(t, name, desc))) continue;
            if (overloaded && a.targets().stream().noneMatch(t -> t.contains("("))) continue;
            JsonObject change = null;
            for (JsonElement c : changes) {
                var one = c.getAsJsonObject();
                var handler = one.get("handler").getAsString();
                if (one.get("mixin").getAsString().equals(a.mixin()) && (a.name().equals(handler) || a.name().endsWith("$" + handler))) change = one;
            }
            if (change == null) {
                return "route " + key + ": " + modOf.getOrDefault(a.mixin(), "unknown mod") + " " + a.mixin() + "." + a.name() + " applied but the analysis didn't predict it";
            }
            if (change.has("lambda")) continue;
            var ats = new ArrayList<String>();
            for (JsonElement t : change.getAsJsonArray("at")) ats.add(t.getAsString());
            Boolean predicted = edge(change.get("kind").getAsString(), ats, change.get("locals").getAsBoolean());
            Boolean actual = edge(a.kind(), a.at(), a.kind().equals("inject") && a.params() > targetParams + 1);
            if (predicted != null && actual != null && !predicted.equals(actual)) {
                return "route " + key + ": " + change.get("mod").getAsString() + " " + a.mixin() + "." + change.get("handler").getAsString() + " applied as " + (actual ? "edge" : "body") + " but was predicted " + (predicted ? "edge" : "body");
            }
        }
        return null;
    }

    private static boolean targets(String target, String name, String desc) {
        String bare = target.substring(target.indexOf(';') + 1);
        return bare.contains("(") ? bare.equals(name + desc) : bare.equals(name);
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

    /** The `route` op: Rust's call of a vanilla method as Java's merged method. A throw is the reply, not caught here: Rust panics with it, as an exception ends a vanilla tick. */
    static void call(Crossing.In in, Crossing.Out out) throws Exception {
        int id = in.readInt();
        run(KEYS[id], OWNERS[id], DESCS[id], STATICS[id], HANDLES[id], in, out);
    }

    private static MethodHandle noop;

    static void noop() {}

    /** `route` against a static no-op, for the crossing bench. */
    static void benchCall(Crossing.In in, Crossing.Out out) throws Exception {
        in.readInt();
        if (noop == null) noop = MethodHandles.lookup().findStatic(Routing.class, "noop", MethodType.methodType(void.class));
        run("bench#noop()V", "dev/horizonlab/javamods/boot/Routing", "()V", true, noop, in, out);
    }

    private static final String SUPPORTED_RETURNS = "VZIJFD";
    /** The object arguments native writes: a `Player` is the acting player, filled as for a player receiver. */
    private static final java.util.Set<Class<?>> READABLE = java.util.Set.of(net.minecraft.core.BlockPos.class, String.class, net.minecraft.world.item.ItemStack.class,
            net.minecraft.world.phys.Vec3.class, net.minecraft.network.chat.Component.class, net.minecraft.world.entity.player.Player.class);

    private static void run(String key, String owner, String desc, boolean isStatic, MethodHandle handle, Crossing.In in, Crossing.Out out) throws Exception {
        byte recv = in.readByte();
        var types = MethodType.fromMethodDescriptorString(desc, Boot.class.getClassLoader()).parameterArray();
        char ret = desc.charAt(desc.indexOf(')') + 1);
        String unreadable = SUPPORTED_RETURNS.indexOf(ret) < 0 ? "return type " + desc.substring(desc.indexOf(')') + 1) : null;
        for (var type : types) {
            if (unreadable == null && !type.isPrimitive() && !READABLE.contains(type)) unreadable = type.getName();
            if (unreadable == null && type == void.class || type == byte.class || type == short.class || type == char.class) unreadable = type.getName();
        }
        if (unreadable != null || handle == null) {
            if (TOLD.add(key)) System.err.println("Horizon: route " + key + ": no argument reader for " + unreadable);
            out.writeByte(1);
            GameBridge.writeText(out, "unknown");
            GameBridge.writeText(out, "no argument reader for " + unreadable);
            return;
        }
        Object receiver = null;
        switch (recv) {
            case 0 -> {
                var player = GameBridge.fill(in, false).player();
                receiver = owner.equals("net/minecraft/world/entity/player/Inventory") ? player.getInventory()
                        : owner.equals("net/minecraft/world/inventory/AbstractContainerMenu") ? player.containerMenu : player;
            }
            case 1 -> {
                var level = GameBridge.level(GameBridge.text(in));
                ((ViewLevelData) level.getLevelData()).gameTime = in.readLong();
                receiver = level;
            }
            default -> {}
        }
        var args = new ArrayList<Object>();
        if (!isStatic) args.add(receiver);
        for (var type : types) {
            if (type == int.class) args.add(in.readInt());
            else if (type == long.class) args.add(in.readLong());
            else if (type == float.class) args.add(in.readFloat());
            else if (type == double.class) args.add(in.readDouble());
            else if (type == boolean.class) args.add(in.readBoolean());
            else if (type == String.class) args.add(in.text());
            else if (type == net.minecraft.world.item.ItemStack.class) args.add(GameBridge.decode(in.readNBytes(in.readInt())));
            else if (type == net.minecraft.world.phys.Vec3.class) args.add(new net.minecraft.world.phys.Vec3(in.readDouble(), in.readDouble(), in.readDouble()));
            else if (type == net.minecraft.network.chat.Component.class) args.add(GameBridge.component(in.readNBytes(in.readInt())));
            else if (type == net.minecraft.world.entity.player.Player.class) args.add(GameBridge.fill(in, false).player());
            else args.add(net.minecraft.core.BlockPos.of(in.readLong()));
        }
        long started = Profile.ON ? System.nanoTime() : 0;
        Object result;
        try {
            result = handle.invokeWithArguments(args);
        } catch (Throwable t) {
            if (GameBridge.outermost()) GameBridge.abandonCall();
            out.writeByte(1);
            GameBridge.writeText(out, modOf(t));
            var first = t.getStackTrace().length > 0 ? " at " + t.getStackTrace()[0] : "";
            GameBridge.writeText(out, t + first);
            return;
        }
        if (Profile.ON) Profile.event("route:" + key, System.nanoTime() - started);
        out.writeByte(0);
        switch (ret) {
            case 'Z' -> out.writeBoolean((Boolean) result);
            case 'I' -> out.writeInt((Integer) result);
            case 'J' -> out.writeLong((Long) result);
            case 'F' -> out.writeFloat((Float) result);
            case 'D' -> out.writeDouble((Double) result);
            default -> {}
        }
        GameBridge.writeEffects(out);
    }

    /** The mod of the first frame that runs a merged mixin handler, else "unknown". */
    static String modOf(Throwable t) {
        for (var frame : t.getStackTrace()) {
            try {
                for (Method m : Class.forName(frame.getClassName(), false, Boot.class.getClassLoader()).getDeclaredMethods()) {
                    if (!m.getName().equals(frame.getMethodName())) continue;
                    var from = m.getAnnotation(org.spongepowered.asm.mixin.transformer.meta.MixinMerged.class);
                    if (from != null) return MODS.getOrDefault(from.mixin(), "unknown");
                }
            } catch (ClassNotFoundException | LinkageError skip) {
                // a frame of a class that is gone
            }
        }
        return "unknown";
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
