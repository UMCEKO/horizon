package dev.horizonlab.javamods.boot;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.Map;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.block.entity.BlockEntityType;

/** `HORIZON_PROFILE`: per-second time of mods' block entity tickers, the wait for the server side's turn (`lock=`), its hold (`hold=`), game bus posts by event class (`posts=`), and JFR dumps. */
final class Profile {
    static final boolean ON = System.getenv("HORIZON_PROFILE") != null && !System.getenv("HORIZON_PROFILE").isEmpty();
    private static final int TOP = 15;
    private static final IdentityHashMap<BlockEntityType<?>, long[]> TICKS = new IdentityHashMap<>();
    private static final HashMap<String, long[]> WAITS = new HashMap<>();
    private static final HashMap<String, long[]> EVENTS = new HashMap<>();
    private static final HashMap<String, long[]> HOLDS = new HashMap<>();
    private static final IdentityHashMap<Class<?>, long[]> POSTS = new IdentityHashMap<>();

    private Profile() {}

    private static void add(long[] stat, long ns) {
        stat[0]++;
        stat[1] += ns;
        stat[2] = Math.max(stat[2], ns);
    }

    static synchronized void tick(BlockEntityType<?> type, long ns) {
        add(TICKS.computeIfAbsent(type, k -> new long[3]), ns);
    }

    static synchronized void event(String what, long ns) {
        add(EVENTS.computeIfAbsent(what, k -> new long[3]), ns);
    }

    /** One post on the game bus of an event of `type`, its listeners' time included (`CompiledEvents`). */
    static synchronized void post(Class<?> type, long ns) {
        add(POSTS.computeIfAbsent(type, k -> new long[3]), ns);
    }

    static synchronized void wait(String op, long ns) {
        add(WAITS.computeIfAbsent(op, k -> new long[3]), ns);
    }

    /** The server side's turn held by an outermost `op`, nested ops included. */
    static synchronized void hold(String op, long ns) {
        add(HOLDS.computeIfAbsent(op, k -> new long[3]), ns);
    }

    private static String rows(Map<String, long[]> stats) {
        var all = new ArrayList<>(stats.entrySet());
        all.sort((a, b) -> Long.compare(b.getValue()[1], a.getValue()[1]));
        var out = new StringBuilder();
        for (var e : all.subList(0, Math.min(TOP, all.size()))) {
            long[] s = e.getValue();
            if (!out.isEmpty()) out.append(',');
            out.append(e.getKey()).append('|').append(s[0]).append('|').append(s[1] / 1000).append('|').append(s[2] / 1000);
        }
        return out.isEmpty() ? "-" : out.toString();
    }

    /** `be=... lock=... ev=... hold=...` for the time since the last take, in the rows Rust's `profile::format_stats` writes. */
    static synchronized String take() {
        var ticks = new HashMap<String, long[]>();
        TICKS.forEach((type, s) -> ticks.merge(String.valueOf(BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(type)), s, (a, b) -> {
            a[0] += b[0];
            a[1] += b[1];
            a[2] = Math.max(a[2], b[2]);
            return a;
        }));
        var posts = new HashMap<String, long[]>();
        POSTS.forEach((type, s) -> posts.put(type.getName().substring(type.getName().lastIndexOf('.') + 1), s));
        String line = "be=" + rows(ticks) + " lock=" + rows(WAITS) + " ev=" + rows(EVENTS) + " hold=" + rows(HOLDS) + " posts=" + rows(posts) + " " + runtime();
        TICKS.clear();
        WAITS.clear();
        EVENTS.clear();
        HOLDS.clear();
        POSTS.clear();
        return line;
    }

    /** The JVM's own costs, apart from the bridge's objects: loaded classes, metaspace and code cache, direct buffers, and the largest reply buffer. */
    static String runtime() {
        long meta = 0, classSpace = 0, code = 0, direct = 0;
        for (var pool : java.lang.management.ManagementFactory.getMemoryPoolMXBeans()) {
            long used = pool.getUsage().getUsed();
            switch (pool.getName()) {
                case "Metaspace" -> meta = used;
                case "Compressed Class Space" -> classSpace = used;
                default -> {
                    if (pool.getName().startsWith("CodeHeap") || pool.getName().equals("Code Cache")) code += used;
                }
            }
        }
        for (var pool : java.lang.management.ManagementFactory.getPlatformMXBeans(java.lang.management.BufferPoolMXBean.class)) {
            if (pool.getName().equals("direct")) direct = pool.getTotalCapacity();
        }
        int classes = java.lang.management.ManagementFactory.getClassLoadingMXBean().getLoadedClassCount();
        return "jvm_classes=" + classes + " jvm_meta_kb=" + meta / 1024 + " jvm_class_kb=" + classSpace / 1024 + " jvm_code_kb=" + code / 1024
            + " direct_kb=" + direct / 1024 + " reply_high_kb=" + Crossing.takeHigh() / 1024;
    }

    /** Writes the running recording, `Recording.dump`: the JVM keeps recording. */
    static String dump(String path) throws Exception {
        for (var recording : jdk.jfr.FlightRecorder.getFlightRecorder().getRecordings()) {
            if (recording.getName().equals("horizon")) {
                recording.dump(java.nio.file.Path.of(path));
                return path;
            }
        }
        return "";
    }
}
