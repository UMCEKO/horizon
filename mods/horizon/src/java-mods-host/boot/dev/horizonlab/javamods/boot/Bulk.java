package dev.horizonlab.javamods.boot;

import java.io.IOException;

/** #262: the bounded typed bulk envelope, read whole before any item runs (`horizon_java_host::bulk`); it rides the op it is registered under. */
final class Bulk {
    private Bulk() {}

    /** The only layout version; the host jar ships with the game, so there is no fallback. */
    static final int VERSION = 1;
    static final int MAX_ITEMS = 256;
    static final int MAX_BYTES = 4096;
    static final int HEADER = 10;

    static final byte OK = 0;
    static final byte REJECTED = 1;
    static final byte GONE = 0;
    static final byte DONE = 1;
    static final byte THREW = 2;

    /** Each op's id, and its fixed context and record sizes. */
    enum Op {
        BLOCK_ENTITY_TICK(1, 4 + 8, 8);

        final int id;
        final int context;
        final int record;

        Op(int id, int context, int record) {
            this.id = id;
            this.context = context;
            this.record = record;
        }

        int maxItems() {
            return Math.min(MAX_ITEMS, (MAX_BYTES - HEADER - context) / record);
        }
    }

    /** An envelope that fails a check; nothing has run when it is thrown. */
    static final class Rejected extends Exception {
        Rejected(String why) {
            super(why, null, false, false);
        }
    }

    /** A checked envelope: its context and then its `count` records are next in `in`; the budget bounds the lock hold. */
    record Request(Op op, long budgetNanos, int count, long started) {
        /** Whether the run has used its work budget; checked only between items, where splitting is safe. */
        boolean spent() {
            return System.nanoTime() - started >= budgetNanos;
        }
    }

    /** Checks the whole envelope's layout against `expected`, leaving `in` at its context. */
    static Request read(Crossing.In in, Op expected) throws Rejected {
        long started = System.nanoTime();
        int total = in.available();
        if (total > MAX_BYTES) throw new Rejected(total + " bytes over " + MAX_BYTES);
        if (total < HEADER) throw new Rejected("ends early");
        int version = in.readByte() & 0xff;
        if (version != VERSION) throw new Rejected("unknown version " + version);
        int op = in.readByte() & 0xff;
        if (op != expected.id) throw new Rejected("op " + op + " where " + expected + " was expected");
        int budget = in.readInt();
        if (budget <= 0) throw new Rejected("budget " + budget + " µs");
        int count = in.readInt();
        if (count < 0 || count > expected.maxItems()) throw new Rejected("count " + count);
        long need = expected.context + (long) count * expected.record;
        if (in.available() != need) throw new Rejected(in.available() + " bytes for " + need);
        return new Request(expected, budget * 1000L, count, started);
    }

    /** Writes the reply to an envelope that was refused, so Rust knows nothing ran. */
    static void reject(Crossing.Out out, Rejected why) throws IOException {
        System.err.println("Horizon: a bulk envelope was refused: " + why.getMessage());
        out.writeByte(VERSION);
        out.writeByte(REJECTED);
        out.writeInt(0);
    }

    /** The reply: a header whose count is patched as each item ends, then status and length-prefixed result per item. */
    static final class Reply {
        private final Crossing.Out out;
        private int item = -1;
        private int processed;

        Reply(Crossing.Out out) throws IOException {
            this.out = out;
            out.writeByte(VERSION);
            out.writeByte(OK);
            out.writeInt(0);
        }

        /** Starts item `processed`'s result: write it into `out`, then `end`. */
        void begin(byte status) throws IOException {
            out.writeByte(status);
            item = out.length();
            out.writeInt(0);
        }

        void end() {
            out.patchInt(item, out.length() - item - 4);
            out.patchInt(2, ++processed);
            item = -1;
        }

        int processed() {
            return processed;
        }
    }
}
