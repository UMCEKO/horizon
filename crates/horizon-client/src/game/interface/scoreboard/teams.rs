//! `Scoreboard`'s and `ServerScoreboard`'s team methods; the routed seams are in `java/scoreboard.rs`.
use super::Game;

impl Game<'_> {
    /// `ServerScoreboard.addPlayerToTeam` after any mod's patch: `Scoreboard`'s, then the packets (sent from the board's changes) and the save flag.
    pub(in crate::game) fn server_team_join_body(&mut self, member: &str, team: &str) -> bool {
        self.team_join(member, team)
    }

    /// `Scoreboard.addPlayerToTeam`: membership only.
    pub(in crate::game) fn team_join_body(&mut self, member: &str, team: &str) -> bool {
        self.score.board.add_to_team(member, team)
    }

    /// `ServerScoreboard.removePlayerFromTeam(String, PlayerTeam)` after any mod's patch.
    pub(in crate::game) fn server_team_leave_body(&mut self, member: &str, team: &str) {
        self.team_leave(member, team);
    }

    /// `Scoreboard.removePlayerFromTeam(String, PlayerTeam)`: vanilla throws unless the member is on that team.
    pub(in crate::game) fn team_leave_body(&mut self, member: &str, team: &str) {
        if self.score.board.players_team(member).is_some_and(|t| t.name == team) {
            self.score.board.remove_from_team(member);
        }
    }

    /// `Scoreboard.addPlayerTeam` for a name no team has, then `ServerScoreboard.onTeamAdded`.
    pub(in crate::game) fn server_team_add(&mut self, name: &str) {
        self.score.board.add_team(name);
        self.team_added(name);
    }

    /// `Scoreboard.removePlayerTeam`, then `ServerScoreboard.onTeamRemoved`.
    pub(in crate::game) fn server_team_remove(&mut self, name: &str) {
        let Some(team) = self.score.board.team(name).cloned() else {
            return;
        };
        self.score.board.remove_team(name);
        self.team_removed(team);
    }

    /// `Scoreboard.removePlayerFromTeam(String)`: the member's team, if any, through the two-argument method.
    pub(in crate::game) fn server_team_leave_member(&mut self, member: &str) -> bool {
        let Some(team) = self.score.board.players_team(member).map(|t| t.name.clone()) else {
            return false;
        };
        self.server_team_leave(member, &team);
        true
    }

    /// `Scoreboard.entityRemoved` for a dead entity that is not a player.
    pub(in crate::game) fn scoreboard_entity_removed(&mut self, holder: &str) {
        self.score.board.reset_all(holder);
        self.server_team_leave_member(holder);
    }
}

#[cfg(not(feature = "java-mods"))]
impl Game<'_> {
    pub(in crate::game) fn server_team_join(&mut self, member: &str, team: &str) -> bool {
        self.server_team_join_body(member, team)
    }

    pub(in crate::game) fn team_join(&mut self, member: &str, team: &str) -> bool {
        self.team_join_body(member, team)
    }

    pub(in crate::game) fn server_team_leave(&mut self, member: &str, team: &str) {
        self.server_team_leave_body(member, team);
    }

    pub(in crate::game) fn team_leave(&mut self, member: &str, team: &str) {
        self.team_leave_body(member, team);
    }

    pub(in crate::game) fn team_added(&mut self, _name: &str) {}

    pub(in crate::game) fn team_removed(&mut self, _team: horizon_client::scoreboard::Team) {}
}
