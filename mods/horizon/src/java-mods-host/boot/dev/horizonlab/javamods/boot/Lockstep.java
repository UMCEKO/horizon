package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.util.ArrayDeque;
import java.util.EnumMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.locks.Condition;
import java.util.concurrent.locks.ReentrantLock;
import java.util.function.BiFunction;
import java.util.function.Function;
import java.util.function.IntSupplier;
import java.util.function.Supplier;
import net.neoforged.bus.api.Event;
import net.neoforged.neoforge.client.event.ComputeFovModifierEvent;
import net.neoforged.neoforge.client.event.MovementInputUpdateEvent;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.living.LivingEvent;
import net.neoforged.neoforge.event.entity.living.LivingFallEvent;
import net.neoforged.neoforge.event.tick.PlayerTickEvent;

/** The Java client's tick interleaved with the window's: it parks at `handleKeybinds` (K) and at the local player's `aiStep` (M), serving the window's calls there until the window releases it. */
public final class Lockstep {
    private Lockstep() {}

    public enum Point { NONE, K, M, ENDED }

    /** The request kind of a client op by name; kinds 1–4 are added by `handler`. */
    static final byte OP = 5;

    static final List<Class<? extends Event>> EVENTS = List.of(MovementInputUpdateEvent.class, ComputeFovModifierEvent.class,
            PlayerTickEvent.Pre.class, PlayerTickEvent.Post.class, LivingEvent.LivingJumpEvent.class, LivingFallEvent.class);

    private record Request(byte kind, String op, byte[] in, CompletableFuture<byte[]> reply) {}

    private static final byte[] EMPTY = new byte[0];
    private static final boolean DEBUG = System.getenv("HORIZON_JAVA_DEBUG") != null;
    private static final Set<String> HEARD = new HashSet<>();
    private static final ReentrantLock LOCK = new ReentrantLock();
    private static final Condition CHANGED = LOCK.newCondition();
    private static final ArrayDeque<Request> INBOX = new ArrayDeque<>();
    private static final EnumMap<Point, byte[]> RELEASES = new EnumMap<>(Point.class);
    private static final Map<Byte, Function<byte[], byte[]>> HANDLERS = new ConcurrentHashMap<>();
    private static volatile Thread jobThread;
    private static boolean announced;
    private static Point parked = Point.NONE;
    private static Point passed = Point.NONE;
    private static byte[] record = EMPTY;

    /** How a client op runs on the parked thread: `ClientBridge.run`, or a check's own. */
    static volatile BiFunction<String, byte[], byte[]> ops = (op, in) -> {
        throw new IllegalStateException("no client ops: " + op);
    };

    private static final AtomicInteger PARKS_K = new AtomicInteger();
    private static final AtomicInteger PARKS_M = new AtomicInteger();
    /** Parks at K and at M so far, for checks. */
    public static final IntSupplier PARKS_AT_K = PARKS_K::get;
    public static final IntSupplier PARKS_AT_M = PARKS_M::get;
    private static volatile byte[] lastFill = EMPTY;

    static {
        // Mods' checks live in another layer: they reach these through the JDK's own types.
        var properties = System.getProperties();
        properties.put("horizon.lockstep.parks.K", PARKS_AT_K);
        properties.put("horizon.lockstep.parks.M", PARKS_AT_M);
        properties.put("horizon.lockstep.lastFill", (Supplier<byte[]>) Lockstep::lastFill);
    }

    /** The release payload of the last park at M, as the window wrote it. */
    public static byte[] lastFill() {
        return lastFill;
    }

    /** Whether the window's next tick runs in lockstep: a mod listens to a movement event of the local player's tick. */
    static boolean wanted() {
        return Listeners.modListens(NeoForge.EVENT_BUS, EVENTS, HEARD, "movement events");
    }

    static void handler(byte kind, Function<byte[], byte[]> run) {
        if (kind == OP) throw new IllegalArgumentException("kind " + kind + " is the client op");
        HANDLERS.put(kind, run);
    }

    private static void reset() {
        parked = Point.NONE;
        passed = Point.NONE;
        record = EMPTY;
        RELEASES.clear();
    }

    private static boolean inFlight() {
        return announced || jobThread != null;
    }

    /** The window is about to hand the tick job over: its calls from now on wait in the inbox, not behind the job. */
    static void announce() {
        LOCK.lock();
        try {
            reset();
            announced = true;
        } finally {
            LOCK.unlock();
        }
    }

    /** The tick job starts on this thread. */
    static void begin() {
        LOCK.lock();
        try {
            if (!announced) reset();
            jobThread = Thread.currentThread();
        } finally {
            LOCK.unlock();
        }
    }

    public static boolean active() {
        return Thread.currentThread() == jobThread;
    }

    private static void serve(Request request) {
        try {
            byte[] reply;
            if (request.kind == OP) {
                reply = ops.apply(request.op, request.in);
            } else {
                var run = HANDLERS.get(request.kind);
                if (run == null) throw new IllegalArgumentException("lockstep request kind " + request.kind);
                reply = run.apply(request.in);
            }
            request.reply.complete(reply);
        } catch (Throwable t) {
            request.reply.completeExceptionally(t);
        }
    }

    /** Publishes the park, serves the window's calls until it releases this point, and answers the release's payload. */
    public static byte[] park(Point point, byte[] parkRecord) {
        (point == Point.K ? PARKS_K : PARKS_M).incrementAndGet();
        if (DEBUG) System.err.println("Horizon: lockstep " + point);
        LOCK.lock();
        try {
            parked = point;
            if (passed.ordinal() < point.ordinal()) passed = point;
            record = parkRecord;
            CHANGED.signalAll();
            while (true) {
                byte[] payload = RELEASES.remove(point);
                if (payload != null) {
                    parked = Point.NONE;
                    record = EMPTY;
                    if (point == Point.M) lastFill = payload;
                    return payload;
                }
                Request next = INBOX.poll();
                if (next == null) {
                    CHANGED.awaitUninterruptibly();
                    continue;
                }
                LOCK.unlock();
                try {
                    serve(next);
                } finally {
                    LOCK.lock();
                }
            }
        } finally {
            LOCK.unlock();
        }
    }

    /** Blocks until the job parked at `point` or went past it; `bool parkedHere`, `int length` and the park's record. */
    static byte[] waitFor(Point point) {
        boolean here;
        byte[] parkRecord;
        LOCK.lock();
        try {
            while (inFlight() && passed.ordinal() < point.ordinal() && parked != Point.ENDED) CHANGED.awaitUninterruptibly();
            here = parked == point;
            parkRecord = here ? record : EMPTY;
        } finally {
            LOCK.unlock();
        }
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        try {
            out.writeBoolean(here);
            out.writeInt(parkRecord.length);
            out.write(parkRecord);
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        return bytes.toByteArray();
    }

    /** A request of `kind` run on the parked thread, waiting for its reply; null when no lockstep tick is in flight. */
    static byte[] call(byte kind, byte[] in) throws ExecutionException, InterruptedException {
        var reply = new CompletableFuture<byte[]>();
        LOCK.lock();
        try {
            if (!inFlight()) return null;
            INBOX.add(new Request(kind, null, in, reply));
            CHANGED.signalAll();
        } finally {
            LOCK.unlock();
        }
        return reply.get();
    }

    /** Lets the job go on from `point` with `payload`; a release for a point the job already left is dropped, one ahead of it waits there. */
    static void release(Point point, byte[] payload) {
        LOCK.lock();
        try {
            if (parked != point && (passed.ordinal() > point.ordinal() || parked == Point.ENDED)) return;
            RELEASES.put(point, payload);
            CHANGED.signalAll();
        } finally {
            LOCK.unlock();
        }
    }

    /** Whether a release waits for the job to reach `point`, for checks. */
    static boolean pending(Point point) {
        LOCK.lock();
        try {
            return RELEASES.containsKey(point);
        } finally {
            LOCK.unlock();
        }
    }

    /** The job's end: what is left in the inbox is served here, then every waiter goes on. */
    static void end() {
        while (true) {
            Request next;
            LOCK.lock();
            try {
                next = INBOX.poll();
                if (next == null) {
                    parked = Point.ENDED;
                    passed = Point.ENDED;
                    record = EMPTY;
                    RELEASES.clear();
                    jobThread = null;
                    announced = false;
                    CHANGED.signalAll();
                    break;
                }
            } finally {
                LOCK.unlock();
            }
            serve(next);
        }
        if (DEBUG) System.err.println("Horizon: lockstep end");
    }

    /** `ClientBridge.call`'s route while a lockstep tick is in flight: the call waits in the inbox and `reply` completes when a park serves it. */
    static boolean intercept(String op, byte[] in, CompletableFuture<byte[]> reply) {
        LOCK.lock();
        try {
            if (!inFlight() || Thread.currentThread() == jobThread) return false;
            INBOX.add(new Request(OP, op, in, reply));
            CHANGED.signalAll();
            return true;
        } finally {
            LOCK.unlock();
        }
    }
}
