package dev.horizonlab.javamods.testmod.mixin;

import dev.horizonlab.javamods.testmod.Checks;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.portal.TeleportTransition;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Records a player's teleport to another level and the level an `addPlayer` joins, on routed vanilla methods. */
@Mixin(value = ServerPlayer.class)
public abstract class JoinRouteMixin {
    @Inject(method = "teleport", at = @At("TAIL"))
    private void horizon$teleportTail(TeleportTransition transition, CallbackInfoReturnable<ServerPlayer> cir) {
        var target = transition.newLevel().dimension().identifier().toString();
        Checks.check("teleport_tail_" + target.substring(target.indexOf(':') + 1), () -> cir.getReturnValue() == (Object) this);
    }
}
