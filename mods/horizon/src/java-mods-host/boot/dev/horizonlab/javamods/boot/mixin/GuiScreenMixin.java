package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientBridge;
import net.minecraft.client.gui.Gui;
import net.minecraft.client.gui.screens.Screen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A screen opens in Java and shows in Horizon's window, which has no Java mouse, narrator or title screen to fall back to. */
@Mixin(Gui.class)
abstract class GuiScreenMixin {
    @Shadow
    private Screen screen;

    @Inject(method = "setScreen", at = @At("HEAD"), cancellable = true)
    private void horizon$setScreen(Screen next, CallbackInfo info) {
        info.cancel();
        Screen previous = screen;
        screen = next;
        ClientBridge.setScreen(previous, next);
    }
}
