package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.gui.Gui;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.ModifyVariable;
import org.spongepowered.asm.mixin.injection.Redirect;

/** The title screen is the window's: with no level and no screen, this client shows nothing. */
@Mixin(Gui.class)
abstract class TitleScreenMixin {
    /** A redirect cannot return null, and the real constructor loads a panorama this client has no GPU for. */
    @Redirect(method = "setScreen", at = @At(value = "NEW", target = "()Lnet/minecraft/client/gui/screens/TitleScreen;"))
    private TitleScreen horizon$title() {
        try {
            return (TitleScreen) sun.reflect.ReflectionFactory.getReflectionFactory().newConstructorForSerialization(TitleScreen.class, Object.class.getDeclaredConstructor()).newInstance();
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    @ModifyVariable(method = "setScreen", at = @At(value = "STORE", ordinal = 0), argsOnly = true)
    private Screen horizon$none(Screen screen) {
        return screen instanceof TitleScreen ? null : screen;
    }
}
