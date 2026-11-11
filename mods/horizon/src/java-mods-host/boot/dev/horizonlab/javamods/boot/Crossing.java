package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.host.Host;
import java.io.DataOutputStream;
import java.io.EOFException;
import java.io.OutputStream;
import java.io.UncheckedIOException;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/** Rust's requests: an int op and a Java-owned direct buffer per thread and nesting level, in place of a String and two byte arrays. */
public final class Crossing {
    private Crossing() {}

    static final int FAILED = -1;
    static final int OVERFLOW = -2;
    private static final byte[] NONE = new byte[0];

    /** FREE runs on the calling thread, CLIENT on the Java client's render thread, GAME on the server side's turn (`ServerSide`). */
    enum Kind { FREE, CLIENT, GAME }

    interface Raw {
        byte[] run(byte[] input) throws Throwable;
    }

    interface Direct {
        void run(In in, Out out) throws Throwable;
    }

    record Handler(String name, Kind kind, Raw raw, Direct direct) {}

    private static final List<Handler> REGISTERED = new ArrayList<>();
    private static Handler[] table;

    private static synchronized void register(Handler handler) {
        if (table != null) throw new IllegalStateException("the op table is sealed: " + handler.name);
        for (Handler known : REGISTERED) if (known.name.equals(handler.name)) throw new IllegalStateException("op registered twice: " + handler.name);
        REGISTERED.add(handler);
    }

    static void game(String name, Raw raw) {
        register(new Handler(name, Kind.GAME, raw, null));
    }

    static void game(String name, Direct direct) {
        register(new Handler(name, Kind.GAME, null, direct));
    }

    static void free(String name, Raw raw) {
        register(new Handler(name, Kind.FREE, raw, null));
    }

    static void free(String name, Direct direct) {
        register(new Handler(name, Kind.FREE, null, direct));
    }

    /** A call of the Java client: empty without one, and answered on its render thread. */
    static void client(String name) {
        register(new Handler(name, Kind.CLIENT, input -> {
            var client = GameBridge.client;
            return client == null ? NONE : client.apply(name, input);
        }, null));
    }

    /** The ops in index order; Rust reads them once at the boot's end, so every op registers before it. */
    static synchronized String[] seal() {
        if (table == null) table = REGISTERED.toArray(new Handler[0]);
        return Arrays.stream(table).map(Handler::name).toArray(String[]::new);
    }

    /** Runs a handler; the adapter hands a byte[] handler its input and takes its reply. */
    static void run(Handler handler, Frame frame) throws Throwable {
        if (handler.direct != null) {
            handler.direct.run(frame.in, frame.out);
        } else {
            byte[] reply = handler.raw.run(frame.in.rest());
            frame.out.write(reply, 0, reply.length);
        }
    }

    static final class Frame {
        final In in = new In();
        final Out out = new Out();
    }

    private static final class State {
        int depth = -1;
        Frame[] frames = new Frame[2];
        byte[] overflow;
    }

    private static final ThreadLocal<State> STATE = ThreadLocal.withInitial(State::new);

    /** One request: the reply goes to the arena's start, and the return is its length, 0 for none, FAILED, or OVERFLOW (then `overflow()`). */
    static int dispatch(int op, ByteBuffer arena, int length) {
        State state = STATE.get();
        if (++state.depth == state.frames.length) state.frames = Arrays.copyOf(state.frames, state.depth * 2);
        Frame frame = state.frames[state.depth];
        if (frame == null) frame = state.frames[state.depth] = new Frame();
        try {
            Handler handler = table[op];
            frame.in.set(arena, length);
            frame.out.reset();
            try {
                switch (handler.kind) {
                    case FREE, CLIENT -> run(handler, frame);
                    case GAME -> ServerSide.serve(handler, frame);
                }
            } catch (Throwable t) {
                t.printStackTrace();
                return FAILED;
            }
            int n = frame.out.length();
            if (n > arena.capacity()) {
                state.overflow = frame.out.toByteArray();
                return OVERFLOW;
            }
            if (n > 0) arena.put(0, frame.out.buffer(), 0, n);
            return n;
        } finally {
            frame.in.release();
            state.depth--;
        }
    }

    /** The reply that did not fit its arena, for the thread that just got OVERFLOW. */
    static byte[] overflow() {
        State state = STATE.get();
        byte[] reply = state.overflow;
        state.overflow = null;
        return reply;
    }

    /** One request by name, as Rust makes it, for Java's own callers. */
    static byte[] call(String op, byte[] input) {
        int id = -1;
        for (int i = 0; i < table.length; i++) if (table[i].name.equals(op)) id = i;
        if (id < 0) throw new IllegalArgumentException(op);
        ByteBuffer arena = ByteBuffer.allocateDirect(Math.max(input.length, 1 << 16));
        arena.put(0, input);
        int n = dispatch(id, arena, input.length);
        if (n == OVERFLOW) return overflow();
        if (n < 0) throw new IllegalStateException(op + " failed");
        byte[] reply = new byte[n];
        arena.get(0, reply);
        return reply;
    }

    static final Host.Dispatcher DISPATCHER = new Host.Dispatcher() {
        @Override
        public int dispatch(int op, ByteBuffer arena, int length) {
            return Crossing.dispatch(op, arena, length);
        }

        @Override
        public byte[] overflow() {
            return Crossing.overflow();
        }

        @Override
        public String[] ops() {
            return seal();
        }
    };

    /** The request's fields as `DataInputStream` reads them, straight from the arena; valid until the handler returns. */
    public static final class In {
        private ByteBuffer buffer;
        private int position;
        private int limit;
        private final String[] levelNames = new String[4];
        private final byte[][] levelBytes = new byte[4][];
        private int levels;

        void set(ByteBuffer buffer, int length) {
            this.buffer = buffer;
            this.position = 0;
            this.limit = length;
        }

        void release() {
            buffer = null;
            position = 0;
            limit = 0;
        }

        public static In of(byte[] bytes) {
            In in = new In();
            in.set(ByteBuffer.wrap(bytes), bytes.length);
            return in;
        }

        private int take(int n) {
            if (limit - position < n) throw new UncheckedIOException(new EOFException());
            int at = position;
            position += n;
            return at;
        }

        public int available() {
            return limit - position;
        }

        public int readInt() {
            return buffer.getInt(take(4));
        }

        public long readLong() {
            return buffer.getLong(take(8));
        }

        public float readFloat() {
            return buffer.getFloat(take(4));
        }

        public double readDouble() {
            return buffer.getDouble(take(8));
        }

        public boolean readBoolean() {
            return buffer.get(take(1)) != 0;
        }

        public byte readByte() {
            return buffer.get(take(1));
        }

        public byte[] readNBytes(int n) {
            if (n < 0) throw new UncheckedIOException(new EOFException());
            byte[] bytes = new byte[n];
            buffer.get(take(n), bytes);
            return bytes;
        }

        /** What is left, as the byte[] a legacy handler takes. */
        byte[] rest() {
            return readNBytes(available());
        }

        /** An int length, then UTF-8: `wire::Out::text`. */
        public String text() {
            return new String(readNBytes(readInt()), StandardCharsets.UTF_8);
        }

        private boolean same(int at, byte[] known) {
            for (int i = 0; i < known.length; i++) if (buffer.get(at + i) != known[i]) return false;
            return true;
        }

        /** `text()` for the few strings a call repeats (level ids): the same String each time, without decoding it. */
        public String name() {
            int length = readInt();
            int at = take(length);
            for (int i = 0; i < levels; i++) {
                byte[] known = levelBytes[i];
                if (known.length == length && same(at, known)) return levelNames[i];
            }
            byte[] bytes = new byte[length];
            buffer.get(at, bytes);
            String name = new String(bytes, StandardCharsets.UTF_8);
            int slot = levels < levelNames.length ? levels++ : 0;
            levelNames[slot] = name;
            levelBytes[slot] = bytes;
            return name;
        }
    }

    /** The reply's fields, written by `DataOutputStream` itself into a buffer a thread reuses. */
    public static final class Out extends DataOutputStream {
        private final Sink sink;

        Out() {
            this(new Sink());
        }

        private Out(Sink sink) {
            super(sink);
            this.sink = sink;
        }

        void reset() {
            written = 0;
            sink.length = 0;
            if (sink.buffer.length > KEEP) sink.buffer = new byte[512];
        }

        int length() {
            return sink.length;
        }

        byte[] buffer() {
            return sink.buffer;
        }

        byte[] toByteArray() {
            return Arrays.copyOf(sink.buffer, sink.length);
        }
    }

    private static final int KEEP = 1 << 20;

    private static final class Sink extends OutputStream {
        byte[] buffer = new byte[512];
        int length;

        private int room(int n) {
            if (length + n > buffer.length) buffer = Arrays.copyOf(buffer, Math.max(buffer.length * 2, length + n));
            int at = length;
            length += n;
            return at;
        }

        @Override
        public void write(int b) {
            int at = room(1);
            buffer[at] = (byte) b;
        }

        @Override
        public void write(byte[] bytes, int offset, int count) {
            int at = room(count);
            System.arraycopy(bytes, offset, buffer, at, count);
        }
    }
}
