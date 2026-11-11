package dev.horizonlab.javamods.boot;

import java.util.ArrayList;
import java.util.List;
import java.util.UUID;

/** A player request's head and the groups Rust sent (`encode_player_groups`), read without touching a view. */
final class PlayerGroups {
    static final int POSE = 1, MODE = 2, SELECTED = 4, XP = 8, VITALS = 16, EFFECTS = 32, MOTION = 64, STACKS = 128, FULL = 1 << 30;
    private static final int KNOWN = POSE | MODE | SELECTED | XP | VITALS | EFFECTS | MOTION | STACKS | FULL;

    record Effect(String id, int duration, int amplifier, boolean ambient, boolean visible) {}

    UUID id;
    String name, dim, pose;
    long gameTime;
    int hand, mask, mode, selected, xpLevel, xpTotal, food;
    double x, y, z, vx, vy, vz;
    float yaw, pitch, xpProgress, health, saturation, exhaustion, absorption;
    boolean shift, onGround;
    final List<Effect> effects = new ArrayList<>();
    int[] slots = new int[0];
    byte[][] tags = new byte[0][];

    boolean has(int group) {
        return (mask & group) != 0;
    }

    static PlayerGroups read(Crossing.In in) {
        var g = new PlayerGroups();
        g.id = new UUID(in.readLong(), in.readLong());
        g.name = in.name();
        g.dim = in.name();
        g.gameTime = in.readLong();
        g.hand = in.readInt();
        g.mask = in.readInt();
        if ((g.mask & ~KNOWN) != 0) throw new IllegalStateException("Horizon: unknown player groups " + Integer.toHexString(g.mask & ~KNOWN));
        if (g.has(POSE)) {
            g.pose = in.name();
            g.x = in.readDouble();
            g.y = in.readDouble();
            g.z = in.readDouble();
            g.yaw = in.readFloat();
            g.pitch = in.readFloat();
            g.shift = in.readBoolean();
        }
        if (g.has(MODE)) g.mode = in.readInt();
        if (g.has(SELECTED)) g.selected = in.readInt();
        if (g.has(XP)) {
            g.xpLevel = in.readInt();
            g.xpProgress = in.readFloat();
            g.xpTotal = in.readInt();
        }
        if (g.has(VITALS)) {
            g.health = in.readFloat();
            g.food = in.readInt();
            g.saturation = in.readFloat();
            g.exhaustion = in.readFloat();
            g.absorption = in.readFloat();
        }
        if (g.has(EFFECTS)) {
            for (int n = in.readInt(); n > 0; n--) g.effects.add(new Effect(in.text(),in.readInt(), in.readInt(), in.readBoolean(), in.readBoolean()));
        }
        if (g.has(MOTION)) {
            g.vx = in.readDouble();
            g.vy = in.readDouble();
            g.vz = in.readDouble();
            g.onGround = in.readBoolean();
        }
        if (g.has(STACKS)) {
            int n = in.readInt();
            g.slots = new int[n];
            g.tags = new byte[n][];
            for (int i = 0; i < n; i++) {
                g.slots[i] = in.readInt();
                g.tags[i] = in.readNBytes(in.readInt());
            }
        }
        return g;
    }
}
