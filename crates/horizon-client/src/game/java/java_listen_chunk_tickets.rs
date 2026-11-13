// game-module-cfg: feature java-mods
// java-listener-family
//! `ChunkTicketLevelUpdatedEvent` on `chunk_ticket_level_updated`: a chunk's loading level changed, posted to the Java mods with a chunk holder.

use super::java_listener::{Family, listens, with_game};
use mod_api::{ChunkTicketLevel, ChunkTicketLevelUpdatedEvent, HookId, ServerHost};
use std::sync::OnceLock;

const HOOKS: &[HookId] = &[HookId::ChunkTicketLevelUpdated];
static SUBSCRIBED: OnceLock<Vec<Option<HookId>>> = OnceLock::new();

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
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED)
    }

    fn chunk_ticket_level_updated(
        &self,
        host: &mut dyn ServerHost,
        event: &mut ChunkTicketLevelUpdatedEvent,
    ) {
        if !listens(HookId::ChunkTicketLevelUpdated) {
            return;
        }
        let [x, z] = event.chunk.pos();
        let (old, new) = (vanilla_level(event.old), vanilla_level(event.new));
        with_game(host, |game| {
            game.java_post_level_event("ChunkTicketLevelUpdatedEvent", |out| {
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
