package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.util.HexFormat;
import java.util.UUID;

/** A player request's head and groups, byte for byte as Rust's `player_group_tests` write them. */
public final class PlayerGroupsCheck {
    /** `first_request_is_full`'s PLAYER_FULL up to slot 0's stone; slots 1 to 40 follow empty. */
    private static final String FULL_HEAD = "00000001000000020000000300000004000000055374657665000000136d696e6563726166743a6f766572776f726c64000000000000004d00000001400000ff000000085354414e44494e473ff80000000000004050000000000000c00200000000000042b4000000000000000000000000000000000000013f0000000000000941a000000000001440a000000000000000000000000000010000000f6d696e6563726166743a7370656564000000640000000100010000000000000000bfb41205bc01a36e0000000000000000010000002900000000000000260a0000030005636f756e74000000010800026964000f6d696e6563726166743a73746f6e6500";

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) throws Exception {
        var bytes = new ByteArrayOutputStream();
        var data = new DataOutputStream(bytes);
        data.write(HexFormat.of().parseHex(FULL_HEAD));
        for (int slot = 1; slot < 41; slot++) {
            data.writeInt(slot);
            data.writeInt(0);
        }
        var in = Crossing.In.of(bytes.toByteArray());
        var g = PlayerGroups.read(in);
        check(in.available() == 0, "the whole request is read");
        check(g.id.equals(new UUID(0x0000_0001_0000_0002L, 0x0000_0003_0000_0004L)) && g.name.equals("Steve") && g.dim.equals("minecraft:overworld"), "head");
        check(g.gameTime == 77 && g.hand == 1, "time and hand");
        check(g.mask == (PlayerGroups.FULL | 0xff), "mask: full, every group");
        check(g.pose.equals("STANDING") && g.x == 1.5 && g.y == 64.0 && g.z == -2.25 && g.yaw == 90f && g.pitch == 0f && !g.shift, "pose");
        check(g.mode == 0 && g.selected == 0, "mode and selected");
        check(g.xpLevel == 1 && g.xpProgress == 0.5f && g.xpTotal == 9, "experience");
        check(g.health == 20f && g.food == 20 && g.saturation == 5f && g.exhaustion == 0f && g.absorption == 0f, "vitals");
        check(g.effects.size() == 1 && g.effects.get(0).equals(new PlayerGroups.Effect("minecraft:speed", 100, 1, false, true)), "effects");
        check(g.vx == 0.0 && g.vy == -0.0784 && g.vz == 0.0 && g.onGround, "motion");
        check(g.slots.length == 41 && g.slots[0] == 0 && g.tags[0].length == 38 && g.slots[40] == 40 && g.tags[40].length == 0, "stacks: 41 slots, the stone's tag in 0");

        // `selected_slot_only` after the head: mask 4, then the slot.
        var delta = new ByteArrayOutputStream();
        var write = new DataOutputStream(delta);
        write.write(HexFormat.of().parseHex(FULL_HEAD.substring(0, (8 + 8 + 4 + 5 + 4 + 19 + 8 + 4) * 2)));
        write.writeInt(PlayerGroups.SELECTED);
        write.writeInt(3);
        var d = PlayerGroups.read(Crossing.In.of(delta.toByteArray()));
        check(d.mask == 4 && d.selected == 3 && d.pose == null && d.slots.length == 0, "a delta reads only its groups");

        var unknown = new ByteArrayOutputStream();
        var bad = new DataOutputStream(unknown);
        bad.write(HexFormat.of().parseHex(FULL_HEAD.substring(0, (8 + 8 + 4 + 5 + 4 + 19 + 8 + 4) * 2)));
        bad.writeInt(1 << 8);
        boolean threw = false;
        try {
            PlayerGroups.read(Crossing.In.of(unknown.toByteArray()));
        } catch (IllegalStateException e) {
            threw = true;
        }
        check(threw, "an unknown group bit throws");
        System.out.println("PlayerGroupsCheck ok");
    }
}
