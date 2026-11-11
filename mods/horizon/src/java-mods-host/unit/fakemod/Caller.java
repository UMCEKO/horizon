package fakemod;

import java.util.function.Supplier;

/** Stands for a mod's code on the stack. */
public final class Caller {
    public static <T> T call(Supplier<T> s) {
        return s.get();
    }
}
