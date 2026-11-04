package dev.horizonlab.javamods.boot.mixin;

import java.util.Optional;
import net.minecraft.network.chat.Component;
import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** `Entity`'s private data accessors, for writes that skip the setters. */
@Mixin(Entity.class)
public interface EntityDataAccess {
    @Accessor("DATA_SHARED_FLAGS_ID") static EntityDataAccessor<Byte> horizon$flags() {
        throw new AssertionError();
    }

    @Accessor("DATA_AIR_SUPPLY_ID") static EntityDataAccessor<Integer> horizon$air() {
        throw new AssertionError();
    }

    @Accessor("DATA_CUSTOM_NAME") static EntityDataAccessor<Optional<Component>> horizon$name() {
        throw new AssertionError();
    }

    @Accessor("DATA_SILENT") static EntityDataAccessor<Boolean> horizon$silent() {
        throw new AssertionError();
    }

    @Accessor("DATA_NO_GRAVITY") static EntityDataAccessor<Boolean> horizon$noGravity() {
        throw new AssertionError();
    }
}
