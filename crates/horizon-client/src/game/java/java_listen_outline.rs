// game-module-cfg: feature java-mods
// java-listener-family
//! `ExtractBlockOutlineRenderStateEvent` on `block_highlight`: the Java world recording ran the mods' listeners; their cancel hides the outline by the hook's contract.

use super::java_listener::Family;
use horizon_client::world_renders::{OutlineAnswer, OutlineKind};
use mod_api::{BlockHighlightEvent, ClientHost, Flow, HookId};
use std::sync::OnceLock;

const HOOKS: &[HookId] = &[HookId::BlockHighlight];
static SUBSCRIBED: OnceLock<Vec<Option<HookId>>> = OnceLock::new();

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(Outline),
    }
}

struct Outline;

impl mod_api::Hooks for Outline {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED)
    }

    // The recording posts the event on the Java client's thread as it extracts the frame; posting it again here would run its listeners twice and wait on the JVM every frame.
    fn block_highlight(&self, _: &dyn ClientHost, event: &mut BlockHighlightEvent) -> Flow {
        verdict(horizon_client::world_renders::outline(), event.pos)
    }
}

/// The recorded answer's verdict for the block native outlines: only a cancel taken for that very block cancels.
fn verdict(answer: Option<OutlineAnswer>, pos: [i32; 3]) -> Flow {
    match answer {
        Some(OutlineAnswer {
            kind: OutlineKind::Canceled,
            block: Some((at, _, _)),
            ..
        }) if at == pos => Flow::Cancel,
        _ => Flow::Continue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(kind: OutlineKind, at: [i32; 3]) -> Option<OutlineAnswer> {
        Some(OutlineAnswer {
            kind,
            block: Some((at, 1, 7)),
            render: true,
            high_contrast: false,
        })
    }

    #[test]
    fn a_recorded_cancel_cancels_only_its_own_block() {
        assert_eq!(
            verdict(answer(OutlineKind::Canceled, [1, 2, 3]), [1, 2, 3]),
            Flow::Cancel
        );
        assert_eq!(
            verdict(answer(OutlineKind::Canceled, [1, 2, 3]), [1, 2, 4]),
            Flow::Continue
        );
        assert_eq!(
            verdict(answer(OutlineKind::Vanilla, [1, 2, 3]), [1, 2, 3]),
            Flow::Continue
        );
        assert_eq!(verdict(None, [1, 2, 3]), Flow::Continue);
    }
}
