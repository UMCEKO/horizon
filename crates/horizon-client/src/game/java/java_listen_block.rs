// game-module-cfg: feature java-mods
// java-listener-family
//! `BonemealEvent` from `bonemeal`, and the site of `BlockEvent.FarmlandTrampleEvent`, which `farmland_trample` cannot post from inside a landing.

use super::Game;
use super::java_hooks::TrampleBy;
use super::java_listener::{Family, Subscriptions, crossed, listens, with_game};
use super::java_mods::write_tag;
use horizon_core::{nbt::Tag, position::BlockPos};
use mod_api::{BonemealEvent, BonemealSource, Flow, HookId, HostSide, Listener};

const HOOKS: &[HookId] = &[HookId::FarmlandTrample, HookId::Bonemeal];
static SUBSCRIBED: Subscriptions = Subscriptions::new();

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(Blocks),
    }
}

struct Blocks;

impl mod_api::Hooks for Blocks {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED, false)
    }

    fn listeners(&self) -> &'static [Listener] {
        super::java_listener::listeners(HOOKS, &SUBSCRIBED, false)
    }

    fn bonemeal(&self, host: HostSide<'_>, event: &mut BonemealEvent<'_>) -> Flow {
        let HostSide::Server(host) = host else {
            return Flow::Continue;
        };
        crossed(HookId::Bonemeal);
        if event.source != BonemealSource::Interaction {
            return Flow::Continue;
        }
        let pos = event.pos.pack();
        let by_player = event.player.is_some();
        let stack = if by_player {
            Vec::new()
        } else {
            event
                .stack
                .as_ref()
                .and_then(|stack| stack.to_nbt(&*host))
                .and_then(|tag| write_tag(Tag::Compound(tag)))
                .unwrap_or_default()
        };
        let posted = with_game(host, |game| {
            if by_player {
                let mut fields = horizon_java_host::wire::Out::default();
                fields.long(pos);
                game.java_post_player_event("BonemealEvent", false, fields.0)
            } else {
                game.java_post_level_event("BonemealEvent", false, |out| {
                    out.long(pos);
                    out.bytes(&stack);
                })
            }
        });
        let Some((true, result)) = posted.flatten() else {
            return Flow::Continue;
        };
        event.successful = result.first() == Some(&1);
        Flow::Cancel
    }
}

const TRAMPLE: &str = "BlockEvent.FarmlandTrampleEvent";

impl Game<'_> {
    /// Posts the trample a landing recorded: true when a listener canceled it.
    pub(super) fn java_trample(&mut self, pos: [i32; 3], distance: f64, who: TrampleBy) -> bool {
        if !listens(HookId::FarmlandTrample) {
            return false;
        }
        let pos = BlockPos::new(pos[0], pos[1], pos[2]).pack();
        let posted = match who {
            TrampleBy::Player => {
                let mut fields = horizon_java_host::wire::Out::default();
                fields.long(pos);
                fields.double(distance);
                self.java_post_player_event(TRAMPLE, false, fields.0)
            }
            TrampleBy::Mob(mob) => self.java_post_level_event(TRAMPLE, false, |out| {
                out.long(pos);
                out.double(distance);
                out.int(mob as i32);
            }),
        };
        posted.is_some_and(|(canceled, _)| canceled)
    }
}
