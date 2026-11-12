// game-module-cfg: feature java-mods
// java-listener-family
//! `ItemTossEvent` and `ArrowLooseEvent`: Java builds the tossed item and decides the shot's charge, where native's sites ask.

use super::java_listener::{Family, listens};
use super::{Game, Hand};
use horizon_client::items::ItemStack;
use horizon_java_host::wire::Out;
use mod_api::HookId;

const HOOKS: &[HookId] = &[HookId::ItemToss, HookId::ArrowLoose];

/// The census only reads the sites; the sites post through `Game`, as they need more than the hooks' events carry.
struct Sites;

impl mod_api::Hooks for Sites {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        &[]
    }
}

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(Sites),
    }
}

impl Game<'_> {
    /// `Player.drop` through `ItemTossEvent`: `None` when no mod listens, `Some(None)` when the toss is gone, `Some(Some(id))` when Java spawned the item.
    pub(super) fn java_item_toss(
        &mut self,
        stack: &ItemStack,
        at: [f64; 3],
        motion: [f64; 3],
        delay: i32,
        named: bool,
    ) -> Option<Option<u32>> {
        if !listens(HookId::ItemToss) {
            return None;
        }
        let tag = super::java_mods::write_tag(horizon_items::item_nbt::item_to_nbt(stack))?;
        let mut fields = Out::default();
        fields.bytes(&tag);
        at.into_iter().chain(motion).for_each(|v| fields.double(v));
        fields.int(delay);
        fields.bool(named);
        let (canceled, result) = self.java_post_player_event("ItemTossEvent", fields.0)?;
        if canceled {
            return Some(None);
        }
        let id = i32::from_be_bytes(result.get(..4)?.try_into().ok()?);
        Some(u32::try_from(id).ok().filter(|id| *id != 0))
    }

    /// `ArrowLooseEvent` through `EventHooks.onArrowLoose`: the charge the listeners left; negative cancels the shot.
    pub(super) fn java_arrow_loose(
        &mut self,
        hand: Hand,
        charge: i32,
        has_ammo: bool,
        crossbow: bool,
    ) -> i32 {
        if !listens(HookId::ArrowLoose) {
            return charge;
        }
        let mut fields = Out::default();
        fields.int(i32::from(matches!(hand, Hand::Off)));
        fields.int(charge);
        fields.bool(has_ammo);
        fields.bool(crossbow);
        self.java_post_player_event("ArrowLooseEvent", fields.0)
            .and_then(|(_, result)| Some(i32::from_be_bytes(result.get(..4)?.try_into().ok()?)))
            .unwrap_or(charge)
    }
}
