package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.host.Host;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/** #262's bulk envelope through the real crossing: bounds, refusals before any item runs, prefixes that keep their effects once, budgets, and nesting. */
public final class BulkCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    /** What items did, in order, as `depth:item`. */
    private static final List<String> LOG = new ArrayList<>();
    /** A value each item bumps then reads back, so a stale read would repeat a number. */
    private static long world;
    private static int depth;

    private static final long THROWS = -1;
    private static final long NESTS = 1000;
    private static final long SLOW = 2000;

    private static byte[] envelope(int version, int op, int budget, int count, int key, long... items) throws Exception {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeByte(version);
        out.writeByte(op);
        out.writeInt(budget);
        out.writeInt(count);
        out.writeInt(key);
        out.writeLong(99);
        for (long item : items) out.writeLong(item);
        return bytes.toByteArray();
    }

    private static byte[] envelope(int budget, long... items) throws Exception {
        return envelope(1, 1, budget, items.length, 7, items);
    }

    /** A run as `GameBridge.blockEntityTickRun` makes it: checked whole, then items in order until one throws or the budget is spent. */
    private static void run(Crossing.In in, Crossing.Out out) throws Exception {
        Bulk.Request request;
        long[] items;
        try {
            request = Bulk.read(in, Bulk.Op.BLOCK_ENTITY_TICK);
            int key = in.readInt();
            in.readLong();
            if (key != 7) throw new Bulk.Rejected("no level for key " + key);
            items = new long[request.count()];
            for (int i = 0; i < items.length; i++) {
                items[i] = in.readLong();
                if (items[i] < THROWS) throw new Bulk.Rejected("item " + items[i]);
            }
        } catch (Bulk.Rejected why) {
            Bulk.reject(out, why);
            return;
        }
        depth++;
        try {
            var reply = new Bulk.Reply(out);
            for (int i = 0; i < items.length; i++) {
                if (i > 0 && request.spent()) break;
                long result;
                try {
                    result = item(items[i]);
                } catch (IllegalStateException threw) {
                    reply.begin(Bulk.THREW);
                    reply.end();
                    break;
                }
                reply.begin(Bulk.DONE);
                out.writeLong(result);
                reply.end();
            }
        } finally {
            depth--;
        }
    }

    private static long item(long item) throws Exception {
        if (item == THROWS) {
            world++;
            throw new IllegalStateException("item threw");
        }
        if (item == SLOW) {
            long until = System.nanoTime() + 3_000_000;
            while (System.nanoTime() < until) Thread.onSpinWait();
        }
        LOG.add(depth + ":" + item);
        if (item == NESTS) {
            var inner = parse(Crossing.call("t_run", envelope(1000, 1, 2)), 2);
            check(inner.processed == 2 && inner.results[1] == inner.results[0] + 1, "the nested run reads its own writes");
            return inner.processed;
        }
        return ++world;
    }

    private record Parsed(boolean rejected, int processed, byte[] statuses, long[] results) {}

    private static Parsed parse(byte[] reply, int sent) {
        var in = ByteBuffer.wrap(reply);
        check(in.get() == Bulk.VERSION, "the reply's version");
        byte outcome = in.get();
        int processed = in.getInt();
        check(processed >= 0 && processed <= sent, "processed " + processed + " of " + sent);
        if (outcome == Bulk.REJECTED) {
            check(processed == 0 && !in.hasRemaining(), "a refusal carries nothing");
            return new Parsed(true, 0, new byte[0], new long[0]);
        }
        check(outcome == Bulk.OK, "outcome " + outcome);
        var statuses = new byte[processed];
        var results = new long[processed];
        for (int i = 0; i < processed; i++) {
            statuses[i] = in.get();
            int length = in.getInt();
            check(length == (statuses[i] == Bulk.DONE ? 8 : 0), "item " + i + " result length " + length);
            if (length == 8) results[i] = in.getLong();
        }
        check(!in.hasRemaining(), "nothing past the last item");
        return new Parsed(false, processed, statuses, results);
    }

    private static Parsed call(byte[] envelope, int sent) {
        return parse(Crossing.call("t_run", envelope), sent);
    }

    private static void refused(byte[] envelope, String what) {
        int logged = LOG.size();
        long before = world;
        check(call(envelope, Integer.MAX_VALUE).rejected, what + " is refused");
        check(LOG.size() == logged && world == before, what + " runs nothing");
    }

    public static void main(String[] args) throws Exception {
        Crossing.free("t_run", BulkCheck::run);
        Crossing.seal();

        byte[] rust = {1, 1, 0, 0, 1, 44, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 4, (byte) 0xd2, -1, -1, -1, -1, -1, -1, -1, -7};
        var asRust = Crossing.In.of(rust);
        var read = Bulk.read(asRust, Bulk.Op.BLOCK_ENTITY_TICK);
        check(read.count() == 1 && read.budgetNanos() == 300_000, "Rust's envelope header");
        check(asRust.readInt() == 2 && asRust.readLong() == 1234 && asRust.readLong() == -7 && asRust.available() == 0, "Rust's envelope body");

        var empty = call(envelope(1000), 0);
        check(!empty.rejected && empty.processed == 0 && LOG.isEmpty(), "an empty run ticks nothing");

        var single = call(envelope(1000, 5), 1);
        check(single.processed == 1 && single.results[0] == 1 && LOG.equals(List.of("1:5")), "a single item");

        int max = Bulk.Op.BLOCK_ENTITY_TICK.maxItems();
        check(max == 256, "the item bound: " + max);
        long[] many = new long[max];
        for (int i = 0; i < max; i++) many[i] = i;
        LOG.clear();
        var large = call(envelope(1_000_000, many), max);
        check(large.processed == max, "a full run ticks every item");
        for (int i = 0; i < max; i++) {
            check(large.statuses[i] == Bulk.DONE && LOG.get(i).equals("1:" + i), "item " + i + " once, in order");
            if (i > 0) check(large.results[i] == large.results[i - 1] + 1, "item " + i + " sees the write before it");
        }
        check(LOG.size() == max, "no item twice");

        refused(envelope(1, 1, 1000, max + 1, 7, new long[max + 1]), "one item over the bound");
        refused(envelope(1, 1, 1000, 600, 7, new long[600]), "an envelope over the byte bound");
        refused(envelope(2, 1, 1000, 1, 7, 5), "an unknown version");
        refused(envelope(1, 9, 1000, 1, 7, 5), "an unknown op");
        refused(envelope(1, 1, 0, 1, 7, 5), "no budget");
        refused(envelope(1, 1, 1000, 2, 7, 5), "a count over the records");
        refused(envelope(1, 1, 1000, 1, 7, 5, 6), "records past the count");
        refused(envelope(1, 1, 1000, -1, 7), "a negative count");
        refused(envelope(1, 1, 1000, 1, 3, 5), "an unknown level");
        refused(envelope(1000, 5, -9, 6), "an invalid id after valid ones");
        refused(new byte[] {1, 1, 0}, "a cut header");

        LOG.clear();
        long before = world;
        var partial = call(envelope(1000, 10, 11, THROWS, 12), 4);
        check(partial.processed == 3 && partial.statuses[2] == Bulk.THREW, "the run ends at the item that threw");
        check(LOG.equals(List.of("1:10", "1:11")) && world == before + 3, "the prefix keeps its effects, the thrower ran once, the rest never ran");
        var resumed = call(envelope(1000, 12), 1);
        check(resumed.results[0] == world && LOG.equals(List.of("1:10", "1:11", "1:12")), "the rest runs once when sent again");

        LOG.clear();
        var budget = call(envelope(1000, SLOW, 20, 21), 3);
        check(budget.processed == 1 && LOG.equals(List.of("1:" + SLOW)), "a spent budget stops before the next item");
        var unbounded = call(envelope(1_000_000, SLOW, 20), 2);
        check(unbounded.processed == 2, "the first item always runs, and a budget left lets the next");

        LOG.clear();
        var nested = call(envelope(1000, 30, NESTS, 31), 3);
        check(nested.processed == 3 && nested.results[1] == 2, "the outer run goes on after the nested one");
        check(LOG.equals(List.of("1:30", "1:" + NESTS, "2:1", "2:2", "1:31")), "nested items run once each, inside their outer item: " + LOG);
        check(nested.results[2] == nested.results[0] + 3, "the outer run reads what the nested run wrote");

        long outer = Bulk.open();
        check(Bulk.enter(0) && Bulk.enter(1) && Host.bulkItem == 1, "items start while no callback stopped the call");
        // What `Host::bulk_stop` does at the first callback that writes into Horizon.
        Host.bulkStop = true;
        check(!Bulk.enter(2) && Host.bulkItem == 1, "no item starts once Rust stopped the call, which names the item that wrote");
        long inner = Bulk.open();
        check(Bulk.enter(0) && Host.bulkItem == 0, "a nested call starts unstopped");
        Bulk.close(inner);
        check(Host.bulkStop && Host.bulkItem == 1, "closing a nested call gives the outer one its cell back");
        Bulk.close(outer);
        check(!Host.bulkStop && Host.bulkItem == 0, "closing the outer call leaves the cell as it found it");

        System.out.println("BulkCheck ok");
    }
}
