package dev.horizonlab.javamods.host;

import java.lang.invoke.MethodHandle;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.function.BiFunction;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.fml.startup.Entrypoint;

/** App-classpath side: starts FML as Server.main does and stops before a MinecraftServer exists. */
public final class Host extends Entrypoint {
    public static final Map<String, Long> MARKS = new LinkedHashMap<>();
    public static volatile BiFunction<String, String, String> entry;
    static long t0;

    private Host() {}

    // natives are registered by the Rust host on this class
    public static native void onEvent(String what);
    public static native int shapeFlags(double[] x, double[] y, double[] z, boolean[] occupied);

    public static void mark(String name) {
        MARKS.put(name, (System.nanoTime() - t0) / 1000);
    }

    public static String marks() {
        StringBuilder b = new StringBuilder();
        MARKS.forEach((k, v) -> b.append(k).append('=').append(v / 1000.0).append("ms "));
        return b.toString();
    }

    public static void run(String[] args) throws Throwable {
        t0 = System.nanoTime();
        var result = startup(args, true, Dist.DEDICATED_SERVER, true);
        mark("fml_startup");
        MethodHandle main = createMainMethodCallable(result, "dev.horizonlab.javamods.boot.Boot");
        main.invokeExact(new String[0]);
        mark("boot_returned");
    }

    public static String call(String op, String arg) {
        if (op.equals("marks")) return marks();
        return entry.apply(op, arg);
    }
}
