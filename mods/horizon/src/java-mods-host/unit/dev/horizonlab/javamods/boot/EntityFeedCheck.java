package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.nio.charset.StandardCharsets;
import java.util.UUID;

/** The window's entity changes as the Java client reads them, and their order against the Java server's packets. */
public final class EntityFeedCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private interface Body {
        void write(DataOutputStream out) throws Exception;
    }

    private static byte[] entry(byte kind, Body body) throws Exception {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeByte(kind);
        body.write(out);
        return bytes.toByteArray();
    }

    private static byte[] add(int id) throws Exception {
        return entry(EntityFeed.ADD, out -> {
            out.writeInt(id);
            out.writeLong(0x0102030405060708L);
            out.writeLong(0x090a0b0c0d0e0f10L);
            byte[] type = "minecraft:pig".getBytes(StandardCharsets.UTF_8);
            out.writeInt(type.length);
            out.write(type);
            out.writeDouble(1.5);
            out.writeDouble(64);
            out.writeDouble(-2.5);
            out.writeFloat(0);
            out.writeFloat(90);
            out.writeFloat(-180);
            out.writeDouble(0);
            out.writeDouble(-0.08);
            out.writeDouble(0);
            out.writeInt(3);
        });
    }

    private static byte[] remove(int... ids) throws Exception {
        return entry(EntityFeed.REMOVE, out -> {
            out.writeInt(ids.length);
            for (int id : ids) out.writeInt(id);
        });
    }

    public static void main(String[] args) throws Exception {
        var add = (EntityFeed.Add) EntityFeed.read(add(7));
        check(add.id() == 7 && add.type().equals("minecraft:pig") && add.data() == 3, "an ADD's id, type and data");
        check(add.uuid().equals(new UUID(0x0102030405060708L, 0x090a0b0c0d0e0f10L)), "an ADD's UUID");
        check(add.x() == 1.5 && add.y() == 64 && add.z() == -2.5 && add.dy() == -0.08, "an ADD's position and motion");
        check(add.pitch() == 0 && add.yaw() == 90 && add.head() == -180, "an ADD's look: pitch, yaw, head yaw");
        var removed = (EntityFeed.Remove) EntityFeed.read(remove(7, 9));
        check(removed.ids().length == 2 && removed.ids()[0] == 7 && removed.ids()[1] == 9, "a REMOVE's ids");

        var feed = new EntityFeed();
        feed.feed(3, add(7));
        feed.feed(5, remove(7));
        feed.seen(6);
        check(feed.ready(4) && feed.ready(6) && !feed.ready(7), "a packet sent after the window's last applied one waits");
        // A packet the server sent after its fourth play packet sees the pig the third added, not its removal.
        var before = feed.upTo(4);
        check(before.size() == 1 && before.get(0) instanceof EntityFeed.Add, "the changes up to a packet's stamp come first");
        var rest = feed.all();
        check(rest.size() == 1 && rest.get(0) instanceof EntityFeed.Remove, "the rest follow it");
        check(feed.all().isEmpty(), "each change is taken once");
        feed.clear();
        check(!feed.ready(1) && feed.ready(0), "a new world starts from no packet");
        System.out.println("EntityFeedCheck ok");
    }
}
