package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.PlayerSync;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.ai.attributes.AttributeInstance;
import net.minecraft.world.entity.ai.attributes.AttributeModifier;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** A mod's change to an attribute instance of the acting player reaches Horizon's player. */
@Mixin(AttributeInstance.class)
abstract class AttributeSyncMixin {
    @Inject(method = {"addTransientModifier", "addOrUpdateTransientModifier"}, at = @At("RETURN"))
    private void horizon$transient(AttributeModifier modifier, CallbackInfo ci) {
        PlayerSync.modifier((AttributeInstance) (Object) this, modifier, false);
    }

    @Inject(method = {"addPermanentModifier", "addOrReplacePermanentModifier"}, at = @At("RETURN"))
    private void horizon$permanent(AttributeModifier modifier, CallbackInfo ci) {
        PlayerSync.modifier((AttributeInstance) (Object) this, modifier, true);
    }

    @Inject(method = "removeModifier(Lnet/minecraft/resources/Identifier;)Z", at = @At("RETURN"))
    private void horizon$removed(Identifier id, CallbackInfoReturnable<Boolean> cir) {
        if (cir.getReturnValueZ()) PlayerSync.removed((AttributeInstance) (Object) this, id);
    }

    @Inject(method = "setBaseValue", at = @At("RETURN"))
    private void horizon$base(double value, CallbackInfo ci) {
        PlayerSync.base((AttributeInstance) (Object) this, value);
    }
}
