// game-module-cfg: feature java-mods
//! The team methods of `Scoreboard` and `ServerScoreboard` that mods' mixins can patch, and the teams Java's scoreboard view is kept equal to.

use super::java_routing::{Recv, TeamArg, route};
use super::*;
use horizon_client::scoreboard::Scoreboard;
use horizon_java_host::wire::Out;

thread_local! {
    /// The team revision Java's scoreboard was last made equal to.
    static SENT: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

impl Game<'_> {
    /// `ServerScoreboard.addPlayerToTeam`: the super call, then the team packet to every player.
    pub(in crate::game) fn server_team_join(&mut self, member: &str, team: &str) -> bool {
        route!(
            self,
            "net/minecraft/server/ServerScoreboard#addPlayerToTeam(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)Z",
            Recv::Scoreboard,
            (member, TeamArg::named(&self.score.board, team))
        );
        self.server_team_join_body(member, team)
    }

    /// `Scoreboard.addPlayerToTeam`: membership only.
    pub(in crate::game) fn team_join(&mut self, member: &str, team: &str) -> bool {
        route!(
            self,
            "net/minecraft/world/scores/Scoreboard#addPlayerToTeam(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)Z",
            Recv::Scoreboard,
            (member, TeamArg::named(&self.score.board, team))
        );
        self.team_join_body(member, team)
    }

    /// `ServerScoreboard.removePlayerFromTeam(String, PlayerTeam)`: the super call, then the team packet to every player.
    pub(in crate::game) fn server_team_leave(&mut self, member: &str, team: &str) {
        route!(
            self,
            "net/minecraft/server/ServerScoreboard#removePlayerFromTeam(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)V",
            Recv::Scoreboard,
            (member, TeamArg::named(&self.score.board, team))
        );
        self.server_team_leave_body(member, team);
    }

    /// `Scoreboard.removePlayerFromTeam(String, PlayerTeam)`: membership only.
    pub(in crate::game) fn team_leave(&mut self, member: &str, team: &str) {
        route!(
            self,
            "net/minecraft/world/scores/Scoreboard#removePlayerFromTeam(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)V",
            Recv::Scoreboard,
            (member, TeamArg::named(&self.score.board, team))
        );
        self.team_leave_body(member, team);
    }

    /// `Scoreboard.onTeamAdded`: nothing in the base class, so only a mod's patch runs.
    pub(in crate::game) fn team_added(&mut self, name: &str) {
        route!(
            self,
            "net/minecraft/world/scores/Scoreboard#onTeamAdded(Lnet/minecraft/world/scores/PlayerTeam;)V",
            Recv::Scoreboard,
            (TeamArg::named(&self.score.board, name))
        );
    }

    /// `Scoreboard.onTeamRemoved`: nothing in the base class, so only a mod's patch runs; `team` is the team as it was.
    pub(in crate::game) fn team_removed(&mut self, team: horizon_client::scoreboard::Team) {
        route!(
            self,
            "net/minecraft/world/scores/Scoreboard#onTeamRemoved(Lnet/minecraft/world/scores/PlayerTeam;)V",
            Recv::Scoreboard,
            (TeamArg::removed(team))
        );
    }

    /// The scoreboard receiver's head: the team revision, and the teams when Java's view lags behind it.
    pub(in crate::game) fn java_scoreboard_fill(&mut self, out: &mut Out) {
        self.load_scoreboard();
        fill(&self.score.board, out);
    }

    /// Makes Java's scoreboard equal to the board's teams now, after a body that changed them.
    pub(in crate::game) fn java_scoreboard_resend(&mut self) {
        if SENT.get() == Some(self.score.board.team_revision()) {
            return;
        }
        let mut out = Out::default();
        self.java_scoreboard_fill(&mut out);
        if self.java_request("scoreboard_view", &out.0).is_none() {
            SENT.set(None);
        }
    }

}

/// `long revision`, `bool lagging`, and when lagging `int n` then each team's packed tag with its members.
fn fill(board: &Scoreboard, out: &mut Out) {
    let revision = board.team_revision();
    out.long(revision as i64);
    let lagging = SENT.replace(Some(revision)) != Some(revision);
    out.bool(lagging);
    if lagging {
        let teams = board.teams_streamed();
        out.int(teams.len() as i32);
        for team in teams {
            out.bytes(&super::java_mods::write_tag(team.pack()).expect("a team encodes as NBT"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_java_host::wire::In;

    fn read_teams(out: &[u8]) -> (i64, bool, Vec<horizon_core::nbt::Tag>) {
        let mut reply = In(out);
        let revision = reply.long().unwrap();
        let lagging = reply.bool().unwrap();
        let mut teams = Vec::new();
        if lagging {
            for _ in 0..reply.int().unwrap() {
                teams.push(super::super::java_mods::read_tag(reply.bytes().unwrap()).unwrap());
            }
        }
        (revision, lagging, teams)
    }

    #[test]
    fn team_args_write_the_packed_team() {
        let mut board = Scoreboard::default();
        board.add_team("red");
        board.add_to_team("Steve", "red");
        let mut out = Out::default();
        super::super::java_routing::RouteArg::put(&TeamArg::named(&board, "red"), &mut out);
        let mut reply = In(&out.0);
        assert_eq!(reply.text(), Some("red"));
        let tag = super::super::java_mods::read_tag(reply.bytes().unwrap()).unwrap();
        assert_eq!(tag, board.team("red").unwrap().pack());
        // A team the board lacks has a name and no tag.
        let mut absent = Out::default();
        super::super::java_routing::RouteArg::put(&TeamArg::named(&board, "blue"), &mut absent);
        let mut reply = In(&absent.0);
        assert_eq!((reply.text(), reply.bytes()), (Some("blue"), Some(&[][..])));
    }

    #[test]
    fn the_teams_cross_only_when_the_revision_moved() {
        SENT.set(None);
        let mut board = Scoreboard::default();
        board.add_team("red");
        let write = |board: &Scoreboard| {
            let mut out = Out::default();
            fill(board, &mut out);
            out.0
        };
        let first = read_teams(&write(&board));
        assert!(first.1 && first.2.len() == 1);
        let again = read_teams(&write(&board));
        assert!(!again.1 && again.0 == first.0);
        board.add_to_team("Steve", "red");
        let moved = read_teams(&write(&board));
        assert!(moved.1 && moved.0 != first.0);
    }
}
