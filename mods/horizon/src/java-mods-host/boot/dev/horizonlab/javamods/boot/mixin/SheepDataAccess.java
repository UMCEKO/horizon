package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.world.entity.animal.sheep.Sheep;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** A sheep's wool byte: colour in the low four bits, sheared above. */
@Mixin(Sheep.class)
public interface SheepDataAccess {
    @Accessor("DATA_WOOL_ID") static EntityDataAccessor<Byte> horizon$wool() {
        throw new AssertionError();
    }
}
