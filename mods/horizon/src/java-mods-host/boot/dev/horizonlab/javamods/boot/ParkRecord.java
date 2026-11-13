package dev.horizonlab.javamods.boot;

import java.nio.ByteBuffer;

/** What Java hands the window when it parks at its player step: `int len` and the motion log (`MotionLog.take`), then `FovHook`'s target. */
public final class ParkRecord {
    private ParkRecord() {}

    public static byte[] take() {
        byte[] log = MotionLog.take();
        byte[] fov = FovHook.take();
        return ByteBuffer.allocate(4 + log.length + fov.length).putInt(log.length).put(log).put(fov).array();
    }
}
