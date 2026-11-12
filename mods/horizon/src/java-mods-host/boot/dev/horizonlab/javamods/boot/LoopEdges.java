package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Map;
import java.util.regex.Pattern;
import net.minecraft.client.Minecraft;
import net.minecraft.client.main.GameConfig;
import net.minecraft.sounds.SoundSource;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.transformer.meta.MixinMerged;

/** Mods' handlers on `Minecraft`'s constructor, `run` and `runTick`, which Horizon never runs: each runs at vanilla's moment. */
final class LoopEdges {
    private LoopEdges() {}

    static final String PAUSE = "Lnet/minecraft/client/sounds/SoundManager;pauseAllExcept([Lnet/minecraft/sounds/SoundSource;)V";
    private static final String[][] LOOP = {{"<init>", "(Lnet/minecraft/client/main/GameConfig;)V"}, {"run", "()V"}, {"runTick", "(Z)V"}};
    // Horizon's own mixins; the test mod's are a mod's like any other.
    private static final String OWN = "dev.horizonlab.javamods.boot.";
    private static final Pattern COUNTER = Pattern.compile("\\$zz([a-z])(\\d+)\\$");

    /** One merged handler on a loop method: its injection, the method to call and its `@MixinMerged` priority. */
    record Handler(Routing.Applied shape, Method method, int priority, String loop) {}

    private static volatile Minecraft minecraft;
    private static volatile List<Handler> pauses = List.of();
    private static volatile boolean filteredTold;

    /** Right after the constructor's steps, as `Main` calls `run()` once the constructor returns: its RETURN/TAIL handlers, then `run`'s HEAD ones. */
    static void start(Minecraft minecraft, GameConfig config) {
        LoopEdges.minecraft = minecraft;
        var mods = mods();
        var inits = new ArrayList<Handler>();
        var runs = new ArrayList<Handler>();
        var pause = new ArrayList<Handler>();
        for (var h : handlers(Minecraft.class)) {
            var s = h.shape();
            boolean inject = s.kind().equals("inject");
            if (h.loop().equals("<init>") && inject && !s.at().isEmpty() && s.at().stream().allMatch(a -> a.equals("RETURN") || a.equals("TAIL"))) inits.add(h);
            else if (h.loop().equals("run") && inject && !s.at().isEmpty() && s.at().stream().allMatch(a -> a.equals("HEAD")) && h.method().getParameterCount() == 1) runs.add(h);
            else if (h.loop().equals("runTick") && s.kind().equals("modify_arg") && s.atTargets().contains(PAUSE) && h.method().getParameterCount() == 1) pause.add(h);
            else System.err.println(unrouted(mods.getOrDefault(s.mixin(), s.mixin()), s.kind(), h.loop(), String.join(",", s.at())));
        }
        for (Method m : Minecraft.class.getDeclaredMethods()) {
            var from = m.getAnnotation(MixinMerged.class);
            if (from == null || from.mixin().startsWith(OWN)) continue;
            String desc = java.lang.invoke.MethodType.methodType(m.getReturnType(), m.getParameterTypes()).toMethodDescriptorString();
            if ((m.getName().equals("run") && desc.equals("()V")) || (m.getName().equals("runTick") && desc.equals("(Z)V"))) {
                System.err.println(unrouted(mods.getOrDefault(from.mixin(), from.mixin()), "overwrite", m.getName(), "-"));
            }
        }
        pauses = List.copyOf(order(pause));
        for (var h : order(inits)) {
            var info = new CallbackInfo("<init>", false);
            Object[] have = {config, info};
            Object[] args = Arrays.stream(h.method().getParameterTypes()).map(type -> Inits.argument(type, have)).toArray();
            GameBridge.safely("Minecraft#<init> handler " + h.shape().mixin(), () -> invoke(h, args));
            System.err.println("Horizon: ran " + mods.getOrDefault(h.shape().mixin(), h.shape().mixin()) + " " + h.shape().mixin() + "." + h.method().getName() + " on Minecraft#<init> " + String.join(",", h.shape().at()) + " (Horizon's GameConfig)");
        }
        for (var h : order(runs)) {
            var info = new CallbackInfo("run", true);
            GameBridge.safely("Minecraft#run handler " + h.shape().mixin(), () -> invoke(h, info));
            String mod = mods.getOrDefault(h.shape().mixin(), h.shape().mixin());
            System.err.println("Horizon: ran " + mod + " " + h.shape().mixin() + "." + h.method().getName() + " on Minecraft#run HEAD");
            if (info.isCancelled()) System.err.println("Horizon: WARNING: " + mod + " " + h.shape().mixin() + " cancelled Minecraft#run: vanilla would not run the game at all; Horizon runs it");
        }
    }

    /** The `client_pause_sources` op: vanilla's `[MUSIC, UI]` through the mods' `@ModifyArg`s on `runTick`'s `pauseAllExcept`, as `int n` and n source names. */
    static byte[] pauseSources() throws IOException {
        SoundSource[][] kept = {{SoundSource.MUSIC, SoundSource.UI}};
        var handlers = pauses;
        if (handlers.isEmpty() && !filteredTold) {
            filteredTold = true;
            System.err.println("Horizon: Minecraft#runTick's route is on but no pauseAllExcept handler is merged (a config plugin filtered it): vanilla's sources keep playing");
        }
        for (var h : handlers) {
            GameBridge.safely("Minecraft#runTick handler " + h.shape().mixin(), () -> kept[0] = (SoundSource[]) invoke(h, (Object) kept[0]));
        }
        var names = new ArrayList<String>();
        for (var source : kept[0]) if (source != null) names.add(source.getName());
        System.err.println("Horizon: pause sources: " + names);
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(names.size());
        for (String name : names) GameBridge.writeText(out, name);
        return bytes.toByteArray();
    }

    static String unrouted(String mod, String kind, String method, String at) {
        return "Horizon: " + mod + " " + kind + " on Minecraft#" + method + " at " + at + ": not routed (Horizon runs no " + method + ")";
    }

    /** The mods' merged handlers on `type`'s loop methods, Horizon's own left out. */
    private static List<Handler> handlers(Class<?> type) {
        var byName = new java.util.HashMap<String, Method>();
        for (Method m : type.getDeclaredMethods()) byName.put(m.getName(), m);
        var out = new ArrayList<Handler>();
        for (var shape : Routing.shapes(type)) {
            if (shape.mixin().startsWith(OWN)) continue;
            String loop = loop(shape.targets());
            Method method = byName.get(shape.name());
            if (loop == null || method == null) continue;
            out.add(new Handler(shape, method, method.getAnnotation(MixinMerged.class).priority(), loop));
        }
        return out;
    }

    private static String loop(List<String> targets) {
        for (String target : targets) {
            for (String[] loop : LOOP) if (Routing.targets(target, loop[0], loop[1])) return loop[0];
        }
        return null;
    }

    /** By priority, then the order Mixin merged them; a pair whose order can't be told is named in a warning. */
    static List<Handler> order(List<Handler> handlers) {
        var out = new ArrayList<>(handlers);
        out.sort((a, b) -> {
            Integer c = compare(a.priority(), a.method().getName(), b.priority(), b.method().getName());
            if (c != null) return c;
            System.err.println("Horizon: WARNING: can't tell the order of " + a.shape().mixin() + "." + a.method().getName() + " and " + b.shape().mixin() + "." + b.method().getName() + " on Minecraft#" + a.loop());
            return 0;
        });
        return out;
    }

    /** Negative when the first runs first, null when neither priority nor merge counter tells them apart. */
    static Integer compare(int priority, String name, int otherPriority, String otherName) {
        if (priority != otherPriority) return Integer.compare(priority, otherPriority);
        var a = COUNTER.matcher(name);
        var b = COUNTER.matcher(otherName);
        if (!a.find() || !b.find()) return null;
        int c = a.group(1).compareTo(b.group(1));
        if (c == 0) c = Long.compare(Long.parseLong(a.group(2)), Long.parseLong(b.group(2)));
        return c == 0 ? null : c;
    }

    private static Object invoke(Handler h, Object... args) {
        try {
            h.method().setAccessible(true);
            return h.method().invoke(Modifier.isStatic(h.method().getModifiers()) ? null : minecraft, args);
        } catch (InvocationTargetException e) {
            throw e.getCause() instanceof RuntimeException r ? r : new IllegalStateException(e.getCause());
        } catch (IllegalAccessException e) {
            throw new IllegalStateException(e);
        }
    }

    /** Mixin class to mod, from the install's `routing.json`; empty without one. */
    private static Map<String, String> mods() {
        var out = new java.util.HashMap<String, String>();
        String path = System.getProperty("horizon.java.analysis");
        if (path == null || path.isEmpty()) return out;
        var file = java.nio.file.Path.of(path).resolveSibling("routing.json");
        if (!java.nio.file.Files.isRegularFile(file)) return out;
        try {
            var routes = com.google.gson.JsonParser.parseString(java.nio.file.Files.readString(file)).getAsJsonObject().getAsJsonObject("routes");
            for (var route : routes.entrySet()) {
                for (var c : route.getValue().getAsJsonObject().getAsJsonArray("changes")) out.put(c.getAsJsonObject().get("mixin").getAsString(), c.getAsJsonObject().get("mod").getAsString());
            }
        } catch (IOException | RuntimeException unreadable) {
            System.err.println("Horizon: can't read " + file + " for the mods of Minecraft's loop handlers: " + unreadable);
        }
        return out;
    }
}
