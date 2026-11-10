package dev.horizonlab.javamods.host;

import java.lang.invoke.MethodHandle;
import java.nio.ByteBuffer;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.function.BiFunction;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.fml.startup.Entrypoint;

/** App-classpath side: starts FML as Server.main does and stops before a MinecraftServer exists. */
public final class Host extends Entrypoint {
    public static final Map<String, Long> MARKS = new LinkedHashMap<>();
    public static volatile BiFunction<String, String, String> entry;
    public static volatile Dispatcher dispatcher;
    public static volatile ClassLoader gameLoader;
    public static volatile ThreadGroup serverGroup;
    /** The methods the class processor serves from Rust, set before FML starts. */
    public static volatile String[] bridges = new String[0];
    static long t0;

    /** Rust's requests, as the boot sets them: an int op and a direct buffer holding the request, which the reply replaces. */
    public interface Dispatcher {
        /** The reply's length, written to the buffer's start; 0 for none, -1 failed, -2 too big for the buffer (then `overflow`). */
        int dispatch(int op, ByteBuffer arena, int length);

        byte[] overflow();

        /** The ops in index order. */
        String[] ops();
    }

    private Host() {}

    // natives are registered by the Rust host on this class
    public static native void onEvent(String what);
    public static native int shapeFlags(double[] x, double[] y, double[] z, boolean[] occupied);
    public static native long shapeHandle(double[] x, double[] y, double[] z, boolean[] occupied);
    public static native void shapeFree(long handle);
    public static native boolean earlyNative();
    public static native void link(Class<?> target);
    /** The crossing bench's reentrancy native, registered by the bench alone. */
    public static native int benchBack(int tag);

    public static void publish(String[] table) {
        bridges = table;
    }

    public static void mark(String name) {
        MARKS.put(name, (System.nanoTime() - t0) / 1000);
    }

    public static String marks() {
        StringBuilder b = new StringBuilder();
        MARKS.forEach((k, v) -> b.append(k).append('=').append(v / 1000.0).append("ms "));
        return b.toString();
    }

    public static void run(String[] args, boolean client) throws Throwable {
        t0 = System.nanoTime();
        var result = startup(args, true, client ? Dist.CLIENT : Dist.DEDICATED_SERVER, true);
        mark("fml_startup");
        MethodHandle main = createMainMethodCallable(result, "dev.horizonlab.javamods.boot.Boot");
        main.invokeExact(new String[0]);
        mark("boot_returned");
    }

    /** Rust's request: the op is an index into `ops()`. */
    public static int dispatch(int op, ByteBuffer arena, int length) {
        useGameLoader();
        return dispatcher.dispatch(op, arena, length);
    }

    /** A buffer the Rust thread that asked fills with requests and reads replies from; Java owns its memory. */
    public static ByteBuffer arena(int capacity) {
        return ByteBuffer.allocateDirect(capacity);
    }

    public static String[] ops() {
        return dispatcher.ops();
    }

    public static byte[] overflow() {
        return dispatcher.overflow();
    }

    /** A thread attached from Rust has no context class loader, and FML's bytecode lookups and mods' services read it. */
    private static void useGameLoader() {
        Thread thread = Thread.currentThread();
        if (thread.getContextClassLoader() == null) thread.setContextClassLoader(gameLoader);
    }

    public static String trace(Throwable t) {
        var out = new java.io.StringWriter();
        t.printStackTrace(new java.io.PrintWriter(out));
        return out.toString();
    }

    public static String call(String op, String arg) {
        useGameLoader();
        if (op.equals("marks")) return marks();
        return entry.apply(op, arg);
    }
}
