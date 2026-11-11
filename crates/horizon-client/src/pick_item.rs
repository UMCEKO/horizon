//! Pick Block on the player's inventory: `ServerGamePacketListenerImpl.tryPickItem`
//! with `Inventory.findSlotMatchingItem`, `pickSlot`, `addAndPickItem` and
//! `getSuitableHotbarSlot`. `items` are the 36 main slots (0-8 the hotbar).
use crate::items::ItemStack;

/// `Inventory.findSlotMatchingItem`: the first non-empty slot holding `stack`'s item and components.
pub fn find_slot_matching_item(items: &[Option<ItemStack>], stack: &ItemStack) -> Option<usize> {
    items.iter().position(|s| {
        s.as_ref()
            .is_some_and(|s| s.same_item_same_components(stack))
    })
}

/// `Inventory.getFreeSlot`: the first empty main-inventory slot.
pub fn free_slot_in(items: &[Option<ItemStack>]) -> Option<usize> {
    items.iter().position(Option::is_none)
}

/// `Inventory.pickSlot` after `setSelectedSlot`: the selected slot trades places with `slot`.
pub fn swap_selected_with(items: &mut [Option<ItemStack>], selected: usize, slot: usize) {
    items.swap(selected, slot);
}

/// `Inventory.addAndPickItem` after `setSelectedSlot`: what the selected slot held moves to `free` (or is replaced when there is none), and the slot takes `stack`.
pub fn place_picked(
    items: &mut [Option<ItemStack>],
    selected: usize,
    stack: &ItemStack,
    free: Option<usize>,
) {
    if items[selected].is_some()
        && let Some(free) = free
    {
        items[free] = items[selected].take();
    }
    items[selected] = Some(stack.clone());
}

/// `Inventory.pickSlot` over `(items, selected)`: a main-inventory stack swaps into a suitable hotbar slot.
pub fn pick_slot_body(items: &mut [Option<ItemStack>], selected: &mut usize, slot: usize) {
    *selected = suitable_hotbar_slot(items, *selected);
    swap_selected_with(items, *selected, slot);
}

/// `Inventory.addAndPickItem` over `(items, selected)`; `free_slot` is what `getFreeSlot` answers.
pub fn add_and_pick_item_body(
    items: &mut [Option<ItemStack>],
    selected: &mut usize,
    stack: &ItemStack,
    free_slot: impl FnOnce(&[Option<ItemStack>]) -> Option<usize>,
) {
    *selected = suitable_hotbar_slot(items, *selected);
    let free = free_slot(items);
    place_picked(items, *selected, stack, free);
}

/// `tryPickItem` without a `Game`: selects a matching stack already in the
/// hotbar, or swaps one from the main inventory into a suitable hotbar slot;
/// with infinite materials (creative) it adds the item when none is there.
/// Returns whether anything changed.
pub fn pick(
    items: &mut [Option<ItemStack>],
    selected: &mut usize,
    stack: &ItemStack,
    infinite_materials: bool,
) -> bool {
    match find_slot_matching_item(items, stack) {
        Some(slot) if slot < 9 => *selected = slot,
        Some(slot) => pick_slot_body(items, selected, slot),
        None if infinite_materials => {
            add_and_pick_item_body(items, selected, stack, free_slot_in);
        }
        None => return false,
    }
    true
}

/// `getSuitableHotbarSlot`: from the selected slot on, the first empty one,
/// else the first without enchantments, else the selected one.
pub fn suitable_hotbar_slot(items: &[Option<ItemStack>], selected: usize) -> usize {
    let from = |ok: &dyn Fn(&Option<ItemStack>) -> bool| {
        (0..9).map(|i| (selected + i) % 9).find(|&i| ok(&items[i]))
    };
    from(&|s| s.is_none())
        .or_else(|| from(&|s| s.as_ref().is_some_and(|s| s.enchantments().is_empty())))
        .unwrap_or(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(item: &str, count: u32) -> Option<ItemStack> {
        Some(ItemStack {
            item: format!("minecraft:{item}").into(),
            count,
            damage: 0,
            extra: None,
        })
    }
    fn inventory(filled: &[(usize, &str)]) -> Vec<Option<ItemStack>> {
        let mut items = vec![None; 36];
        for &(i, item) in filled {
            items[i] = stack(item, 5);
        }
        items
    }
    fn stone() -> ItemStack {
        stack("stone", 1).unwrap()
    }

    #[test]
    fn survival_selects_a_matching_hotbar_stack() {
        let mut items = inventory(&[(0, "dirt"), (6, "stone"), (20, "stone")]);
        let mut selected = 2;
        assert!(pick(&mut items, &mut selected, &stone(), false));
        assert_eq!(selected, 6);
        assert_eq!(
            items,
            inventory(&[(0, "dirt"), (6, "stone"), (20, "stone")])
        );
    }

    #[test]
    fn survival_swaps_a_main_inventory_stack_into_the_hotbar() {
        // The first empty hotbar slot from the selected one, wrapping round.
        let mut items = inventory(&[(3, "dirt"), (4, "dirt"), (20, "stone")]);
        let mut selected = 3;
        assert!(pick(&mut items, &mut selected, &stone(), false));
        assert_eq!(selected, 5);
        assert_eq!(items[5], stack("stone", 5));
        assert_eq!(items[20], None);
        // A full hotbar: the selected slot (nothing is enchanted) trades places.
        let mut items: Vec<_> = (0..9).map(|_| stack("dirt", 5)).collect();
        items.extend((9..36).map(|i| if i == 30 { stack("stone", 5) } else { None }));
        let mut selected = 7;
        assert!(pick(&mut items, &mut selected, &stone(), false));
        assert_eq!(selected, 7);
        assert_eq!(
            (items[7].clone(), items[30].clone()),
            (stack("stone", 5), stack("dirt", 5))
        );
    }

    #[test]
    fn survival_without_the_item_does_nothing_and_creative_adds_it() {
        let before = inventory(&[(0, "dirt")]);
        let mut items = before.clone();
        let mut selected = 0;
        assert!(!pick(&mut items, &mut selected, &stone(), false));
        assert_eq!((items, selected), (before.clone(), 0));
        // Creative: the next empty hotbar slot takes one.
        let mut items = before;
        assert!(pick(&mut items, &mut selected, &stone(), true));
        assert_eq!(selected, 1);
        assert_eq!(items[1], stack("stone", 1));
        // A full hotbar: the slot's stack moves to the first free slot.
        let mut items: Vec<_> = (0..9).map(|_| stack("dirt", 5)).collect();
        items.extend(vec![None; 27]);
        let mut selected = 4;
        assert!(pick(&mut items, &mut selected, &stone(), true));
        assert_eq!(
            (selected, items[4].clone(), items[9].clone()),
            (4, stack("stone", 1), stack("dirt", 5))
        );
    }

    #[test]
    fn find_slot_matching_item_skips_empty_and_other_stacks() {
        let items = inventory(&[(2, "dirt"), (7, "stone"), (30, "stone")]);
        assert_eq!(find_slot_matching_item(&items, &stone()), Some(7));
        assert_eq!(find_slot_matching_item(&items[..7], &stone()), None);
    }

    #[test]
    fn pick_slot_body_swaps_into_an_empty_suitable_slot() {
        let mut items = inventory(&[(3, "dirt"), (20, "stone")]);
        let mut selected = 3;
        pick_slot_body(&mut items, &mut selected, 20);
        assert_eq!(selected, 4);
        assert_eq!(
            (items[4].clone(), items[20].clone()),
            (stack("stone", 5), None)
        );
    }

    #[test]
    fn pick_slot_body_trades_places_when_the_hotbar_is_full() {
        let mut items: Vec<_> = (0..9).map(|_| stack("dirt", 5)).collect();
        items.extend((9..36).map(|i| if i == 12 { stack("stone", 5) } else { None }));
        let mut selected = 2;
        pick_slot_body(&mut items, &mut selected, 12);
        assert_eq!(selected, 2);
        assert_eq!(
            (items[2].clone(), items[12].clone()),
            (stack("stone", 5), stack("dirt", 5))
        );
    }

    #[test]
    fn add_and_pick_item_body_takes_an_empty_suitable_slot_untouched() {
        let mut items = inventory(&[(0, "dirt")]);
        let mut selected = 0;
        add_and_pick_item_body(&mut items, &mut selected, &stone(), free_slot_in);
        assert_eq!(selected, 1);
        assert_eq!(
            (items[0].clone(), items[1].clone()),
            (stack("dirt", 5), stack("stone", 1))
        );
    }

    #[test]
    fn add_and_pick_item_body_moves_a_full_slot_to_the_free_slot() {
        let mut items: Vec<_> = (0..9).map(|_| stack("dirt", 5)).collect();
        items.extend(vec![None; 27]);
        let mut selected = 4;
        add_and_pick_item_body(&mut items, &mut selected, &stone(), free_slot_in);
        assert_eq!(selected, 4);
        assert_eq!(
            (items[4].clone(), items[9].clone()),
            (stack("stone", 1), stack("dirt", 5))
        );
    }

    #[test]
    fn add_and_pick_item_body_replaces_the_slot_when_nothing_is_free() {
        let mut items: Vec<_> = (0..36).map(|_| stack("dirt", 5)).collect();
        let mut selected = 4;
        add_and_pick_item_body(&mut items, &mut selected, &stone(), free_slot_in);
        assert_eq!(items[4], stack("stone", 1));
        assert_eq!(items.iter().filter(|s| **s == stack("dirt", 5)).count(), 35);
    }
}
