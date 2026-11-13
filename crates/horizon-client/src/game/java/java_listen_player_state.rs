// game-module-cfg: feature java-mods
// java-listener-family
//! A player's spawn point, wake-up and advancements, and a changed game rule, posted to the Java mods' NeoForge events.

use super::java_listener::{Family, Subscriptions, crossed, with_game};
use crate::game::Game;
use horizon_core::position::BlockPos;
use horizon_java_host::wire::Out;
use mod_api::{
    Flow, GameRuleChangedEvent, HookId, PlayerAdvancementDoneEvent, PlayerAdvancementProgressEvent,
    PlayerBedLeaveEvent, PlayerSetSpawnEvent, ProgressType, ServerHost, Listener,
};

const HOOKS: &[HookId] = &[
    HookId::PlayerSetSpawn,
    HookId::PlayerBedLeave,
    HookId::PlayerAdvancementDone,
    HookId::PlayerAdvancementProgress,
    HookId::GameRuleChanged,
];
static SUBSCRIBED: Subscriptions = Subscriptions::new();

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(PlayerState),
    }
}

struct PlayerState;

/// Whether `AdvancementEarnEvent` follows a completion: only a displayed advancement that just became done is earned.
fn earns_event(display: bool, done_now: bool) -> bool {
    display && done_now
}

/// Posts `event` on `player`'s view, running it as that guest when another player is in place: whether a listener cancelled it.
fn post_for(game: &mut Game<'static>, player: u128, event: &str, fields: Out) -> Option<bool> {
    let posted = if game.lan_acting_uuid == Some(player) {
        game.java_post_player_event(event, false, fields.0)
    } else {
        let conn = game.lan.as_ref()?.hub.guests.iter().find(|g| g.uuid == player)?.conn;
        let posted = game.as_guest(conn, |game| game.java_post_player_event(event, false, fields.0));
        if posted.is_none() {
            log::warn!("Horizon: {event} was not posted: its player is in a turn already");
        }
        posted.flatten()
    };
    posted.map(|(cancelled, _)| cancelled)
}

impl mod_api::Hooks for PlayerState {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED, false)
    }

    fn listeners(&self) -> &'static [Listener] {
        super::java_listener::listeners(HOOKS, &SUBSCRIBED, false)
    }

    fn player_set_spawn(&self, host: &mut dyn ServerHost, event: &mut PlayerSetSpawnEvent<'_>) -> Flow {
        crossed(HookId::PlayerSetSpawn);
        let mut fields = Out::default();
        fields.bool(event.pos.is_some());
        if let Some([x, y, z]) = event.pos {
            fields.text(event.dimension);
            fields.long(BlockPos::new(x, y, z).pack());
            fields.float(0.0);
            fields.float(0.0);
            fields.bool(event.forced);
        }
        let player = event.player;
        let cancelled = with_game(host, |game| post_for(game, player, "PlayerSetSpawnEvent", fields));
        if cancelled.flatten() == Some(true) { Flow::Cancel } else { Flow::Continue }
    }

    fn player_bed_leave(&self, host: &mut dyn ServerHost, event: &mut PlayerBedLeaveEvent<'_>) {
        crossed(HookId::PlayerBedLeave);
        let mut fields = Out::default();
        fields.bool(event.wake_immediately);
        fields.bool(event.update_level);
        let player = event.player;
        with_game(host, |game| post_for(game, player, "PlayerWakeUpEvent", fields));
    }

    fn player_advancement_done(&self, host: &mut dyn ServerHost, event: &mut PlayerAdvancementDoneEvent<'_>) {
        crossed(HookId::PlayerAdvancementDone);
        let (player, id) = (event.player, event.advancement);
        with_game(host, |game| {
            let displayed = game
                .adv
                .tree
                .by_id
                .get(id)
                .is_some_and(|index| game.adv.tree.list[*index].display.is_some());
            if earns_event(displayed, true) {
                let mut fields = Out::default();
                fields.text(id);
                post_for(game, player, "AdvancementEvent.AdvancementEarnEvent", fields);
            }
        });
    }

    fn player_advancement_progress(&self, host: &mut dyn ServerHost, event: &mut PlayerAdvancementProgressEvent<'_>) {
        crossed(HookId::PlayerAdvancementProgress);
        let mut fields = Out::default();
        fields.text(event.advancement);
        fields.text(event.criterion);
        fields.bool(event.progress_type == ProgressType::Revoke);
        let done: Vec<&str> = event
            .progress
            .criteria
            .iter()
            .filter(|c| c.obtained_at.is_some())
            .map(|c| c.name.as_str())
            .collect();
        fields.int(done.len() as i32);
        done.iter().for_each(|name| fields.text(name));
        let player = event.player;
        with_game(host, |game| post_for(game, player, "AdvancementEvent.AdvancementProgressEvent", fields));
    }

    fn game_rule_changed(&self, host: &mut dyn ServerHost, event: &mut GameRuleChangedEvent<'_>) {
        crossed(HookId::GameRuleChanged);
        let (rule, value) = (event.rule, event.value.to_string());
        with_game(host, |game| {
            game.java_post_level_event("GameRuleChangedEvent", false, |out| {
                out.text(rule);
                out.text(&value);
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::earns_event;

    #[test]
    fn only_a_displayed_completion_is_earned() {
        assert!(earns_event(true, true));
        assert!(!earns_event(false, true));
        assert!(!earns_event(true, false));
    }
}
