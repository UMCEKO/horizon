// game-module-cfg: feature java-mods
// java-listener-family
//! A player's respawn, clone and dimension change (`PlayerEvent.Clone`, `PlayerRespawnEvent`, `EntityTravelToDimensionEvent`, `PlayerChangedDimensionEvent`), posted on the player's view.

use super::Game;
use super::java_listener::{Family, Subscriptions, listens, with_game};
use horizon_client::lan::playerdata::uuid_ints;
use horizon_java_host::wire::Out;
use mod_api::{
    EntityTravelToDimensionEvent, Flow, HookId, Listener, PlayerChangedDimensionEvent,
    PlayerCloneEvent, PlayerRespawnedEvent, ServerHost,
};

const HOOKS: &[HookId] = &[
    HookId::PlayerClone,
    HookId::PlayerRespawned,
    HookId::EntityTravelToDimension,
    HookId::PlayerChangedDimension,
];
static SUBSCRIBED: Subscriptions = Subscriptions::new();

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(Respawns),
    }
}

struct Respawns;

/// `PlayerRespawnEvent` for the guest in place; `end_conquered` is true for the return from the End credits.
pub(super) fn post_respawned(game: &mut Game<'_>, end_conquered: bool) {
    if !listens(HookId::PlayerRespawned) {
        return;
    }
    let mut fields = Out::default();
    fields.bool(end_conquered);
    game.java_post_player_event("PlayerEvent.PlayerRespawnEvent", false, fields.0);
}

/// Runs `f` as the guest `conn`, or at once when that guest is the one in place.
fn as_player<R>(
    game: &mut Game<'static>,
    conn: u32,
    f: impl FnOnce(&mut Game<'static>) -> R,
) -> Option<R> {
    if game.lan_acting == Some(conn) {
        Some(f(game))
    } else {
        game.as_guest(conn, f)
    }
}

impl mod_api::Hooks for Respawns {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED, false)
    }

    fn listeners(&self) -> &'static [Listener] {
        super::java_listener::listeners(HOOKS, &SUBSCRIBED, false)
    }

    fn player_clone(&self, host: &mut dyn ServerHost, event: &mut PlayerCloneEvent<'_>) {
        if !listens(HookId::PlayerClone) {
            return;
        }
        let (uuid, was_death) = (event.player, event.was_death);
        with_game(host, |game| {
            let conn = game
                .lan
                .as_ref()?
                .hub
                .guests
                .iter()
                .find(|g| g.uuid == uuid)?
                .conn;
            // The new view is filled whole: it is a new object, as `new ServerPlayer` is.
            super::java_mods::java_player_view_reset(Some(uuid_ints(uuid)));
            let mut fields = Out::default();
            fields.bool(was_death);
            as_player(game, conn, |game| {
                game.java_post_player_event("PlayerEvent.Clone", false, fields.0)
            })
        });
    }

    fn player_respawned(&self, host: &mut dyn ServerHost, _event: &mut PlayerRespawnedEvent<'_>) {
        with_game(host, |game| post_respawned(game, false));
    }

    fn entity_travel_to_dimension(
        &self,
        host: &mut dyn ServerHost,
        event: &mut EntityTravelToDimensionEvent<'_>,
    ) -> Flow {
        if !listens(HookId::EntityTravelToDimension) {
            return Flow::Continue;
        }
        let (entity, dimension) = (event.entity.id(), event.dimension);
        let posted = with_game(host, |game| {
            let conn = game
                .lan
                .as_ref()?
                .hub
                .guests
                .iter()
                .find(|g| g.entity_id as u32 == entity)?
                .conn;
            let mut fields = Out::default();
            fields.text(dimension);
            as_player(game, conn, |game| {
                game.java_post_player_event("EntityTravelToDimensionEvent", false, fields.0)
            })
        });
        if posted
            .flatten()
            .flatten()
            .is_some_and(|(cancelled, _)| cancelled)
        {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn player_changed_dimension(
        &self,
        host: &mut dyn ServerHost,
        event: &mut PlayerChangedDimensionEvent<'_>,
    ) {
        if !listens(HookId::PlayerChangedDimension) {
            return;
        }
        let (from, to) = (event.from, event.to);
        with_game(host, |game| {
            let mut fields = Out::default();
            fields.text(from);
            fields.text(to);
            game.java_post_player_event("PlayerEvent.PlayerChangedDimensionEvent", false, fields.0)
        });
    }
}
