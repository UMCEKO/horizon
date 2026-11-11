package dev.horizonlab.javamods.boot;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Callable;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

/** The lockstep between a tick job and the window, on two plain threads: parks, the inbox, releases and the end. */
public final class LockstepCheck {
    private static final List<String> SERVED = Collections.synchronizedList(new ArrayList<>());

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    /** A blocking step with 2 s to finish: a deadlock fails the check instead of hanging it. */
    private static <T> T within(String what, Callable<T> step) throws Exception {
        var future = new CompletableFuture<T>();
        Thread thread = new Thread(() -> {
            try {
                future.complete(step.call());
            } catch (Throwable t) {
                future.completeExceptionally(t);
            }
        }, "window");
        thread.setDaemon(true);
        thread.start();
        try {
            return future.get(2, TimeUnit.SECONDS);
        } catch (java.util.concurrent.TimeoutException e) {
            throw new AssertionError(what + ": no answer in 2 s");
        }
    }

    private record Waited(boolean here, byte[] record) {}

    private static Waited waitFor(Lockstep.Point point) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(within("wait " + point, () -> Lockstep.waitFor(point))));
        boolean here = in.readBoolean();
        return new Waited(here, in.readNBytes(in.readInt()));
    }

    private static byte[] bytes(String text) {
        return text.getBytes(StandardCharsets.UTF_8);
    }

    private static CompletableFuture<byte[]> request(String op) {
        var reply = new CompletableFuture<byte[]>();
        check(Lockstep.intercept(op, new byte[0], reply), op + " goes to the inbox");
        return reply;
    }

    private static Thread job(String name, Runnable parks) {
        Thread thread = new Thread(() -> {
            Lockstep.begin();
            try {
                parks.run();
            } finally {
                Lockstep.end();
            }
        }, name);
        thread.setDaemon(true);
        thread.start();
        return thread;
    }

    public static void main(String[] args) throws Exception {
        Lockstep.ops = (op, in) -> {
            SERVED.add(op + "@" + Thread.currentThread().getName());
            return bytes(op);
        };
        byte[] parkRecord = {7, 8, 9};
        byte[] fill = {1, 2, 3, 4};
        var released = new AtomicReference<byte[]>();

        Lockstep.announce();
        var early = request("early");
        Thread first = job("job", () -> {
            Lockstep.park(Lockstep.Point.K, new byte[0]);
            released.set(Lockstep.park(Lockstep.Point.M, parkRecord));
        });
        check(Arrays.equals(within("a request after announce, before begin", () -> early.get()), bytes("early")), "the early request is answered at K");
        var atK = waitFor(Lockstep.Point.K);
        check(atK.here && atK.record.length == 0, "parked at K");
        var a = request("a");
        var b = request("b");
        check(Arrays.equals(within("a", () -> a.get()), bytes("a")) && Arrays.equals(within("b", () -> b.get()), bytes("b")), "both answered");
        check(SERVED.equals(List.of("early@job", "a@job", "b@job")), "served in order on the job's thread: " + SERVED);
        Lockstep.release(Lockstep.Point.K, new byte[0]);
        var atM = waitFor(Lockstep.Point.M);
        check(atM.here && Arrays.equals(atM.record, parkRecord), "parked at M with its record");
        var c = request("c");
        check(Arrays.equals(within("c", () -> c.get()), bytes("c")), "a request at M");
        Lockstep.release(Lockstep.Point.K, bytes("stale"));
        check(!Lockstep.pending(Lockstep.Point.K), "a release for a point already passed is dropped");
        Lockstep.release(Lockstep.Point.M, fill);
        first.join(2000);
        check(!first.isAlive(), "the job ended");
        check(Arrays.equals(released.get(), fill), "park at M answers the release byte for byte");
        check(Arrays.equals(Lockstep.lastFill(), fill), "the last fill is kept for checks");
        check(!Lockstep.intercept("late", new byte[0], new CompletableFuture<>()), "after the end a request takes the executor's path");

        Lockstep.announce();
        Thread second = job("job2", () -> Lockstep.park(Lockstep.Point.K, new byte[0]));
        check(waitFor(Lockstep.Point.K).here, "the second job parks at K");
        Lockstep.release(Lockstep.Point.K, new byte[0]);
        var noM = waitFor(Lockstep.Point.M);
        check(!noM.here && noM.record.length == 0, "a job that ended without M answers not parked");
        second.join(2000);
        check(!second.isAlive(), "the second job ended");

        Lockstep.announce();
        Lockstep.release(Lockstep.Point.K, new byte[0]);
        Lockstep.release(Lockstep.Point.M, fill);
        Thread third = job("job3", () -> released.set(Lockstep.park(Lockstep.Point.M, new byte[0])));
        third.join(2000);
        check(!third.isAlive() && Arrays.equals(released.get(), fill), "releases sent ahead wait at their points");
        System.out.println("LockstepCheck ok");
    }
}
