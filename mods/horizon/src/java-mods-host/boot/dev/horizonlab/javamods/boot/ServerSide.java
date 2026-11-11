package dev.horizonlab.javamods.boot;

import java.util.concurrent.locks.ReentrantLock;

/** The server side's Java objects belong to the thread holding its turn: the server's own as it ticks, or another between its calls (the boot, a guest's configuration); the Java client never takes it and hears of the server only in packets (`Clients`). */
final class ServerSide {
    private ServerSide() {}

    private static final ReentrantLock TURN = new ReentrantLock();

    /** The Java client's render thread, which owns the client side; null without a client. */
    static volatile Thread client;

    /** A server op on this thread's turn; a wait for it is another server-side caller's, never the window's. */
    static void serve(Crossing.Handler handler, Crossing.Frame frame) throws Throwable {
        if (Thread.currentThread() == client) throw new IllegalStateException(handler.name() + " is the server's: the Java client's thread can't run it");
        if (!TURN.tryLock()) {
            long queued = System.nanoTime();
            TURN.lock();
            if (Profile.ON) Profile.wait(handler.name(), System.nanoTime() - queued);
        }
        try {
            GameBridge.game(handler, frame);
        } finally {
            TURN.unlock();
        }
    }
}
