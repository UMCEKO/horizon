package dev.horizonlab.javamods.boot;

/** What Java hands the window when it parks at its player step: what mods decided earlier this tick (`FovHook`'s target). */
public final class ParkRecord {
    private ParkRecord() {}

    public static byte[] take() {
        return FovHook.take();
    }
}
