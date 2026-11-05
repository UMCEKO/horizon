package dev.horizonlab.javamods.boot.mixin;

import java.util.Map;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.level.pathfinder.PathType;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The raw map of `Mob` the AI group carries. */
@Mixin(Mob.class)
public interface MobAccess {
    @Accessor("pathfindingMalus") Map<PathType, Float> horizon$pathfindingMalus();
}
