// game-module-cfg: feature java-mods
// java-listener-family
//! `ChunkTicketLevelUpdatedEvent` on `chunk_ticket_level_updated`: a chunk's loading level changed, posted to the Java mods with a chunk holder.

use super::java_listener::{Family, Subscriptions, crossed, with_game};
use mod_api::{ChunkTicketLevel, ChunkTicketLevelUpdatedEvent, HookId, Listener, ServerHost};

const HOOKS: &[HookId] = &[HookId::ChunkTicketLevelUpdated];
static SUBSCRIBED: Subscriptions = Subscriptions::new();

/// `ChunkLevel.MAX_LEVEL + 1`: the level of a chunk no ticket reaches.
const UNLOADED: i32 = 45;

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(ChunkTickets),
    }
}

struct ChunkTickets;

/// The `ChunkLevel` number Java reads for a level the hook tells.
fn vanilla_level(level: ChunkTicketLevel) -> i32 {
    level.level().map_or(UNLOADED, i32::from)
}

impl mod_api::Hooks for ChunkTickets {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED, false)
    }

    fn listeners(&self) -> &'static [Listener] {
        super::java_listener::listeners(HOOKS, &SUBSCRIBED, false)
    }

    fn chunk_ticket_level_updated(
        &self,
        host: &mut dyn ServerHost,
        event: &mut ChunkTicketLevelUpdatedEvent,
    ) {
        crossed(HookId::ChunkTicketLevelUpdated);
        let [x, z] = event.chunk.pos();
        let (old, new) = (vanilla_level(event.old), vanilla_level(event.new));
        with_game(host, |game| {
            game.java_post_level_event("ChunkTicketLevelUpdatedEvent", false, |out| {
                out.int(x);
                out.int(z);
                out.int(old);
                out.int(new);
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_told_on_chunk_levels_scale() {
        assert_eq!(vanilla_level(ChunkTicketLevel::Loaded(31)), 31);
        assert_eq!(vanilla_level(ChunkTicketLevel::Loaded(33)), 33);
        assert_eq!(vanilla_level(ChunkTicketLevel::Unloaded), 45);
    }
}
