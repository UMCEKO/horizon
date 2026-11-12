package fakemod;

import net.minecraft.client.particle.ParticleEngine;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;

/** Stands for a mod's client extension that makes only its destroy particles; the check only reflects on it. */
public class Effects {
    public boolean addDestroyEffects(BlockState state, Level level, BlockPos pos, ParticleEngine engine) {
        return true;
    }
}
