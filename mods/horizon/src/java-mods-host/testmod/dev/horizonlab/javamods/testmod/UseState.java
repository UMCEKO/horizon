package dev.horizonlab.javamods.testmod;

import net.minecraft.world.phys.HitResult;

/** What the patched use paths counted between calls. */
public final class UseState {
    private UseState() {}

    public static int pearlUses;
    public static int bucketRays;
    public static int transforms;
    public static HitResult.Type lastBucketType;
}
