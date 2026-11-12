package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.util.function.Predicate;

/** What mod code wrote to the local player's motion state since the window's last step, in order: native's next step starts from it (`G/java_lockstep.rs` `read_motion_writes`). */
public final class MotionLog {
    private MotionLog() {}

    static final byte VELOCITY = 1, POSITION = 2, YAW = 3, PITCH = 4, GROUND = 5, FALL_RESET = 6, SPRINT = 7, ABILITIES = 8, OLD_POS = 9;

    /** The local player while the client is in lockstep, else null. */
    public static volatile Object watching;
    /** Horizon's own fills of the player (`ClientBridge.fill`, `turnPlayer`, `move`) are not mods' writes. */
    public static boolean filling;
    /** Whether a class is mod code; a seam for checks. */
    static Predicate<Class<?>> modClass = Shells::isMods;

    private static final String ENTITY = "net.minecraft.world.entity.";
    private static final String MIXINS = "dev.horizonlab.javamods.boot.mixin.";
    private static final StackWalker WALKER = StackWalker.getInstance(StackWalker.Option.RETAIN_CLASS_REFERENCE);
    private static final Object LOCK = new Object();
    private static final ByteArrayOutputStream BYTES = new ByteArrayOutputStream();
    private static final DataOutputStream OUT = new DataOutputStream(BYTES);
    private static int entries;

    private static boolean watched(Object e) {
        return e != null && e == watching && !filling;
    }

    /** The first frame below this log, the mixin handler and vanilla's entity code is a mod's. */
    static boolean byMod() {
        return WALKER.walk(frames -> frames.map(StackWalker.StackFrame::getDeclaringClass)
                .dropWhile(c -> c == MotionLog.class || c.getName().startsWith(MIXINS) || c.getName().startsWith(ENTITY))
                .findFirst().map(modClass::test).orElse(false));
    }

    private interface Body {
        void write(DataOutputStream out) throws IOException;
    }

    private static void record(byte kind, Body body) {
        synchronized (LOCK) {
            try {
                OUT.writeByte(kind);
                body.write(OUT);
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
            entries++;
        }
    }

    public static void velocity(Object e, double x, double y, double z) {
        if (watched(e) && byMod()) record(VELOCITY, out -> { out.writeDouble(x); out.writeDouble(y); out.writeDouble(z); });
    }

    public static void position(Object e, double x, double y, double z) {
        if (watched(e) && byMod()) record(POSITION, out -> { out.writeDouble(x); out.writeDouble(y); out.writeDouble(z); });
    }

    public static void yaw(Object e, float y) {
        if (watched(e) && byMod()) record(YAW, out -> out.writeFloat(y));
    }

    public static void pitch(Object e, float x) {
        if (watched(e) && byMod()) record(PITCH, out -> out.writeFloat(x));
    }

    public static void ground(Object e, boolean g) {
        if (watched(e) && byMod()) record(GROUND, out -> out.writeBoolean(g));
    }

    public static void fallReset(Object e) {
        if (watched(e) && byMod()) record(FALL_RESET, out -> {});
    }

    public static void sprint(Object e, boolean s) {
        if (watched(e) && byMod()) record(SPRINT, out -> out.writeBoolean(s));
    }

    public static void abilities(Object e, boolean flying, boolean mayfly, float flyingSpeed) {
        if (watched(e) && byMod()) record(ABILITIES, out -> { out.writeBoolean(flying); out.writeBoolean(mayfly); out.writeFloat(flyingSpeed); });
    }

    /** Where vanilla takes `xo/yo/zo` (`Entity.commonTick` in `ClientLevel.tickNonPassenger`), whoever calls it. */
    public static void oldPos(Object e) {
        if (watched(e)) record(OLD_POS, out -> {});
    }

    /** `int n`, then the entries since the last take; the log is empty after. */
    public static byte[] take() {
        synchronized (LOCK) {
            var bytes = new ByteArrayOutputStream(4 + BYTES.size());
            try {
                new DataOutputStream(bytes).writeInt(entries);
                BYTES.writeTo(bytes);
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
            BYTES.reset();
            entries = 0;
            return bytes.toByteArray();
        }
    }
}
