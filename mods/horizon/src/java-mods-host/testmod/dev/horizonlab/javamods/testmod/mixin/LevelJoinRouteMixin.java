package dev.horizonlab.javamods.testmod.mixin;

import dev.horizonlab.javamods.testmod.Checks;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Records the level of an `addPlayer`, vanilla's private join. */
@Mixin(ServerLevel.class)
public abstract class LevelJoinRouteMixin {
    @Inject(method = "addPlayer", at = @At("HEAD"))
    private void horizon$addPlayerHead(ServerPlayer player, CallbackInfo ci) {
        var level = ((ServerLevel) (Object) this).dimension().identifier().toString();
        Checks.check("add_player_head_" + level.substring(level.indexOf(':') + 1), () -> player.level() != null);
    }
}
