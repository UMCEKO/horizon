package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientBridge;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.entity.MoverType;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Movement is native: the window's physics moves this player, whose level holds only block entities. */
@Mixin(LocalPlayer.class)
abstract class LocalPlayerMoveMixin {
    @Inject(method = "move", at = @At("HEAD"), cancellable = true)
    private void horizon$move(MoverType type, Vec3 delta, CallbackInfo info) {
        info.cancel();
        if (type == MoverType.SELF) ClientBridge.move((LocalPlayer) (Object) this);
    }
}
