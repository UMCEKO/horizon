package dev.horizonlab.javamods.boot;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;

/** The server side and the Java client cross at once: neither waits on the other, two server-side callers never interleave, and the client's thread can't run a server op. */
public final class SidesCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private static int count;

    private static byte[] ints(int... values) {
        var buffer = ByteBuffer.allocate(values.length * 4);
        for (int value : values) buffer.putInt(value);
        return buffer.array();
    }

    public static void main(String[] args) throws Exception {
        ExecutorService render = Executors.newSingleThreadExecutor(task -> {
            var thread = new Thread(task, "Render thread");
            thread.setDaemon(true);
            ServerSide.client = thread;
            return thread;
        });
        var serverIn = new CountDownLatch(1);
        var clientRan = new CountDownLatch(1);
        GameBridge.client = (name, input) -> {
            try {
                return render.submit(() -> {
                    clientRan.countDown();
                    return input;
                }).get();
            } catch (Exception e) {
                throw new IllegalStateException(e);
            }
        };
        Crossing.client("t_client");
        var inside = new AtomicInteger();
        var interleaved = new AtomicBoolean();
        Crossing.game("t_server", (in, out) -> {
            if (inside.incrementAndGet() != 1) interleaved.set(true);
            boolean hold = in.readInt() != 0;
            boolean ran = true;
            if (hold) {
                serverIn.countDown();
                ran = clientRan.await(10, TimeUnit.SECONDS);
            } else {
                int seen = count;
                Thread.onSpinWait();
                count = seen + 1;
            }
            inside.decrementAndGet();
            out.writeBoolean(ran);
        });
        Crossing.seal();

        var held = new boolean[1];
        var server = new Thread(() -> held[0] = Crossing.call("t_server", ints(1))[0] != 0, "Server thread");
        server.start();
        check(serverIn.await(10, TimeUnit.SECONDS), "the server's op started");
        byte[] echoed = Crossing.call("t_client", new byte[] {7});
        check(echoed.length == 1 && echoed[0] == 7, "the client's op answers while a server op holds its turn");
        server.join(20_000);
        check(!server.isAlive() && held[0], "the server's op saw the client run, so neither waited on the other");

        int threads = 4, calls = 500;
        var callers = new ArrayList<Thread>();
        for (int t = 0; t < threads; t++) {
            var caller = new Thread(() -> {
                for (int i = 0; i < calls; i++) Crossing.call("t_server", ints(0));
            }, "Server caller " + t);
            callers.add(caller);
            caller.start();
        }
        for (int i = 0; i < calls; i++) Crossing.call("t_client", new byte[] {1});
        for (var caller : callers) caller.join(60_000);
        check(!interleaved.get() && count == threads * calls, "server-side callers take turns: " + count + " of " + threads * calls);

        var refused = render.submit(() -> {
            try {
                Crossing.call("t_server", ints(0));
                return false;
            } catch (IllegalStateException failed) {
                return true;
            }
        }).get();
        check(refused, "the client's thread can't run a server op");
        System.out.println("SidesCheck ok");
    }
}
