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
import java.lang.reflect.ParameterizedType;
import java.lang.reflect.Type;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Optional;
import java.util.TreeMap;
import java.util.TreeSet;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.ServerCommonPacketListenerImpl;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.food.FoodData;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.item.ItemInstance;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LevelAccessor;
import net.minecraft.world.level.LevelReader;
import net.minecraft.world.level.ServerLevelAccessor;
import net.minecraft.world.level.portal.TeleportTransition;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.level.storage.TagValueOutput;
import net.minecraft.world.level.storage.ValueInput;
import net.minecraft.world.level.storage.ValueOutput;
import net.minecraft.world.phys.Vec3;

/** The install's routing (`routing.json`) confirmed against the mixins merged at boot, as `routes.bin`; see docs/java-mods-map.md. */
public final class Routing {
    private Routing() {}

    static final int VERSION = 2;

    /** One method handle per route, by its place in `routes.bin`; null for a route that is off. */
    public static volatile MethodHandle[] HANDLES = new MethodHandle[0];

    private static volatile String[] KEYS = new String[0];
    private static volatile String[] DESCS = new String[0];
    private static volatile String[] OWNERS = new String[0];
    private static volatile boolean[] STATICS = new boolean[0];
    private static volatile Plan[] PLANS = new Plan[0];
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

    /** `routes.bin`, handed to Rust once: Java keeps no copy; empty when the install has no routing. */
    static synchronized byte[] bin() {
        var out = bin;
        bin = new byte[0];
        return out;
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
        var plans = new Plan[ordered.size()];
        var owners = new String[ordered.size()];
        var idOf = new java.util.HashMap<String, Integer>();
        var liveFlags = new boolean[ordered.size()];
        var edgeFlags = new boolean[ordered.size()];
        boolean server = !net.neoforged.fml.loading.FMLEnvironment.getDist().isClient();
        var gates = gates(Routing.class.getResourceAsStream("/route_gates.tsv"));
        var statuses = statuses(Routing.class.getResourceAsStream("/route_status.tsv"));
        var ungated = new ArrayList<String>();
        var waiting = new ArrayList<String>();
        int id = 0, on = 0, dropped = 0, inert = 0, constructors = 0;
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
            boolean built = constructor(name);
            if (built) constructors++;
            boolean live = applied > 0 && !built;
            if (live) {
                on++;
                handles[id] = handle(type, key, name, desc, route.get("static").getAsBoolean(), loader);
                var method = declared(type, name, desc);
                if (method != null) {
                    plans[id] = plan(key, method, gates.containsKey(key));
                    if (plans[id].unreadable() != null && TOLD.add(key)) System.err.println("Horizon: route " + key + ": no argument reader for " + plans[id].unreadable());
                }
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
            var admission = live ? admission(family, type, owner, name, desc, route.getAsJsonArray("overriders"), route.get("guards"), loader) : null;
            boolean none = admission != null && admission.inert();
            write(out, admission, family);
            if (none) inert++;
            String status = statuses.get(key);
            System.err.println(built ? constructorLine(key, route.getAsJsonObject("construct"), applied > 0, mods) : line(key, body, live, none, gates.get(key), status, mods));
            if (live && !none && !gates.containsKey(key)) {
                if (status == null) for (String mod : mods) ungated.add(mod + ": " + simple(owner) + "." + name);
                else if (!status.startsWith("verdict: ")) waiting.add(status);
            }
            id++;
        }
        if (!ungated.isEmpty()) System.err.println(summary(ungated));
        if (!waiting.isEmpty()) System.err.println(waiting(waiting));
        HANDLES = handles;
        KEYS = keys;
        DESCS = descs;
        STATICS = statics;
        PLANS = plans;
        OWNERS = owners;
        MODS = modOf;
        ids = idOf;
        Routing.live = liveFlags;
        edges = edgeFlags;
        bin = bytes.toByteArray();
        Glyphs.decide();
        System.err.println("Horizon: routes: " + on + " on (" + inert + " inert by guard), " + (ordered.size() - on) + " off (" + constructors + " constructors, never bound), " + dropped + " changes dropped, confirmed in " + (System.nanoTime() - began) / 1_000_000 + " ms");
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

    /** Each route's deferral reason or `verdict: <verdict>`, from `route_status.tsv`; empty when the stream is null. */
    static java.util.Map<String, String> statuses(java.io.InputStream stream) throws IOException {
        var out = new java.util.HashMap<String, String>();
        if (stream == null) return out;
        try (stream) {
            for (String row : new String(stream.readAllBytes(), StandardCharsets.UTF_8).split("\n")) {
                if (row.isBlank() || row.startsWith("#")) continue;
                String[] cells = row.split("\t");
                if (cells.length > 1) out.put(cells[0], cells[1]);
            }
        }
        return out;
    }

    /** One route's boot line; `partial` is null for a route without a gate, `status` null for one with no deferral or verdict. */
    static String line(String key, boolean body, boolean live, boolean inert, Integer partial, String status, java.util.Collection<String> mods) {
        String open = status == null ? "UNGATED" : status.startsWith("verdict: ") ? status.substring("verdict: ".length()) : "deferred (" + status + ")";
        String gate = inert ? "inert (guard)" : partial == null ? open : partial == 0 ? "gated" : "gated (partial: " + partial + " sites)";
        return "Horizon: route " + key + " " + (body ? "body" : "edge") + " " + (live ? "on" : "off") + " " + gate + " (" + String.join(", ", mods) + ")";
    }

    /** Whether a route's method is a constructor or static initialiser: Rust never calls one on an object it holds, so such a route is never bound. */
    static boolean constructor(String name) {
        return name.equals("<init>") || name.equals("<clinit>");
    }

    /** A constructor route's boot line: who runs its patches (`construct` in routing.json, null from an older analysis) and why. */
    static String constructorLine(String key, JsonObject construct, boolean applied, java.util.Collection<String> mods) {
        String by = construct == null ? "deferred (no decision in the analysis)" : construct.get("by").getAsString() + " (" + construct.get("reason").getAsString() + ")";
        return "Horizon: route " + key + " constructor " + (applied ? "applied" : "not applied") + ", " + by + " (" + String.join(", ", mods) + ")";
    }

    /** The loud line for patches that are on but have no gate; the items are `mod: Class.method`. */
    static String summary(List<String> ungated) {
        return "Horizon: " + ungated.size() + " behaviours mods change won't take effect (native runs them without the mods' patches): " + String.join(", ", ungated);
    }

    /** The quiet line for patches whose native gate another workstream owns; the items are deferral reasons, counted by owner. */
    static String waiting(List<String> reasons) {
        var counts = new TreeMap<String, Integer>(java.util.Comparator.comparingInt(Routing::ownerOrder).thenComparing(s -> s));
        for (String reason : reasons) counts.merge(ownerOf(reason), 1, Integer::sum);
        var parts = new ArrayList<String>();
        counts.forEach((who, n) -> parts.add(who + " " + n));
        return "Horizon: " + reasons.size() + " routes wait for their owner: " + String.join(", ", parts);
    }

    /** `W9` for `owner: W9 3-2`, `w12-coverage` for `spec: w12-coverage SPEC-6`, else the reason's kind; as tools/java_routing.py groups them. */
    static String ownerOf(String reason) {
        if (reason.startsWith("owner: ")) return reason.substring("owner: ".length()).split(" ")[0];
        if (reason.startsWith("spec: w12-coverage ")) return "w12-coverage";
        int colon = reason.indexOf(':');
        return colon < 0 ? reason : reason.substring(0, colon);
    }

    private static int ownerOrder(String who) {
        return who.matches("W\\d+") ? Integer.parseInt(who.substring(1)) : Integer.MAX_VALUE;
    }

    private static String simple(String owner) {
        return owner.substring(owner.lastIndexOf('/') + 1);
    }

    /** Whether the mixin's handler is merged into the target: Mixin's own applied list is empty by the time the boot looks, but the merged method names its mixin. A predicted mixin that is not applied is off, not an error: plugins, require = 0 and unloaded targets leave mixins out. */
    static boolean applied(List<String[]> merged, String mixin) {
        return merged.stream().anyMatch(m -> mixin.equals(m[1]));
    }

    /** A merged handler as it stands in the target class: the annotation Mixin kept names what it injects into and where. */
    record Applied(String name, String mixin, String kind, List<String> at, boolean argsOnly, int params, List<String> targets, List<String> atTargets) {}

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
                    var atTargets = new ArrayList<String>();
                    var raw = a.annotationType().getMethod("at").invoke(a);
                    for (Object one : raw instanceof Object[] many ? many : new Object[] {raw}) {
                        at.add((String) one.getClass().getMethod("value").invoke(one));
                        atTargets.add((String) one.getClass().getMethod("target").invoke(one));
                    }
                    boolean argsOnly = false;
                    try {
                        argsOnly = (boolean) a.annotationType().getMethod("argsOnly").invoke(a);
                    } catch (NoSuchMethodException none) {
                        // only ModifyVariable has it
                    }
                    out.add(new Applied(m.getName(), from.mixin(), kind, at, argsOnly, m.getParameterCount(), List.of(names), atTargets));
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

    static boolean targets(String target, String name, String desc) {
        String bare = target.substring(target.indexOf(';') + 1);
        return bare.contains("(") ? bare.equals(name + desc) : bare.equals(name);
    }

    static MethodHandle handle(Class<?> owner, String key, String name, String desc, boolean isStatic, ClassLoader loader) throws Exception {
        if (constructor(name)) throw new IllegalStateException("route " + key + ": a constructor is never bound");
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
        run(KEYS[id], PLANS[id], STATICS[id], HANDLES[id], in, out);
    }

    private static MethodHandle noop;

    static void noop() {}

    /** `route` against a static no-op, for the crossing bench. */
    static void benchCall(Crossing.In in, Crossing.Out out) throws Exception {
        in.readInt();
        if (noop == null) noop = MethodHandles.lookup().findStatic(Routing.class, "noop", MethodType.methodType(void.class));
        run("bench#noop()V", plan("bench#noop()V", Routing.class.getDeclaredMethod("noop"), false), true, noop, in, out);
    }

    /** What one argument's bytes build: the call's state is for the readers that need the acting player or a level. */
    @FunctionalInterface
    interface ArgReader {
        Object read(Crossing.In in, Args call) throws Exception;
    }

    /** What a return value writes. */
    @FunctionalInterface
    interface RetWriter {
        void write(Object value, Crossing.Out out) throws Exception;
    }

    /** One call's state while its arguments are read. */
    static final class Args {
        final String key;
        ServerPlayer filled;
        ServerLevel level;
        final List<TagValueOutput> outputs = new ArrayList<>();

        Args(String key) {
            this.key = key;
        }

        ServerLevel level() {
            return level != null ? level : GameBridge.level(0);
        }
    }

    /** Where a player receiver's owner class takes its receiver from. */
    enum Proj { PLAYER, INVENTORY, MENU, CONNECTION, FOOD }

    /** A route's readers and writer, built once at confirm; `unreadable` names the first type with none. */
    record Plan(ArgReader[] args, RetWriter ret, Proj proj, Class<?> owner, int outputs, String unreadable) {}

    private static final java.util.Set<Class<?>> LEVELS = java.util.Set.of(Level.class, ServerLevel.class, LevelReader.class, LevelAccessor.class, ServerLevelAccessor.class, BlockGetter.class);

    private static ArgReader entity(Class<?> type) {
        return (in, c) -> {
            var e = Flat.toEntity(in.readInt());
            if (e != null && !type.isInstance(e)) throw new ClassCastException("route " + c.key + ": entity " + e.getId() + " is a " + e.getClass().getName() + ", not a " + type.getName());
            return e;
        };
    }

    /** The reader for a parameter of this class, or null when there is none. */
    static ArgReader reader(Class<?> type) {
        if (type == int.class) return (in, c) -> in.readInt();
        if (type == long.class) return (in, c) -> in.readLong();
        if (type == float.class) return (in, c) -> in.readFloat();
        if (type == double.class) return (in, c) -> in.readDouble();
        if (type == boolean.class) return (in, c) -> in.readBoolean();
        if (type == BlockPos.class) return (in, c) -> BlockPos.of(in.readLong());
        if (type == String.class) return (in, c) -> in.text();
        if (type == ItemStack.class || type == ItemInstance.class) return (in, c) -> GameBridge.decode(in.readNBytes(in.readInt()));
        if (type == Component.class) return (in, c) -> Flat.toComponent(in.readNBytes(in.readInt()));
        if (type == Player.class || type == ServerPlayer.class) {
            return (in, c) -> {
                var call = GameBridge.fill(in, false);
                c.level = call.level();
                return c.filled = call.player();
            };
        }
        if (type == Vec3.class) return (in, c) -> new Vec3(in.readDouble(), in.readDouble(), in.readDouble());
        if (Entity.class.isAssignableFrom(type)) return entity(type);
        if (type == DamageSource.class) return (in, c) -> Flat.toDamageSource(in.readNBytes(in.readInt()), c.level());
        if (type == MobEffectInstance.class) {
            return (in, c) -> {
                var id = in.text();
                var holder = BuiltInRegistries.MOB_EFFECT.get(Identifier.parse(id)).orElseThrow(() -> new IllegalStateException("route " + c.key + ": no effect " + id));
                return new MobEffectInstance(holder, in.readInt(), in.readInt(), in.readBoolean(), in.readBoolean(), in.readBoolean());
            };
        }
        if (type == Holder.class) return (in, c) -> Flat.toHolder(in.text());
        if (type == ResourceKey.class) return (in, c) -> ResourceKey.create(ResourceKey.createRegistryKey(Identifier.parse(in.text())), Identifier.parse(in.text()));
        if (LEVELS.contains(type)) return (in, c) -> GameBridge.level(in.text());
        if (type == TeleportTransition.class) return (in, c) -> Flat.toTeleportTransition(in.readNBytes(in.readInt()));
        if (type == ValueInput.class) {
            return (in, c) -> {
                var tag = NbtIo.read(new java.io.DataInputStream(new java.io.ByteArrayInputStream(in.readNBytes(in.readInt()))), NbtAccounter.unlimitedHeap());
                return TagValueInput.create(ProblemReporter.DISCARDING, GameBridge.registries(), tag);
            };
        }
        if (type == ValueOutput.class) {
            return (in, c) -> {
                var output = TagValueOutput.createWithContext(ProblemReporter.DISCARDING, GameBridge.registries());
                c.outputs.add(output);
                return output;
            };
        }
        return null;
    }

    /** The writer for a return type, or null when there is none. */
    static RetWriter writer(Type type) {
        if (type == void.class) return (v, o) -> {};
        if (type == boolean.class || type == Boolean.class) return (v, o) -> o.writeBoolean((Boolean) v);
        if (type == int.class || type == Integer.class) return (v, o) -> o.writeInt((Integer) v);
        if (type == long.class || type == Long.class) return (v, o) -> o.writeLong((Long) v);
        if (type == float.class || type == Float.class) return (v, o) -> o.writeFloat((Float) v);
        if (type == double.class || type == Double.class) return (v, o) -> o.writeDouble((Double) v);
        if (type == Vec3.class) {
            return (v, o) -> {
                var p = (Vec3) v;
                o.writeDouble(p.x);
                o.writeDouble(p.y);
                o.writeDouble(p.z);
            };
        }
        if (type == ItemStack.class) {
            return (v, o) -> {
                byte[] bytes = Flat.raw((ItemStack) v);
                o.writeInt(bytes.length);
                o.write(bytes);
            };
        }
        if (type == String.class) return (v, o) -> GameBridge.writeText(o, (String) v);
        if (type == ResourceKey.class) return (v, o) -> GameBridge.writeText(o, ((ResourceKey<?>) v).identifier().toString());
        if (type == InteractionResult.class) {
            return (v, o) -> {
                int kind = v instanceof InteractionResult.Success ? 0 : v instanceof InteractionResult.Fail ? 1 : v instanceof InteractionResult.Pass ? 2 : 3;
                o.writeInt(kind);
                o.writeInt(v instanceof InteractionResult.Success s ? s.swingSource().ordinal() : 0);
            };
        }
        if (type instanceof ParameterizedType p && p.getRawType() == Optional.class) {
            var inner = writer(p.getActualTypeArguments()[0]);
            if (inner == null) return null;
            return (v, o) -> {
                var present = (Optional<?>) v;
                o.writeBoolean(present.isPresent());
                if (present.isPresent()) inner.write(present.get(), o);
            };
        }
        return null;
    }

    /** Which part of the player a player-filled call hands the method: the owner's class picks it. */
    static Proj projection(Class<?> owner) {
        if (owner == Inventory.class) return Proj.INVENTORY;
        if (AbstractContainerMenu.class.isAssignableFrom(owner)) return Proj.MENU;
        if (owner == ServerGamePacketListenerImpl.class || owner == ServerCommonPacketListenerImpl.class) return Proj.CONNECTION;
        if (owner == FoodData.class) return Proj.FOOD;
        return Proj.PLAYER;
    }

    static Object project(Proj proj, Class<?> owner, ServerPlayer player, String key) {
        return switch (proj) {
            case INVENTORY -> player.getInventory();
            case MENU -> {
                var menu = player.containerMenu;
                if (!owner.isInstance(menu)) throw new IllegalStateException("route " + key + ": the open menu is a " + (menu == null ? "null" : menu.getClass().getName()) + ", not a " + owner.getName());
                yield menu;
            }
            case CONNECTION -> player.connection;
            case FOOD -> player.getFoodData();
            case PLAYER -> player;
        };
    }

    private static Method declared(Class<?> type, String name, String desc) {
        for (Method m : type.getDeclaredMethods()) {
            if (m.getName().equals(name) && MethodType.methodType(m.getReturnType(), m.getParameterTypes()).toMethodDescriptorString().equals(desc)) return m;
        }
        return null;
    }

    /** The table of one route: a gated route that has no reader for a type fails here, at boot. */
    static Plan plan(String key, Method method, boolean gated) {
        var types = method.getParameterTypes();
        var readers = new ArgReader[types.length];
        String unreadable = null;
        int outputs = 0;
        for (int i = 0; i < types.length; i++) {
            readers[i] = reader(types[i]);
            if (readers[i] == null && unreadable == null) unreadable = types[i].getName();
            if (types[i] == ValueOutput.class) outputs++;
        }
        var ret = writer(method.getGenericReturnType());
        if (ret == null && unreadable == null) unreadable = "return type " + method.getGenericReturnType().getTypeName();
        if (unreadable != null && gated) throw new IllegalStateException("route " + key + ": no argument reader for " + unreadable);
        var owner = method.getDeclaringClass();
        return new Plan(readers, ret, projection(owner), owner, outputs, unreadable);
    }

    private static void run(String key, Plan plan, boolean isStatic, MethodHandle handle, Crossing.In in, Crossing.Out out) throws Exception {
        byte recv = in.readByte();
        if (plan == null || plan.unreadable() != null || handle == null) {
            String why = plan == null ? "its method" : plan.unreadable();
            if (TOLD.add(key)) System.err.println("Horizon: route " + key + ": no argument reader for " + why);
            out.writeByte(1);
            GameBridge.writeText(out, "unknown");
            GameBridge.writeText(out, "no argument reader for " + why);
            return;
        }
        Object receiver = null;
        var call = new Args(key);
        switch (recv) {
            case 0 -> {
                var filled = GameBridge.fill(in, false);
                call.filled = filled.player();
                call.level = filled.level();
                receiver = project(plan.proj(), plan.owner(), call.filled, key);
            }
            case 1 -> {
                var level = GameBridge.level(in.readInt());
                ((ViewLevelData) level.getLevelData()).gameTime = in.readLong();
                call.level = level;
                receiver = level;
            }
            case 3 -> receiver = GameBridge.server();
            case 4 -> receiver = GameBridge.server().getScoreboard();
            case 5 -> {
                var level = GameBridge.level(in.text());
                ((ViewLevelData) level.getLevelData()).gameTime = in.readLong();
                call.level = level;
                var at = BlockPos.of(in.readLong());
                receiver = level.getBlockEntity(at);
                if (receiver == null || !plan.owner().isInstance(receiver)) throw new IllegalStateException("route " + key + ": no " + plan.owner().getName() + " at " + at + (receiver == null ? "" : ", found a " + receiver.getClass().getName()));
            }
            default -> {}
        }
        var args = new ArrayList<Object>();
        if (!isStatic) args.add(receiver);
        for (var reader : plan.args()) args.add(reader.read(in, call));
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
        plan.ret().write(result, out);
        for (var output : call.outputs) GameBridge.writeTag(out, output.buildResult());
        if (call.filled != null) PlayerSync.writeChanges(call.filled, out);
        else out.writeInt(-1);
        GameBridge.writeEffects(out);
    }

    /** Whether a mod's mixin changes this method and the route is on; W10's seams ask. */
    public static boolean patched(String key) {
        return route(key) >= 0;
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

    /** A live route's admission: its family's receivers narrowed by the guards' receiver classes, and the guards' items and block states. */
    static RouteGuards.Admission admission(String family, Class<?> owner, String ownerName, String name, String desc, JsonArray overriders, JsonElement guards, ClassLoader loader) {
        var parsed = RouteGuards.parse(guards, loader);
        return RouteGuards.admission(family, owner == null ? null : admitted(family, owner, ownerName, name, desc, overriders, parsed.recv()), parsed, RouteGuards::registeredItems);
    }

    /** One route's admission in `routes.bin`: receivers (-1 any, -2 pending, -3 none, else the names), items (-1 any, else the ids), block states (-1 any, else Java's ids). */
    static void write(DataOutputStream out, RouteGuards.Admission admission, String family) throws IOException {
        if (admission == null || admission.inert()) {
            out.writeInt(admission == null ? -1 : -3);
            out.writeInt(-1);
            out.writeInt(-1);
            return;
        }
        if (admission.receivers() == null) {
            out.writeInt(family.equals("entity") || family.equals("block_entity") ? -2 : -1);
        } else {
            out.writeInt(admission.receivers().size());
            for (String a : admission.receivers()) text(out, a);
        }
        if (admission.items() == null) {
            out.writeInt(-1);
        } else {
            out.writeInt(admission.items().size());
            for (String item : admission.items()) text(out, item);
        }
        if (admission.blocks() == null) {
            out.writeInt(-1);
        } else {
            out.writeInt(admission.blocks().length);
            for (int state : admission.blocks()) out.writeInt(state);
        }
    }

    /** The registry names the route's family admits, or null for a family without a list (all of it, or decided by the spec that adds its receivers); `recv` keeps only those of its classes. */
    private static List<String> admitted(String family, Class<?> owner, String ownerName, String name, String desc, JsonArray overriders, List<Class<?>> recv) {
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
            var member = entry.getValue().getClass();
            if (reaches(member, ownerName, name, desc, overriders) && (recv == null || RouteGuards.matches(recv, member))) out.add(entry.getKey().identifier().toString());
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
