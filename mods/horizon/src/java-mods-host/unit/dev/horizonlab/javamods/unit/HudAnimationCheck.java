package dev.horizonlab.javamods.unit;

import dev.horizonlab.javamods.boot.HudAnimation;
import net.minecraft.client.DeltaTracker;

/** The animation's end rule, and the real-time delta the HUD's boxes follow. */
public final class HudAnimationCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) {
        check(HudAnimation.ends(true, 0.32F), "identical with time passed ends it");
        check(!HudAnimation.ends(true, 0F), "identical with no time passed doesn't");
        check(!HudAnimation.ends(false, 0.5F), "differing recordings keep it going");
        var timer = new DeltaTracker.Timer(20.0F, 0L, target -> target);
        timer.advanceRealTime(0);
        check(timer.getRealtimeDeltaTicks() == 0F, "no time");
        timer.advanceRealTime(16);
        check(Math.abs(timer.getRealtimeDeltaTicks() - 0.32F) < 1e-6, "16 ms is 0.32 ticks");
        timer.advanceRealTime(50);
        check(Math.abs(timer.getRealtimeDeltaTicks() - 0.68F) < 1e-6, "34 ms more is 0.68 ticks");
        System.out.println("HudAnimationCheck ok");
    }
}
