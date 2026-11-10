package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.Backed;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;

@Mixin(Entity.class)
public abstract class EntityBackedMixin implements Backed {
    @Unique
    private boolean horizon$backed;

    @Override
    public boolean horizon$backed() {
        return horizon$backed;
    }

    @Override
    public void horizon$backed(boolean backed) {
        horizon$backed = backed;
    }
}
