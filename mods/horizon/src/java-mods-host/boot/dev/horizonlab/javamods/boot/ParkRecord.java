package dev.horizonlab.javamods.boot;

import java.nio.ByteBuffer;

/** What Java hands the window when it parks at its player step: what mods asked of the step so far this tick. */
public final class ParkRecord {
    private ParkRecord() {}

    /** `int len` and the motion log (`MotionLog.take`). */
    public static byte[] take() {
        byte[] log = MotionLog.take();
        return ByteBuffer.allocate(4 + log.length).putInt(log.length).put(log).array();
    }
}
