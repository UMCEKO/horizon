package dev.horizonlab.javamods.boot;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/** Which vanilla sources make a sound native also makes: the Java client's copy of those is dropped, and every other sound plays. */
public final class SoundOwners {
    private SoundOwners() {}

    /** A stack frame, reduced to what the table keys on. */
    record Frame(String cls, String method) {}

    private record Row(String method, boolean nativeOwned) {}

    private static final Map<String, List<Row>> ROWS = new HashMap<>();
    private static final Set<String> SEEN = new HashSet<>();
    private static final boolean DEBUG = System.getenv("HORIZON_JAVA_DEBUG") != null;
    private static final Set<String> PLUMBING_CLASSES = Set.of("net.minecraft.client.sounds.SoundEngine", "net.minecraft.client.sounds.SoundManager", ClientSounds.class.getName(), SoundOwners.class.getName());
    private static final Set<String> LEVELS = Set.of("net.minecraft.client.multiplayer.ClientLevel", "net.minecraft.world.level.Level");
    private static final String ENTITY = "net.minecraft.world.entity.Entity";
    private static final String LOCAL_PLAYER = "net.minecraft.client.player.LocalPlayer";

    /** Reads the table and the animate closure's block and fluid rows (native's), then notes the mods that patch a native source. */
    static void load() throws Exception {
        parse(read("/sound_sources.tsv"), read("/animate_closure.tsv"));
        var warned = new HashSet<String>();
        for (var entry : ROWS.entrySet()) {
            if (entry.getValue().stream().noneMatch(Row::nativeOwned)) continue;
            String cls = entry.getKey();
            try {
                Class.forName(cls, false, SoundOwners.class.getClassLoader());
            } catch (ClassNotFoundException | LinkageError e) {
                continue;
            }
            for (var info : org.spongepowered.asm.mixin.Mixins.getMixinsForClass(cls)) {
                String config = info.getConfig().getName();
                if (config.startsWith("horizon_java")) continue;
                String mod = config.replaceFirst("\\.mixins?\\.json$", "");
                for (String line : dev.horizonlab.javamods.parity.MixinScanner.scan(mod, config, "client", info.getClassBytes()).split("\n")) {
                    String[] f = line.split("\t", -1);
                    if (f.length < 9 || !List.of(f[6].split(",")).contains(cls)) continue;
                    for (String target : f[7].split(",")) {
                        String name = target.replaceFirst("^L[^;]+;", "").replaceFirst("\\(.*", "");
                        if (nativeOwnsRow(cls, name) && warned.add(mod + cls + name)) System.err.println("Horizon: " + mod + " patches " + cls + "#" + name + ", a sound native plays: W12 routing needed");
                    }
                }
            }
        }
    }

    private static String read(String resource) throws IOException {
        try (var in = SoundOwners.class.getResourceAsStream(resource)) {
            return new String(in.readAllBytes(), StandardCharsets.UTF_8);
        }
    }

    /** `sound_sources.tsv` (class, method or `*`, owner, why) and the closure's block and fluid rows. */
    static void parse(String sources, String closure) {
        ROWS.clear();
        for (String line : sources.split("\n")) {
            if (line.isBlank() || line.startsWith("#")) continue;
            String[] f = line.split("\t", -1);
            ROWS.computeIfAbsent(f[0].replace('/', '.'), k -> new ArrayList<>()).add(new Row(f[1], f[2].equals("native")));
        }
        for (String line : closure.split("\n")) {
            if (line.isBlank()) continue;
            String[] f = line.split("\t", -1);
            if (f[0].equals("block") || f[0].equals("fluid")) ROWS.computeIfAbsent(f[1].replace('/', '.'), k -> new ArrayList<>()).add(new Row(f[2], true));
        }
    }

    /** `lambda$tick$0` belongs to `tick`. */
    private static String sourceMethod(String method) {
        if (method.startsWith("lambda$")) {
            int end = method.lastIndexOf('$');
            if (end > 7) return method.substring(7, end);
        }
        return method;
    }

    /** The engine and the `play*` entry points, with the lambdas and mixin handlers inside them. */
    static boolean plumbing(Frame frame) {
        String method = sourceMethod(frame.method());
        if (PLUMBING_CLASSES.contains(frame.cls())) return true;
        if (LEVELS.contains(frame.cls())) return method.startsWith("playSound") || method.startsWith("handler$") || method.equals("playLocalSound") || method.equals("playSeededSound") || method.equals("playPlayerSound");
        if (frame.cls().equals(ENTITY)) return method.startsWith("playSound") || method.startsWith("handler$");
        return frame.cls().equals(LOCAL_PLAYER) && (method.equals("playSound") || method.equals("playNotifySound") || method.startsWith("handler$"));
    }

    /** The first frame, walking out from the sound, that is not plumbing: what asked for it. */
    static Frame source(Iterable<Frame> frames) {
        for (Frame frame : frames) if (!plumbing(frame)) return frame;
        return null;
    }

    private static boolean nativeOwnsRow(String cls, String method) {
        for (String c : new String[] {cls, cls.contains("$") ? cls.substring(0, cls.indexOf('$')) : cls}) {
            for (Row row : ROWS.getOrDefault(c, List.of())) if (row.method().equals("*") || row.method().equals(method)) return row.nativeOwned();
        }
        return false;
    }

    /** Whether a frame is a vanilla source native ports. A frame outside the table, and any mod's, is not. */
    static boolean nativeOwns(Frame frame) {
        return nativeOwnsRow(frame.cls(), sourceMethod(frame.method()));
    }

    /** Whether the sound being played right now comes from a source native ports. */
    static boolean nativeOwnsCaller() {
        if (ROWS.isEmpty()) return false;
        Frame source = StackWalker.getInstance().walk(frames -> frames.map(f -> new Frame(f.getClassName(), f.getMethodName())).filter(f -> !plumbing(f)).findFirst().orElse(null));
        if (source == null) return false;
        boolean owned = nativeOwns(source);
        if (owned && DEBUG && SEEN.add(source.cls() + "#" + source.method())) System.err.println("Horizon: a sound from " + source.cls() + "#" + source.method() + " is native's");
        return owned;
    }
}
