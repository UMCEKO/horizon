package dev.horizonlab.javamods.boot;

/** An entity view whose state Horizon owns: its bridged methods act natively. */
public interface Backed {
    boolean horizon$backed();

    void horizon$backed(boolean backed);
}
