package dev.horizonlab.javamods.boot.mixin;

import java.util.concurrent.atomic.AtomicLong;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** A legacy random's raw state, handed back to the native roll that lent it. */
@Mixin(LegacyRandomSource.class)
public interface LegacySeedAccess {
    @Accessor("seed") AtomicLong horizon$seed();
}
