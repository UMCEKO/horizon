package dev.horizonlab.javamods.boot;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.IOException;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.List;
import java.util.UUID;

/** What the window applied of the server's entity packets, in their order, and how far it got: the Java server's packets to this client wait until the window applied what was sent before them. */
public final class EntityFeed {
    static final byte ADD = 0;
    static final byte DATA = 1;
    static final byte EQUIP = 2;
    static final byte PASSENGERS = 3;
    static final byte MOVE = 4;
    static final byte EVENT = 5;
    static final byte REMOVE = 6;
    static final byte TICK = 7;
    /** An ADD's event bits: the client's listeners hear this entity's join and leave. */
    static final int JOIN = 1;
    static final int LEAVE = 2;
    /** A TICK's bits: its Pre and Post. */
    static final int PRE = 1;
    static final int POST = 2;

    /** One change, as `java_client_entities.rs` writes it. */
    public sealed interface Change permits Add, Data, Equip, Passengers, Move, Event, Remove, Tick {}

    public record Add(int id, UUID uuid, String type, double x, double y, double z, float pitch, float yaw, float head, double dx, double dy, double dz, int data, int events) implements Change {}

    /** `ClientboundSetEntityDataPacket` as it came. */
    public record Data(int id, byte[] packet) implements Change {}

    /** `ClientboundSetEquipmentPacket` as it came. */
    public record Equip(int id, byte[] packet) implements Change {}

    public record Passengers(int vehicle, int[] riders) implements Change {}

    public record Move(int id, double x, double y, double z, float yaw, float pitch, float head, double dx, double dy, double dz, boolean onGround) implements Change {}

    public record Event(int id, byte event) implements Change {}

    public record Remove(int[] ids) implements Change {}

    /** The window ticked the entity: the Java client posts the tick events `mask` names. */
    public record Tick(int id, int mask) implements Change {}

    private record Entry(long seq, byte[] body) {}

    private final ArrayDeque<Entry> pending = new ArrayDeque<>();
    private long seen;

    /** A change the window applied with its play packet at `seq`. */
    public void feed(long seq, byte[] body) {
        pending.add(new Entry(seq, body));
        seen = Math.max(seen, seq);
    }

    /** The window applied the play packets up to `seq`. */
    public void seen(long seq) {
        seen = Math.max(seen, seq);
    }

    /** Whether a packet the server sent after its `stamp`th play packet may apply: the window applied that one. */
    public boolean ready(long stamp) {
        return stamp <= seen;
    }

    /** The changes of the play packets up to `stamp`, oldest first, taken. */
    public List<Change> upTo(long stamp) throws IOException {
        var out = new ArrayList<Change>();
        while (!pending.isEmpty() && pending.peekFirst().seq() <= stamp) out.add(read(pending.pollFirst().body()));
        return out;
    }

    /** Every change not taken yet, oldest first. */
    public List<Change> all() throws IOException {
        return upTo(Long.MAX_VALUE);
    }

    /** The window left its world. */
    public void clear() {
        pending.clear();
        seen = 0;
    }

    static Change read(byte[] body) throws IOException {
        var in = new DataInputStream(new ByteArrayInputStream(body));
        byte kind = in.readByte();
        return switch (kind) {
            case ADD -> new Add(in.readInt(), new UUID(in.readLong(), in.readLong()), new String(in.readNBytes(in.readInt()), java.nio.charset.StandardCharsets.UTF_8),
                    in.readDouble(), in.readDouble(), in.readDouble(), in.readFloat(), in.readFloat(), in.readFloat(), in.readDouble(), in.readDouble(), in.readDouble(), in.readInt(), in.readUnsignedByte());
            case DATA -> new Data(in.readInt(), in.readNBytes(in.readInt()));
            case EQUIP -> new Equip(in.readInt(), in.readNBytes(in.readInt()));
            case PASSENGERS -> new Passengers(in.readInt(), ints(in));
            case MOVE -> new Move(in.readInt(), in.readDouble(), in.readDouble(), in.readDouble(), in.readFloat(), in.readFloat(), in.readFloat(), in.readDouble(), in.readDouble(), in.readDouble(), in.readBoolean());
            case EVENT -> new Event(in.readInt(), in.readByte());
            case REMOVE -> new Remove(ints(in));
            case TICK -> new Tick(in.readInt(), in.readUnsignedByte());
            default -> throw new IOException("entity feed entry " + kind);
        };
    }

    private static int[] ints(DataInputStream in) throws IOException {
        int[] out = new int[in.readInt()];
        for (int i = 0; i < out.length; i++) out[i] = in.readInt();
        return out;
    }
}
