package dev.horizonlab.javamods.boot;

import java.util.ArrayList;
import java.util.List;
import java.util.UUID;

/** A player request's head and the groups Rust sent (`encode_player_groups`), read without touching a view. */
final class PlayerGroups {
    static final int POSE = 1, MODE = 2, SELECTED = 4, XP = 8, VITALS = 16, EFFECTS = 32, MOTION = 64, STACKS = 128, MENU = 256, FULL = 1 << 30;
    private static final int KNOWN = POSE | MODE | SELECTED | XP | VITALS | EFFECTS | MOTION | STACKS | MENU | FULL;

    record Effect(String id, int duration, int amplifier, boolean ambient, boolean visible) {}

    UUID id;
    String name, pose;
    long gameTime;
    int key, hand, mask, mode, selected, xpLevel, xpTotal, food;
    double x, y, z, vx, vy, vz;
    float flyingSpeed, walkingSpeed, yaw, pitch, xpProgress, health, saturation, exhaustion, absorption;
    boolean abilitiesHeld, invulnerable, flying, mayfly, instabuild, mayBuild, shift, onGround;
    final List<Effect> effects = new ArrayList<>();
    int[] slots = new int[0];
    byte[][] tags = new byte[0][];
    int menuId = -1;
    byte[] menuCarried = new byte[0];
    int[] menuSlots = new int[0], dataIds = new int[0], dataValues = new int[0];
    byte[][] menuTags = new byte[0][];

    boolean has(int group) {
        return (mask & group) != 0;
    }

    static PlayerGroups read(Crossing.In in) {
        var g = new PlayerGroups();
        g.id = new UUID(in.readLong(), in.readLong());
        g.name = in.name();
        g.key = in.readInt();
        g.gameTime = in.readLong();
        g.hand = in.readInt();
        g.abilitiesHeld = in.readBoolean();
        g.invulnerable = in.readBoolean();
        g.flying = in.readBoolean();
        g.mayfly = in.readBoolean();
        g.instabuild = in.readBoolean();
        g.mayBuild = in.readBoolean();
        g.flyingSpeed = in.readFloat();
        g.walkingSpeed = in.readFloat();
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
        if (g.has(MENU)) {
            g.menuId = in.readInt();
            g.menuCarried = in.readNBytes(in.readInt());
            int n = in.readInt();
            g.menuSlots = new int[n];
            g.menuTags = new byte[n][];
            for (int i = 0; i < n; i++) {
                g.menuSlots[i] = in.readInt();
                g.menuTags[i] = in.readNBytes(in.readInt());
            }
            int m = in.readInt();
            g.dataIds = new int[m];
            g.dataValues = new int[m];
            for (int i = 0; i < m; i++) {
                g.dataIds[i] = in.readInt();
                g.dataValues[i] = in.readInt();
            }
        }
        return g;
    }
}
