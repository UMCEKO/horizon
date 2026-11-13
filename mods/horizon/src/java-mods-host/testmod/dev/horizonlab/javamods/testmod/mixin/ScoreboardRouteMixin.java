package dev.horizonlab.javamods.testmod.mixin;

import dev.horizonlab.javamods.testmod.Checks;
import dev.horizonlab.javamods.testmod.RouteState;
import net.minecraft.world.scores.PlayerTeam;
import net.minecraft.world.scores.Scoreboard;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Handlers after `Scoreboard`'s team methods: they see the change the body made, and run before `ServerScoreboard`'s own. */
@Mixin(Scoreboard.class)
public abstract class ScoreboardRouteMixin {
    @Inject(method = "addPlayerToTeam", at = @At("RETURN"))
    private void horizon$joined(String member, PlayerTeam team, CallbackInfoReturnable<Boolean> cir) {
        if (!team.getName().equals("red")) return;
        var board = (Scoreboard) (Object) this;
        RouteState.teamLog.add("scoreboard:join");
        Checks.check("team_join_inner_sees_member", () -> cir.getReturnValue() && team.getPlayers().contains(member) && board.getPlayersTeam(member) == team);
    }

    @Inject(method = "removePlayerFromTeam(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)V", at = @At("RETURN"))
    private void horizon$left(String member, PlayerTeam team, CallbackInfo ci) {
        if (!team.getName().equals("red")) return;
        var board = (Scoreboard) (Object) this;
        RouteState.teamLog.add("scoreboard:leave");
        Checks.check("team_leave_inner_sees_member_gone", () -> !team.getPlayers().contains(member) && board.getPlayersTeam(member) == null);
    }
}
