// game-module-cfg: feature java-mods
//! The scoreboard's team methods, on the server's scoreboard.

use super::*;

horizon_java_host::java_bridges! { table for Game<'static> {
    route "net/minecraft/server/ServerScoreboard".addPlayerToTeam
    "(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)Z" if "@dev/horizonlab/javamods/boot/Routing.serverScoreboard" =>
    fn route_server_team_join(game, member: Text, team: TeamName) -> bool {
        let joined = game.server_team_join_body(&member.0, &team.0);
        game.java_scoreboard_resend();
        joined
    }

    route "net/minecraft/server/ServerScoreboard".removePlayerFromTeam
    "(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)V" if "@dev/horizonlab/javamods/boot/Routing.serverScoreboard" =>
    fn route_server_team_leave(game, member: Text, team: TeamName) -> () {
        game.server_team_leave_body(&member.0, &team.0);
        game.java_scoreboard_resend();
    }

    route "net/minecraft/world/scores/Scoreboard".addPlayerToTeam
    "(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)Z" if "@dev/horizonlab/javamods/boot/Routing.serverScoreboard" =>
    fn route_team_join(game, member: Text, team: TeamName) -> bool {
        let joined = game.team_join_body(&member.0, &team.0);
        game.java_scoreboard_resend();
        joined
    }

    route "net/minecraft/world/scores/Scoreboard".removePlayerFromTeam
    "(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)V" if "@dev/horizonlab/javamods/boot/Routing.serverScoreboard" =>
    fn route_team_leave(game, member: Text, team: TeamName) -> () {
        game.team_leave_body(&member.0, &team.0);
        game.java_scoreboard_resend();
    }

    route "net/minecraft/world/scores/Scoreboard".onTeamAdded
    "(Lnet/minecraft/world/scores/PlayerTeam;)V" if "@dev/horizonlab/javamods/boot/Routing.serverScoreboard" =>
    fn route_team_added(_game, _team: Skip) -> () {}

    route "net/minecraft/world/scores/Scoreboard".onTeamRemoved
    "(Lnet/minecraft/world/scores/PlayerTeam;)V" if "@dev/horizonlab/javamods/boot/Routing.serverScoreboard" =>
    fn route_team_removed(_game, _team: Skip) -> () {}
} }
