package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.Shells;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.client.gui.screens.inventory.AbstractRecipeBookScreen;
import net.minecraft.client.gui.screens.inventory.InventoryScreen;
import net.minecraft.client.input.CharacterEvent;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** A shell's vanilla handler bodies never run: native owns the screen, `Shells` answers who takes an input. */
@Mixin(value = {AbstractContainerScreen.class, AbstractRecipeBookScreen.class, InventoryScreen.class}, priority = 100)
abstract class ShellMixin {
    @Inject(method = "mouseClicked", at = @At("HEAD"), cancellable = true, require = 0)
    private void horizon$clicked(MouseButtonEvent event, boolean doubleClick, CallbackInfoReturnable<Boolean> info) {
        if (Shells.is(this)) info.setReturnValue(Shells.clicked((Screen) (Object) this, event, doubleClick));
    }

    @Inject(method = "mouseReleased", at = @At("HEAD"), cancellable = true, require = 0)
    private void horizon$released(MouseButtonEvent event, CallbackInfoReturnable<Boolean> info) {
        if (Shells.is(this)) info.setReturnValue(Shells.released((Screen) (Object) this, event));
    }

    @Inject(method = "mouseDragged", at = @At("HEAD"), cancellable = true, require = 0)
    private void horizon$dragged(MouseButtonEvent event, double dx, double dy, CallbackInfoReturnable<Boolean> info) {
        if (Shells.is(this)) info.setReturnValue(Shells.dragged((Screen) (Object) this, event, dx, dy));
    }

    @Inject(method = "mouseScrolled", at = @At("HEAD"), cancellable = true, require = 0)
    private void horizon$scrolled(double x, double y, double sx, double sy, CallbackInfoReturnable<Boolean> info) {
        if (Shells.is(this)) info.setReturnValue(Shells.scrolled((Screen) (Object) this, x, y, sx, sy));
    }

    @Inject(method = "keyPressed", at = @At("HEAD"), cancellable = true, require = 0)
    private void horizon$key(KeyEvent event, CallbackInfoReturnable<Boolean> info) {
        if (Shells.is(this)) info.setReturnValue(Shells.key((Screen) (Object) this, event));
    }

    @Inject(method = "charTyped", at = @At("HEAD"), cancellable = true, require = 0)
    private void horizon$typed(CharacterEvent event, CallbackInfoReturnable<Boolean> info) {
        if (Shells.is(this)) info.setReturnValue(Shells.typed((Screen) (Object) this, event));
    }

    @Inject(method = "tick", at = @At("HEAD"), cancellable = true, require = 0)
    private void horizon$tick(CallbackInfo info) {
        if (Shells.is(this)) info.cancel();
    }

    @Inject(method = "containerTick", at = @At("HEAD"), cancellable = true, require = 0)
    private void horizon$containerTick(CallbackInfo info) {
        if (Shells.is(this)) info.cancel();
    }

    @Inject(method = "removed", at = @At("HEAD"), cancellable = true, require = 0)
    private void horizon$removed(CallbackInfo info) {
        if (Shells.is(this)) info.cancel();
    }
}
