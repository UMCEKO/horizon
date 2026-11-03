package dev.horizonlab.javamods.boot;

/** When the mods' HUD stops animating. */
public final class HudAnimation {
    private HudAnimation() {}

    /** An identical recording ends it only if real time passed since the one before: with none, a box that follows frame time couldn't have moved. */
    public static boolean ends(boolean identical, float realtimeDeltaTicks) {
        return identical && realtimeDeltaTicks > 0;
    }
}
