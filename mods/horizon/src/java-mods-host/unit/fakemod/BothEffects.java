package fakemod;

import net.minecraft.client.particle.ParticleEngine;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;

/** A mod's client extension making both destroy and hit particles, the hit one inherited from `Effects`' sibling shape. */
public class BothEffects extends Effects {
    public boolean addHitEffects(BlockState state, Level level, BlockPos pos, Direction side, ParticleEngine engine) {
        return true;
    }
}
