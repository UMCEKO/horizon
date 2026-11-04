package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.world.entity.LightningBolt;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** Raw fields of `LightningBolt` the species group carries (`damage` is NeoForge's). */
@Mixin(LightningBolt.class)
public interface LightningBoltAccess {
    @Accessor("visualOnly") boolean horizon$visualOnly();

    @Accessor("visualOnly") void horizon$visualOnly(boolean visualOnly);

    @Accessor("damage") float horizon$damage();

    @Accessor("damage") void horizon$damage(float damage);

    @Accessor("life") int horizon$life();

    @Accessor("life") void horizon$life(int life);

    @Accessor("flashes") int horizon$flashes();

    @Accessor("flashes") void horizon$flashes(int flashes);
}
