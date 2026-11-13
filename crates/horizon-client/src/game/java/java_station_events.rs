// game-module-cfg: feature java-mods
// java-listener-family
//! The anvil's and grindstone's events (`AnvilUpdateEvent`, `AnvilCraftEvent.Pre/Post`, `GrindstoneEvent.OnPlaceItem/OnTakeItem`), posted on the acting player's view.

use super::Game;
use super::java_listener::{Family, Subscriptions, crossed, listens, with_game};
use super::java_mods::{read_tag, write_tag};
use crate::game::api_values::{TryIntoApi, TryIntoEngine};
use horizon_core::position::BlockPos;
use horizon_java_host::wire::{In, Out};
use mod_api::item::item_stack::ItemStack;
use mod_api::{
    AnvilCraftPostEvent, AnvilCraftPreEvent, AnvilUpdateEvent, Flow, GrindstonePlaceEvent,
    GrindstoneTakeEvent, HookId, HostSide, Listener, ServerHost,
};

const HOOKS: &[HookId] = &[
    HookId::AnvilUpdate,
    HookId::AnvilCraftPre,
    HookId::AnvilCraftPost,
    HookId::GrindstonePlace,
    HookId::GrindstoneTake,
];
static SUBSCRIBED: Subscriptions = Subscriptions::new();

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(Stations),
    }
}

struct Stations;

/// A stack as Java decodes it: its saved form, empty for none.
fn stack_out(game: &Game<'_>, out: &mut Out, stack: Option<&ItemStack>) -> Option<()> {
    let Some(stack) = stack else {
        out.bytes(&[]);
        return Some(());
    };
    let written = stack
        .try_into_engine(game.items())
        .ok()
        .and_then(|engine| write_tag(horizon_client::save::item_to_nbt(&engine)));
    let Some(bytes) = written else {
        log::warn!(
            "Horizon: a station event's stack is not one Java can read; the event is not posted"
        );
        return None;
    };
    out.bytes(&bytes);
    Some(())
}

/// A stack as Java wrote it; `Some(None)` for none.
fn stack_in(reply: &mut In<'_>) -> Option<Option<ItemStack>> {
    let bytes = reply.bytes()?;
    if bytes.is_empty() {
        return Some(None);
    }
    let engine = horizon_client::save::item_from_nbt(&read_tag(bytes)?)?;
    engine.try_into_api().ok().map(Some)
}

#[derive(Debug, PartialEq)]
struct AnvilAnswer {
    output: Option<ItemStack>,
    xp_cost: i32,
    material_cost: i32,
}

fn anvil_answer(result: &[u8]) -> Option<AnvilAnswer> {
    let mut reply = In(result);
    let answer = AnvilAnswer {
        output: stack_in(&mut reply)?,
        xp_cost: reply.int()?,
        material_cost: reply.int()?,
    };
    reply.0.is_empty().then_some(answer)
}

#[derive(Debug, PartialEq)]
struct PlaceAnswer {
    output: Option<ItemStack>,
    xp: i32,
}

fn place_answer(result: &[u8]) -> Option<PlaceAnswer> {
    let mut reply = In(result);
    let answer = PlaceAnswer {
        output: stack_in(&mut reply)?,
        xp: reply.int()?,
    };
    reply.0.is_empty().then_some(answer)
}

#[derive(Debug, PartialEq)]
struct TakeAnswer {
    xp: i32,
    new_top: Option<ItemStack>,
    new_bottom: Option<ItemStack>,
}

fn take_answer(result: &[u8]) -> Option<TakeAnswer> {
    let mut reply = In(result);
    let answer = TakeAnswer {
        xp: reply.int()?,
        new_top: stack_in(&mut reply)?,
        new_bottom: stack_in(&mut reply)?,
    };
    reply.0.is_empty().then_some(answer)
}

/// Posts `event` for `player` with the fields `build` writes: on their view, as that guest when they are one.
fn post(
    host: &mut dyn ServerHost,
    hook: HookId,
    event: &str,
    player: u128,
    build: impl FnOnce(&Game<'static>) -> Option<Out>,
) -> Option<(bool, Vec<u8>)> {
    if !listens(hook) {
        return None;
    }
    crossed(hook);
    with_game(host, |game| {
        let fields = build(game)?;
        let guest = game
            .lan
            .as_ref()
            .and_then(|lan| lan.hub.guests.iter().find(|guest| guest.uuid == player))
            .map(|guest| guest.conn)
            .filter(|_| game.lan_acting_uuid != Some(player));
        match guest {
            Some(conn) => game
                .as_guest(conn, |game| game.java_post_player_event(event, false, fields.0))
                .flatten(),
            None => game.java_post_player_event(event, false, fields.0),
        }
    })
    .flatten()
}

fn pos_long(pos: [i32; 3]) -> i64 {
    BlockPos::new(pos[0], pos[1], pos[2]).pack()
}

fn flow(cancelled: bool) -> Flow {
    if cancelled {
        Flow::Cancel
    } else {
        Flow::Continue
    }
}

impl mod_api::Hooks for Stations {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED, false)
    }

    fn listeners(&self) -> &'static [Listener] {
        super::java_listener::listeners(HOOKS, &SUBSCRIBED, false)
    }

    fn anvil_update(&self, host: HostSide<'_>, event: &mut AnvilUpdateEvent<'_>) -> Flow {
        // A client's menu is predicted: the server's posting is what the player is told.
        let HostSide::Server(host) = host else {
            return Flow::Continue;
        };
        let update = &*event.update;
        let posted = post(
            host,
            HookId::AnvilUpdate,
            "AnvilUpdateEvent",
            event.player,
            |game| {
                let mut out = Out::default();
                stack_out(game, &mut out, Some(&update.left))?;
                stack_out(game, &mut out, update.right.as_ref())?;
                out.bool(update.name.is_some());
                out.text(update.name.as_deref().unwrap_or(""));
                stack_out(game, &mut out, update.vanilla.output.as_ref())?;
                out.int(update.vanilla.xp_cost);
                out.int(update.vanilla.material_cost);
                stack_out(game, &mut out, update.output.as_ref())?;
                out.int(update.xp_cost);
                out.int(update.material_cost);
                Some(out)
            },
        );
        let Some((cancelled, result)) = posted else {
            return Flow::Continue;
        };
        if let Some(answer) = anvil_answer(&result) {
            event.update.output = answer.output;
            event.update.xp_cost = answer.xp_cost;
            event.update.material_cost = answer.material_cost;
        } else {
            log::warn!("Horizon: Java's answer to AnvilUpdateEvent came in pieces");
        }
        flow(cancelled)
    }

    fn anvil_craft_pre(
        &self,
        host: &mut dyn ServerHost,
        event: &mut AnvilCraftPreEvent<'_>,
    ) -> Flow {
        let posted = post_craft(
            host,
            HookId::AnvilCraftPre,
            "AnvilCraftEvent.Pre",
            event.player,
            event.pos,
            event.craft,
        );
        flow(posted.is_some_and(|(cancelled, _)| cancelled))
    }

    fn anvil_craft_post(&self, host: &mut dyn ServerHost, event: &mut AnvilCraftPostEvent<'_>) {
        post_craft(
            host,
            HookId::AnvilCraftPost,
            "AnvilCraftEvent.Post",
            event.player,
            event.pos,
            event.craft,
        );
    }

    fn grindstone_place(
        &self,
        host: &mut dyn ServerHost,
        event: &mut GrindstonePlaceEvent<'_>,
    ) -> Flow {
        let place = &*event.place;
        let posted = post(
            host,
            HookId::GrindstonePlace,
            "GrindstoneEvent.OnPlaceItem",
            event.player,
            |game| {
                let mut out = Out::default();
                stack_out(game, &mut out, place.top.as_ref())?;
                stack_out(game, &mut out, place.bottom.as_ref())?;
                out.int(place.xp);
                Some(out)
            },
        );
        let Some((cancelled, result)) = posted else {
            return Flow::Continue;
        };
        if let Some(answer) = place_answer(&result) {
            event.place.output = answer.output;
            event.place.xp = answer.xp;
        } else {
            log::warn!("Horizon: Java's answer to GrindstoneEvent.OnPlaceItem came in pieces");
        }
        flow(cancelled)
    }

    fn grindstone_take(
        &self,
        host: &mut dyn ServerHost,
        event: &mut GrindstoneTakeEvent<'_>,
    ) -> Flow {
        let take = &*event.take;
        let pos = pos_long(event.pos);
        let posted = post(
            host,
            HookId::GrindstoneTake,
            "GrindstoneEvent.OnTakeItem",
            event.player,
            |game| {
                let mut out = Out::default();
                out.long(pos);
                stack_out(game, &mut out, take.top.as_ref())?;
                stack_out(game, &mut out, take.bottom.as_ref())?;
                out.int(take.xp);
                stack_out(game, &mut out, take.new_top.as_ref())?;
                stack_out(game, &mut out, take.new_bottom.as_ref())?;
                Some(out)
            },
        );
        let Some((cancelled, result)) = posted else {
            return Flow::Continue;
        };
        if let Some(answer) = take_answer(&result) {
            event.take.xp = answer.xp;
            event.take.new_top = answer.new_top;
            event.take.new_bottom = answer.new_bottom;
        } else {
            log::warn!("Horizon: Java's answer to GrindstoneEvent.OnTakeItem came in pieces");
        }
        flow(cancelled)
    }
}

/// An anvil craft's event (`Pre` or `Post`) with the inputs and the result as they were before.
fn post_craft(
    host: &mut dyn ServerHost,
    hook: HookId,
    name: &str,
    player: u128,
    pos: [i32; 3],
    craft: &mod_api::item::menu::AnvilCraft,
) -> Option<(bool, Vec<u8>)> {
    post(host, hook, name, player, |game| {
        let mut out = Out::default();
        out.long(pos_long(pos));
        stack_out(game, &mut out, craft.left.as_ref())?;
        stack_out(game, &mut out, craft.right.as_ref())?;
        stack_out(game, &mut out, Some(&craft.output))?;
        Some(out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(build: impl FnOnce(&mut Out)) -> Vec<u8> {
        let mut out = Out::default();
        build(&mut out);
        out.0
    }

    #[test]
    fn an_anvil_answer_with_no_output_keeps_its_costs() {
        let bytes = written(|out| {
            out.bytes(&[]);
            out.int(7);
            out.int(2);
        });
        assert_eq!(
            anvil_answer(&bytes),
            Some(AnvilAnswer {
                output: None,
                xp_cost: 7,
                material_cost: 2
            })
        );
    }

    #[test]
    fn the_costs_keep_their_sign() {
        let bytes = written(|out| {
            out.bytes(&[]);
            out.int(-3);
            out.int(0);
        });
        assert_eq!(anvil_answer(&bytes).map(|a| a.xp_cost), Some(-3));
    }

    #[test]
    fn a_short_or_long_answer_is_refused() {
        let bytes = written(|out| {
            out.bytes(&[]);
            out.int(7);
            out.int(2);
        });
        assert_eq!(anvil_answer(&bytes[..bytes.len() - 1]), None);
        let mut long = bytes;
        long.push(0);
        assert_eq!(anvil_answer(&long), None);
        assert_eq!(place_answer(&[]), None);
        assert_eq!(take_answer(&[0, 0, 0, 1]), None);
    }

    #[test]
    fn a_grindstone_take_answers_its_experience_and_what_the_inputs_hold() {
        let bytes = written(|out| {
            out.int(-1);
            out.bytes(&[]);
            out.bytes(&[]);
        });
        assert_eq!(
            take_answer(&bytes),
            Some(TakeAnswer {
                xp: -1,
                new_top: None,
                new_bottom: None
            })
        );
    }

    #[test]
    fn a_place_answer_without_output_leaves_vanilla_s_experience() {
        let bytes = written(|out| {
            out.bytes(&[]);
            out.int(-1);
        });
        assert_eq!(
            place_answer(&bytes),
            Some(PlaceAnswer {
                output: None,
                xp: -1
            })
        );
    }

    #[test]
    fn a_cancel_is_the_flow_the_hook_stops_on() {
        assert!(matches!(flow(true), Flow::Cancel));
        assert!(matches!(flow(false), Flow::Continue));
    }
}
