package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientBridge;
import dev.horizonlab.javamods.boot.Lockstep;
import dev.horizonlab.javamods.boot.ParkRecord;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** In a lockstep tick the window's player step runs here, at `LocalPlayer.aiStep`, and its result fills this player. */
@Mixin(LocalPlayer.class)
abstract class LocalPlayerStepMixin {
    @Inject(method = "aiStep", at = @At("HEAD"), cancellable = true)
    private void horizon$step(CallbackInfo info) {
        LocalPlayer self = (LocalPlayer) (Object) this;
        if (!Lockstep.active() || self != Minecraft.getInstance().player) return;
        byte[] fill = Lockstep.park(Lockstep.Point.M, ParkRecord.take());
        ClientBridge.fill(self, fill);
        info.cancel();
    }
}
