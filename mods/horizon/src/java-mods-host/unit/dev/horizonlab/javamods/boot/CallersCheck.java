package dev.horizonlab.javamods.boot;

import com.example.mod.FakeMod;

/** Who made a call: a mod's class counts, Horizon's and the game's own do not, and skipped prefixes are looked through. */
public final class CallersCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) {
        check(FakeMod.call(() -> Callers.byMod("dev.horizonlab.javamods.boot.CallersCheck")), "a mod's call counts");
        check(!Callers.byMod(), "Horizon's own call does not");
        check(!FakeMod.call(() -> Callers.byMod("com.example.mod.", "dev.horizonlab.javamods.boot.CallersCheck")), "a skipped mod prefix leaves Horizon's frame, which does not count");
    }
}
