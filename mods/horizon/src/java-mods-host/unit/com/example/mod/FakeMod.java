package com.example.mod;

import java.util.function.Supplier;

/** A stand-in for a mod's class: calls whatever it is handed. */
public final class FakeMod {
    private FakeMod() {}

    public static boolean call(Supplier<Boolean> target) {
        return target.get();
    }
}
