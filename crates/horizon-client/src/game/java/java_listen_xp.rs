// game-module-cfg: feature java-mods
// java-listener-family
//! A player's experience (`PlayerXpEvent.PickupXp`, `XpChange`, `LevelChange`), posted on the acting player's view.

use super::java_listener::{Family, listens, with_game};
use horizon_java_host::wire::{In, Out};
use mod_api::{
    Flow, HookId, PickupXpEvent, PlayerXpChangeEvent, PlayerXpLevelChangeEvent, ServerHost,
};
use std::sync::OnceLock;

const HOOKS: &[HookId] = &[
    HookId::PickupXp,
    HookId::PlayerXpChange,
    HookId::PlayerXpLevelChange,
];
static SUBSCRIBED: OnceLock<Vec<Option<HookId>>> = OnceLock::new();

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(PlayerExperience),
    }
}

struct PlayerExperience;

/// Posts `event` with `fields`: whether a listener cancelled it, and the `int` its site reads back when it has one.
fn post(
    host: &mut dyn ServerHost,
    hook: HookId,
    event: &str,
    fields: Out,
) -> Option<(bool, Option<i32>)> {
    if !listens(hook) {
        return None;
    }
    let (canceled, result) = with_game(host, |game| game.java_post_player_event(event, fields.0))??;
    Some((canceled, In(&result).int()))
}

/// Posts a signed `value` and applies what Java answers: the new value, or a cancel.
fn change(host: &mut dyn ServerHost, hook: HookId, event: &str, value: &mut i32) -> Flow {
    let mut fields = Out::default();
    fields.int(*value);
    match post(host, hook, event, fields) {
        Some((true, _)) => Flow::Cancel,
        Some((false, Some(answer))) => {
            *value = answer;
            Flow::Continue
        }
        _ => Flow::Continue,
    }
}

impl mod_api::Hooks for PlayerExperience {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED)
    }

    fn pickup_xp(&self, host: &mut dyn ServerHost, event: &mut PickupXpEvent<'_>) -> Flow {
        let mut fields = Out::default();
        fields.int(event.orb as i32);
        match post(host, HookId::PickupXp, "PlayerXpEvent.PickupXp", fields) {
            Some((true, _)) => Flow::Cancel,
            _ => Flow::Continue,
        }
    }

    fn player_xp_change(
        &self,
        host: &mut dyn ServerHost,
        event: &mut PlayerXpChangeEvent<'_>,
    ) -> Flow {
        change(
            host,
            HookId::PlayerXpChange,
            "PlayerXpEvent.XpChange",
            &mut event.amount,
        )
    }

    fn player_xp_level_change(
        &self,
        host: &mut dyn ServerHost,
        event: &mut PlayerXpLevelChangeEvent<'_>,
    ) -> Flow {
        change(
            host,
            HookId::PlayerXpLevelChange,
            "PlayerXpEvent.LevelChange",
            &mut event.levels,
        )
    }
}
