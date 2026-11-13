package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientBridge;
import dev.horizonlab.javamods.boot.Shells;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Gui;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.core.registries.BuiltInRegistries;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The screen vanilla builds for a menu native forwarded: a mod's own class opens as a Java screen, vanilla's is a shell when a mod listens to screen events, else the window's stand-in. */
@Mixin(Gui.class)
abstract class ShellAdoptMixin {
    @Inject(method = "setScreen", at = @At("HEAD"), cancellable = true)
    private void horizon$adopt(Screen screen, CallbackInfo info) {
        if (!Shells.forwarding || !(screen instanceof AbstractContainerScreen<?> container)) return;
        var player = Minecraft.getInstance().player;
        if (player == null || container.getMenu() != player.containerMenu) return;
        Shells.forwarded(container.getMenu());
        if (Shells.isMods(screen.getClass())) {
            Shells.modScreen(Minecraft.getInstance());
            return;
        }
        if (Shells.wantedNow()) {
            Shells.adopt(screen);
            return;
        }
        info.cancel();
        Shells.retire(Minecraft.getInstance());
        ClientBridge.standIn();
    }
}
