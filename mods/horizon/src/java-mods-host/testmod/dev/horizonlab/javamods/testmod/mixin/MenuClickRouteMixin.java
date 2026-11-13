package dev.horizonlab.javamods.testmod.mixin;

import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.ContainerInput;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A no-op route on `doClick`: with it on, the vanilla menus native opens get their Java views. */
@Mixin(AbstractContainerMenu.class)
public abstract class MenuClickRouteMixin {
    @Inject(method = "doClick", at = @At("HEAD"))
    private void horizon$head(int slot, int button, ContainerInput input, Player player, CallbackInfo ci) {}
}
