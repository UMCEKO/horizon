// game-module-cfg: feature java-mods
// java-listener-family
//! `EnchantmentLevelSetEvent`, `PlayerEnchantItemEvent` and `ItemSmeltedEvent` from their hooks, posted on the acting player's view.

use super::api_values::TryIntoEngine;
use super::java_listener::{Family, listens, with_game};
use super::java_mods::write_tag;
use super::Game;
use horizon_core::position::BlockPos;
use horizon_java_host::wire::{In, Out};
use mod_api::{
    EnchantmentLevelSetEvent, HookId, ItemSmeltedEvent, PlayerEnchantItemEvent, ServerHost,
};
use std::sync::OnceLock;

const HOOKS: &[HookId] = &[
    HookId::EnchantmentLevelSet,
    HookId::PlayerEnchantItem,
    HookId::ItemSmelted,
];
static SUBSCRIBED: OnceLock<Vec<Option<HookId>>> = OnceLock::new();

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(EnchantEvents),
    }
}

struct EnchantEvents;

/// A stack as Java decodes it: its saved form, length first.
fn stack_bytes(game: &Game<'_>, stack: &mod_api::ItemStack) -> Option<Vec<u8>> {
    let stack = stack.try_into_engine(game.items()).ok()?;
    write_tag(horizon_client::save::item_to_nbt(&stack))
}

/// Runs `f` as the player the event is about: where a hook fires outside the player's turn, in one.
fn as_player<R>(
    game: &mut Game<'static>,
    player: u128,
    entity: i32,
    f: impl FnOnce(&mut Game<'static>) -> R,
) -> Option<R> {
    if game.lan_acting_uuid == Some(player) {
        return Some(f(game));
    }
    let host = game.lan.as_ref()?;
    let index = host.hub.guests.identity_index(player, entity as u32)?;
    let conn = host.hub.guests[index].conn;
    game.as_guest(conn, f)
}

/// `EnchantmentLevelSetEvent`'s fields after the view: the table, the row, the power, the item, the drawn level and the current one.
fn level_fields(pos: [i32; 3], row: i32, power: i32, item: &[u8], original: i32, level: i32) -> Out {
    let mut out = Out::default();
    out.long(BlockPos::new(pos[0], pos[1], pos[2]).pack());
    out.int(row);
    out.int(power);
    out.bytes(item);
    out.int(original);
    out.int(level);
    out
}

/// The level a listener left, from the post's result.
fn level_reply(result: &[u8]) -> Option<i32> {
    In(result).int()
}

/// `PlayerEnchantItemEvent`'s fields after the view: the item, then each enchantment's id and level.
fn enchant_fields(item: &[u8], enchantments: &[(std::sync::Arc<str>, u32)]) -> Out {
    let mut out = Out::default();
    out.bytes(item);
    out.int(enchantments.len() as i32);
    for (id, level) in enchantments {
        out.text(id);
        out.int(*level as i32);
    }
    out
}

/// `ItemSmeltedEvent`'s fields after the view: the taken stack (absent once the slot is empty) and the count.
fn smelted_fields(stack: Option<&[u8]>, amount: u32) -> Out {
    let mut out = Out::default();
    out.bool(stack.is_some());
    if let Some(stack) = stack {
        out.bytes(stack);
    }
    out.int(amount as i32);
    out
}

impl mod_api::Hooks for EnchantEvents {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED)
    }

    fn enchantment_level_set(
        &self,
        host: &mut dyn ServerHost,
        event: &mut EnchantmentLevelSetEvent<'_>,
    ) {
        if !listens(HookId::EnchantmentLevelSet) {
            return;
        }
        let offer = event.offer;
        let posted = with_game(host, |game| {
            as_player(game, offer.player, offer.player_entity_id, |game| {
                let item = stack_bytes(game, offer.item)?;
                let fields = level_fields(
                    event.pos,
                    offer.row,
                    offer.power,
                    &item,
                    offer.original_level,
                    event.level,
                );
                game.java_post_player_event("EnchantmentLevelSetEvent", fields.0)
            })
        });
        if let Some((_, result)) = posted.flatten().flatten()
            && let Some(level) = level_reply(&result)
        {
            event.level = level;
        }
    }

    fn player_enchant_item(
        &self,
        host: &mut dyn ServerHost,
        event: &mut PlayerEnchantItemEvent<'_>,
    ) {
        if !listens(HookId::PlayerEnchantItem) {
            return;
        }
        with_game(host, |game| {
            as_player(game, event.player, event.player_entity_id, |game| {
                let item = stack_bytes(game, event.item)?;
                let fields = enchant_fields(&item, event.enchantments);
                game.java_post_player_event("PlayerEnchantItemEvent", fields.0)
            })
        });
    }

    fn item_smelted(&self, host: &mut dyn ServerHost, event: &mut ItemSmeltedEvent<'_>) {
        if !listens(HookId::ItemSmelted) {
            return;
        }
        with_game(host, |game| {
            as_player(game, event.player, event.player_entity_id, |game| {
                let stack = match event.stack {
                    Some(stack) => Some(stack_bytes(game, stack)?),
                    None => None,
                };
                let fields = smelted_fields(stack.as_deref(), event.amount_removed);
                game.java_post_player_event("PlayerEvent.ItemSmeltedEvent", fields.0)
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_level_a_listener_left_is_the_one_read_back() {
        let mut out = Out::default();
        out.int(30);
        assert_eq!(level_reply(&out.0), Some(30));
        assert_eq!(level_reply(&out.0[..3]), None);
        assert_eq!(level_reply(&[]), None);
    }

    #[test]
    fn a_level_event_writes_its_fields_in_the_order_java_reads_them() {
        let item = [7u8, 8, 9];
        let out = level_fields([1, 64, -2], 2, 15, &item, 11, 12);
        let mut read = In(&out.0);
        assert_eq!(read.long(), Some(BlockPos::new(1, 64, -2).pack()));
        assert_eq!(read.int(), Some(2));
        assert_eq!(read.int(), Some(15));
        assert_eq!(read.bytes(), Some(&item[..]));
        assert_eq!(read.int(), Some(11));
        assert_eq!(read.int(), Some(12));
        assert!(read.0.is_empty());
    }

    #[test]
    fn an_enchant_event_lists_each_enchantment_with_its_level() {
        let item = [7u8, 8, 9];
        let given: [(std::sync::Arc<str>, u32); 2] = [("minecraft:sharpness".into(), 3), ("minecraft:unbreaking".into(), 2)];
        let out = enchant_fields(&item, &given);
        let mut read = In(&out.0);
        assert_eq!(read.bytes(), Some(&item[..]));
        assert_eq!(read.int(), Some(2));
        assert_eq!(read.text(), Some("minecraft:sharpness"));
        assert_eq!(read.int(), Some(3));
        assert_eq!(read.text(), Some("minecraft:unbreaking"));
        assert_eq!(read.int(), Some(2));
        assert!(read.0.is_empty());
    }

    #[test]
    fn a_smelted_event_with_an_empty_slot_carries_no_stack() {
        let out = smelted_fields(None, 0);
        let mut read = In(&out.0);
        assert_eq!(read.bool(), Some(false));
        assert_eq!(read.int(), Some(0));
        assert!(read.0.is_empty());
        let out = smelted_fields(Some(&[1, 2]), 3);
        let mut read = In(&out.0);
        assert_eq!(read.bool(), Some(true));
        assert_eq!(read.bytes(), Some(&[1u8, 2][..]));
        assert_eq!(read.int(), Some(3));
    }
}
