package dev.horizonlab.javamods.boot.mixin;

import java.util.Map;
import net.minecraft.core.Holder;
import net.minecraft.world.effect.MobEffect;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.entity.EntityEquipment;
import net.minecraft.world.entity.LivingEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** Raw fields of `LivingEntity` the group bytes carry. */
@Mixin(LivingEntity.class)
public interface LivingAccess {
    @Accessor("absorptionAmount") void horizon$absorption(float value);

    @Accessor("activeEffects") Map<Holder<MobEffect>, MobEffectInstance> horizon$effects();

    @Accessor("equipment") EntityEquipment horizon$equipment();
}
