package dev.horizonlab.javamods.testmod.mixin;

import dev.horizonlab.javamods.testmod.Checks;
import dev.horizonlab.javamods.testmod.RouteState;
import net.minecraft.server.ServerScoreboard;
import net.minecraft.world.scores.PlayerTeam;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Handlers after `ServerScoreboard`'s team overrides, which call `Scoreboard`'s method first. */
@Mixin(ServerScoreboard.class)
public abstract class ServerScoreboardRouteMixin {
    @Inject(method = "addPlayerToTeam", at = @At("RETURN"))
    private void horizon$joined(String member, PlayerTeam team, CallbackInfoReturnable<Boolean> cir) {
        if (!team.getName().equals("red")) return;
        var inner = RouteState.teamLog.isEmpty() ? "" : RouteState.teamLog.get(RouteState.teamLog.size() - 1);
        RouteState.teamLog.add("server:join");
        var board = (ServerScoreboard) (Object) this;
        Checks.check("team_join_routed", () -> inner.equals("scoreboard:join") && cir.getReturnValue());
        Checks.check("team_view_matches", () -> board.getPlayersTeam(member) != null && board.getPlayersTeam(member).getName().equals("red"));
    }

    @Inject(method = "removePlayerFromTeam(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)V", at = @At("RETURN"))
    private void horizon$left(String member, PlayerTeam team, CallbackInfo ci) {
        if (!team.getName().equals("red")) return;
        var inner = RouteState.teamLog.isEmpty() ? "" : RouteState.teamLog.get(RouteState.teamLog.size() - 1);
        RouteState.teamLog.add("server:leave");
        Checks.check("team_leave_routed", () -> inner.equals("scoreboard:leave"));
    }
}
