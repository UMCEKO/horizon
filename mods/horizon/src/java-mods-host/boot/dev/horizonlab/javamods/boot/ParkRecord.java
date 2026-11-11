package dev.horizonlab.javamods.boot;

/** What Java hands the window when it parks at its player step: what mods asked of the step so far this tick. */
public final class ParkRecord {
    private ParkRecord() {}

    private static final byte[] EMPTY = new byte[0];

    public static byte[] take() {
        return EMPTY;
    }
}
