package dev.horizonlab.javamods.boot;

import net.minecraft.world.entity.player.Input;

/** The input request's bits decode to the `Input` vanilla's constructor builds from the same seven booleans, and encode back to the same byte. */
public final class LockstepEventsCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) {
        for (int bits = 0; bits < 128; bits++) {
            Input expected = new Input((bits & 1) != 0, (bits & 2) != 0, (bits & 4) != 0, (bits & 8) != 0, (bits & 16) != 0, (bits & 32) != 0, (bits & 64) != 0);
            Input decoded = LockstepEvents.keyPresses(bits);
            check(decoded.equals(expected), "decode " + bits);
            check(LockstepEvents.bits(decoded) == bits, "encode " + bits);
        }
        check(LockstepEvents.bits(new Input(false, false, false, false, true, false, false)) == 16, "jump is bit 4");
        System.out.println("LockstepEventsCheck ok");
    }
}
