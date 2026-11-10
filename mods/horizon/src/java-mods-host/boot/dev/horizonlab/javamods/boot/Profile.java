package dev.horizonlab.javamods.boot;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.Map;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.block.entity.BlockEntityType;

/** `HORIZON_PROFILE`: per-second time of mods' block entity tickers and of the wait for `GameBridge`'s lock, and JFR dumps. */
final class Profile {
    static final boolean ON = System.getenv("HORIZON_PROFILE") != null && !System.getenv("HORIZON_PROFILE").isEmpty();
    private static final int TOP = 15;
    private static final IdentityHashMap<BlockEntityType<?>, long[]> TICKS = new IdentityHashMap<>();
    private static final HashMap<String, long[]> WAITS = new HashMap<>();
    private static final HashMap<String, long[]> EVENTS = new HashMap<>();

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

    static synchronized void wait(String op, long ns) {
        add(WAITS.computeIfAbsent(op, k -> new long[3]), ns);
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

    /** `be=... lock=... ev=...` for the time since the last take, in the rows Rust's `profile::format_stats` writes. */
    static synchronized String take() {
        var ticks = new HashMap<String, long[]>();
        TICKS.forEach((type, s) -> ticks.merge(String.valueOf(BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(type)), s, (a, b) -> {
            a[0] += b[0];
            a[1] += b[1];
            a[2] = Math.max(a[2], b[2]);
            return a;
        }));
        String line = "be=" + rows(ticks) + " lock=" + rows(WAITS) + " ev=" + rows(EVENTS);
        TICKS.clear();
        WAITS.clear();
        EVENTS.clear();
        return line;
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
