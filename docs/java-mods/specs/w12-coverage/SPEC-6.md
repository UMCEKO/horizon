# SPEC-6: scoreboard and teams (Waystones, Lootr)

Design: `DESIGN.md` §2.6 row 6. Needs SPEC-3 (`Recv::Scoreboard`, `PlayerTeam` argument). Size: ~300 changed lines.

Paths as SPEC-2; `L/` = `crates/horizon-client/src/`. Read first: vanilla `Scoreboard.java` (`addPlayerToTeam`, `removePlayerFromTeam` both overloads, `addPlayerTeam`, `removePlayerTeam`, `onTeamAdded`, `onTeamRemoved`, `entityRemoved`, and how teams load from the save) and `ServerScoreboard.java` (its overrides, which call `super` and then send packets and `setDirty`) in `.local/minecraft/26.3/sources/server/net/minecraft/{world/scores,server}/`; `L/scoreboard_cmd.rs` (`Env`, the `team.*` arms at ~873–944); `G/scoreboard.rs` (`GameEnv`, ~946); `G/persist.rs` ~2157 (teams from the save); `horizon-client-core/src/scoreboard.rs` (`add_team`, `remove_team`, `add_to_team`, `remove_from_team`, `pack`).

Comments in code: one line at most.

## Keys
| Key | Kind | Mod |
|---|---|---|
| `net/minecraft/server/ServerScoreboard#addPlayerToTeam(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)Z` | edge RETURN | waystones |
| `net/minecraft/server/ServerScoreboard#removePlayerFromTeam(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)V` | edge RETURN | waystones |
| `net/minecraft/world/scores/Scoreboard#addPlayerToTeam(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)Z` | edge RETURN | lootr |
| `net/minecraft/world/scores/Scoreboard#removePlayerFromTeam(…)` | inject (SPEC-1's analysis gives its kind and descriptor) | lootr |
| `net/minecraft/world/scores/Scoreboard#onTeamAdded(Lnet/minecraft/world/scores/PlayerTeam;)V` | inject | lootr |
| `net/minecraft/world/scores/Scoreboard#onTeamRemoved(Lnet/minecraft/world/scores/PlayerTeam;)V` | inject | lootr |

`ServerScoreboard` overrides these and calls `super` first (check each in the source): the `Scoreboard` routes are reached from inside the `ServerScoreboard` body. W12 §2.3's overrider rule applies: native's `ServerScoreboard` body port calls the gated `Scoreboard` port at the super-call point.

## 1. Seams (one `Game` method per vanilla method, gate first)
In `G/scoreboard.rs` (an `impl Game<'_>` block next to `GameEnv`):
```rust
/// `ServerScoreboard.addPlayerToTeam`: the super call, then the team packet to every player.
pub(in crate::game) fn server_team_join(&mut self, member: &str, team: &str) -> bool { route!(…ServerScoreboard#addPlayerToTeam…, Recv::Scoreboard, (member, TeamArg(team))); self.server_team_join_body(member, team) }
/// `Scoreboard.addPlayerToTeam`: membership only.
pub(in crate::game) fn team_join(&mut self, member: &str, team: &str) -> bool { route!(…Scoreboard#addPlayerToTeam…); self.team_join_body(member, team) }
```
and the same pairs for `removePlayerFromTeam` (`server_team_leave`/`team_leave`), `onTeamAdded` (`server_team_added`/`team_added`), `onTeamRemoved`. Bodies:
- `server_team_join_body` = `self.team_join(member, team)` (the gated super call) then what native does today after a join (its team packet broadcast; find it: `lan/` sends `SetPlayerTeam` on board changes) and `setDirty` (native's save flag).
- `team_join_body` = `self.score.board.add_to_team(member, team)`.
- Removal mirrors it. `team_added`/`team_removed` bodies are what `add_team`/`remove_team` callers do after the board change.
- `Env` (`L/scoreboard_cmd.rs`) gains `fn team_join(&mut self, member: &str, team: &str) -> bool`, `team_leave`, `team_add(&mut self, name) -> …`, `team_remove`, with default bodies equal to today's direct board calls (the `Mock` in `scoreboard_cmd_tests.rs` keeps working). `GameEnv` overrides them with the `server_*` methods. The `team.*` arms call `env.team_*` instead of `env.sb_mut().…`.
- `G/persist.rs` ~2157: follow vanilla's load. If vanilla's team load calls `addPlayerToTeam` (it does in 1.21's `Scoreboard.loadPlayerTeams`-style code; confirm in 26.3), call `self.team_join` there (the `Scoreboard` one, not the server one: no packets at load).
- `entity_removed`: if vanilla's `entityRemoved` reaches `removePlayerFromTeam`, call `server_team_leave` from native's equivalent.

## 2. The scoreboard view (Java)
Handlers read the scoreboard (team members, a player's team). Java's `ServerScoreboard` (on the `MinecraftServer` view) holds nothing today.
- Native keeps `score.revision: u64`, bumped by every team or membership change (in `team_*_body` and anywhere else the board's teams change; grep `teams` writes in `horizon-client-core/src/scoreboard.rs`).
- `Recv::Scoreboard` writes `long revision` and, when it differs from the revision last sent (a thread-local, as SPEC-2's `SENT`), `bool true` + the teams group: `int n`, n × (`text name`, `bytes` the team's packed tag from `Scoreboard::pack`'s team entry, `int m`, m × `text member`). Otherwise `bool false`.
- Java (`B/ScoreboardView.java`, new): on a teams group, make the Java scoreboard equal it, **raw**: create/remove `PlayerTeam`s and memberships through the fields (an accessor mixin on `Scoreboard`'s `teamsByName` and `teamsByPlayer`), never through `addPlayerToTeam`/`addPlayerTeam` (those would fire the mods' handlers a second time). Team properties from the packed tag through vanilla's own team codec (`PlayerTeam`'s `pack`/load pair in 26.3).
- Writes by mod code to the Java scoreboard: `addPlayerToTeam`/`removePlayerFromTeam` are themselves route stubs on a backed receiver, so they reach native. Mark the Java `ServerScoreboard` backed (`Routing`'s backed test for this family: the instance is `GameBridge.server.getScoreboard()`). Other writers mods call (`addPlayerTeam`, `removePlayerTeam`, `PlayerTeam` setters) come from `java_mods.py check --analysis`'s closure for family `scoreboard`: add a bridge for each the showcase pack reaches, in `G/java_bridges/scoreboard.rs` (new), writing native and bumping the revision.

## 3. Body bridges (`G/java_bridges/scoreboard.rs`)
One `route … if backed` entry per key, calling the `*_body` fn. After a body that changed membership, the revision moved, so the receiver's next fill resends the teams group; the handlers after the body (RETURN) need it now: the bridge re-sends the teams group to Java before returning (same writer as §2).

## 4. Tests
- Rust (`G/scoreboard.rs` tests): `team_args_write_the_packed_team`; `revision_moves_on_join_and_leave`; with a `Mock` `Env` (no java-mods): `team.join` still joins (default `Env` bodies).
- Test mod mixins (`J/testmod/…/mixin/ScoreboardRouteMixin.java`): `@Inject(method = "addPlayerToTeam", at = @At("RETURN"))` on `Scoreboard` records `(member, team, cir.getReturnValue(), team.getPlayers().contains(member))`; one on `ServerScoreboard` the same. The `bridges` session runs `/team add red`, `/team join red @s`, `/team leave @s`.
  Checks (`ChecksRouting`): `team_join_routed`: the `Scoreboard` RETURN fired once with `true` and saw the member inside the team; the `ServerScoreboard` RETURN fired after it (order: inner before outer, as vanilla's super call); `team_leave_routed`: likewise for the leave; `team_view_matches`: inside the handler, `getPlayersTeam(member).getName()` is `red`.
- Parity: `python3 tools/parity.py --changed-files <files> --list`, run what it names (team command harnesses).
- `cargo test -p horizon-client --features java-mods --lib scoreboard`, clippy both ways, `java_mods.py build/check/unit`, one `bridges` run.

## Done bar
Checks pass; the gates for the six keys are in `route_gates.tsv`; their `deferred.tsv` rows are gone; `java_routing.py lint` passes.

## For the lead (live) and the user
- Lead: showcase install, `/team add a`, `/team join a @s`: no exception; the boot shows the six routes `gated`.
- User (needs two players, Waystones' team sharing): dedicated server, players A and B. A places and names a waystone. Without a team, B can't see it in a warp stone's list. `/team add t`, `/team join t A`, `/team join t B`: if Waystones' config shares waystones in teams (check its config file in the install), B now sees A's waystone without having visited it. `/team leave B`: it disappears for B. Expected as on NeoForge.
