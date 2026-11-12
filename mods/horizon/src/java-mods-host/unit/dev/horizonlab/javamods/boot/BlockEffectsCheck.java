package dev.horizonlab.javamods.boot;

/** Which client extensions count as a mod's own destroy or hit particles: those a mod's class declares. */
public final class BlockEffectsCheck {
    public static void main(String[] args) {
        if (BlockEffects.bits(fakemod.Effects.class) != BlockEffects.DESTROY) throw new AssertionError("destroy only");
        if (BlockEffects.bits(fakemod.BothEffects.class) != (BlockEffects.DESTROY | BlockEffects.HIT)) throw new AssertionError("both, one inherited");
        if (BlockEffects.bits(BlockEffects.class) != 0) throw new AssertionError("Horizon's own class is not a mod's");
        if (BlockEffects.bits(Object.class) != 0) throw new AssertionError("Object");
        System.out.println("BlockEffectsCheck ok");
    }
}
