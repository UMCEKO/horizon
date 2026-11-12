// game-module-cfg: feature java-mods
// java-listener-family
//! `UseItemOnBlockEvent` from the native click phases: Java mods cancel a phase native runs.

use super::java_listener::{Family, listens, with_game};
use horizon_core::position::BlockPos;
use horizon_java_host::wire::{In, Out};
use mod_api::{Flow, Hand, HookId, HostSide, InteractionResult, UseItemOnBlockEvent, UsePhase};
use std::sync::OnceLock;

const HOOKS: &[HookId] = &[HookId::UseItemOnBlock];
static SUBSCRIBED: OnceLock<Vec<Option<HookId>>> = OnceLock::new();

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(UseOnBlock),
    }
}

struct UseOnBlock;

/// `Direction.get3DDataValue` of a face vector.
fn face_value(face: [i32; 3]) -> Option<u8> {
    match face {
        [0, -1, 0] => Some(0),
        [0, 1, 0] => Some(1),
        [0, 0, -1] => Some(2),
        [0, 0, 1] => Some(3),
        [-1, 0, 0] => Some(4),
        [1, 0, 0] => Some(5),
        _ => None,
    }
}

fn phase_value(phase: UsePhase) -> Option<u8> {
    match phase {
        UsePhase::ItemBeforeBlock => Some(0),
        UsePhase::Block => Some(1),
        UsePhase::ItemAfterBlock => Some(2),
        _ => None,
    }
}

/// `EventPosts.useItemOnBlock`'s result: `byte kind`, `byte swing`.
fn read_result(result: &[u8]) -> Option<InteractionResult> {
    let mut result = In(result);
    let kind = result.byte()?;
    let swing = result.byte()?;
    Some(match (kind, swing) {
        (0, _) => InteractionResult::Pass,
        (1, 1) => InteractionResult::Success,
        (1, 2) => InteractionResult::SuccessServer,
        (1 | 2, _) => InteractionResult::Consume,
        (3, _) => InteractionResult::Fail,
        (4, _) => InteractionResult::TryWithEmptyHand,
        _ => return None,
    })
}

impl mod_api::Hooks for UseOnBlock {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED)
    }

    fn use_item_on_block(&self, host: HostSide<'_>, event: &mut UseItemOnBlockEvent<'_>) -> Flow {
        let HostSide::Server(host) = host else {
            return Flow::Continue;
        };
        let (Some(phase), Some(face)) =
            (phase_value(event.phase), face_value(event.click.hit.face))
        else {
            return Flow::Continue;
        };
        if !listens(HookId::UseItemOnBlock) {
            return Flow::Continue;
        }
        let click = &event.click;
        let mut fields = Out::default();
        fields.0.push(phase);
        fields.int(i32::from(click.hand == Hand::Off));
        fields.long(BlockPos::new(click.pos[0], click.pos[1], click.pos[2]).pack());
        fields.0.push(face);
        click.hit.point.iter().for_each(|v| fields.double(*v));
        fields.bool(click.hit.inside);
        let (pos, phase_kind) = (click.pos, event.phase);
        let posted = with_game(host, |game| {
            // Java already posts the phases of a click or a use it runs whole.
            let state = game.world.block(pos[0], pos[1], pos[2]);
            let java_block = horizon_java_host::bridge::java_block(state);
            let java_use_on = phase_kind == UsePhase::ItemAfterBlock
                && horizon_java_host::get().is_some_and(|host| {
                    game.held()
                        .is_some_and(|stack| host.item_code_runs(&stack.item, true))
                });
            if java_block || java_use_on {
                return None;
            }
            game.java_post_player_event("UseItemOnBlockEvent", fields.0)
        });
        let Some((true, result)) = posted.flatten() else {
            return Flow::Continue;
        };
        if let Some(result) = read_result(&result) {
            *event.cancellation_result = result;
        }
        Flow::Cancel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_results_map_to_the_api_ones() {
        assert_eq!(read_result(&[1, 1]), Some(InteractionResult::Success));
        assert_eq!(read_result(&[1, 2]), Some(InteractionResult::SuccessServer));
        assert_eq!(read_result(&[2, 0]), Some(InteractionResult::Consume));
        assert_eq!(read_result(&[3, 0]), Some(InteractionResult::Fail));
        assert_eq!(
            read_result(&[4, 0]),
            Some(InteractionResult::TryWithEmptyHand)
        );
        assert_eq!(read_result(&[0, 0]), Some(InteractionResult::Pass));
        assert_eq!(read_result(&[9, 0]), None);
    }

    #[test]
    fn faces_follow_direction_data_values() {
        assert_eq!(face_value([0, 1, 0]), Some(1));
        assert_eq!(face_value([1, 0, 0]), Some(5));
        assert_eq!(face_value([1, 1, 0]), None);
    }
}
