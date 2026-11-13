// game-module-cfg: feature java-mods
// java-listener-family
//! `ItemAttributeModifierEvent` from `item_attribute_modifier`: a stack's modifiers as Java computes them, asked once per distinct stack.

use super::java_listener::{Family, listens};
use horizon_items::items::ItemStack;
use horizon_java_host::wire::In;
use mod_api::{
    AttributeModifier, AttributeOperation, EquipmentSlotGroup, HookId, Identifier,
    ItemAttributeModifier, ItemAttributeModifierEvent, ItemAttributeModifiers,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

const HOOKS: &[HookId] = &[HookId::ItemAttributeModifier];
static SUBSCRIBED: OnceLock<Vec<Option<HookId>>> = OnceLock::new();
const REMEMBERED: usize = 4096;

thread_local! {
    /// The engine stack the event in flight is about: the API's snapshot of it cannot be saved for Java.
    static CURRENT: RefCell<Option<ItemStack>> = const { RefCell::new(None) };
    /// Java's answer per distinct stack (its saved form, count taken away).
    static ANSWERS: RefCell<HashMap<Vec<u8>, Arc<[ItemAttributeModifier]>>> = RefCell::default();
}

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(ItemAttributes),
    }
}

/// Runs `f`, which posts `item_attribute_modifier` for `stack`, with `stack` at the listener's hand.
pub(in crate::game) fn with_engine_stack<R>(stack: &ItemStack, f: impl FnOnce() -> R) -> R {
    let prior = CURRENT.replace(Some(stack.clone()));
    let out = f();
    CURRENT.set(prior);
    out
}

/// Forgets Java's answers, as its server ends.
pub(super) fn forget() {
    ANSWERS.with_borrow_mut(HashMap::clear);
}

struct ItemAttributes;

impl mod_api::Hooks for ItemAttributes {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED)
    }

    fn item_attribute_modifier(&self, event: &mut ItemAttributeModifierEvent<'_>) {
        // A client's tooltip is Java's own; only the server's equipment asks.
        if !listens(HookId::ItemAttributeModifier) || !super::java_mods::java_server_started() {
            return;
        }
        let Some(java) = CURRENT.with_borrow(|stack| stack.as_ref().and_then(modifiers_of)) else {
            return;
        };
        apply(event.modifiers, &java);
    }
}

/// Java's modifiers for `stack`: cached, else asked.
fn modifiers_of(stack: &ItemStack) -> Option<Arc<[ItemAttributeModifier]>> {
    let mut one = stack.clone();
    one.count = 1;
    let key = super::java_mods::write_tag(horizon_items::item_nbt::item_to_nbt(&one))?;
    if let Some(known) = ANSWERS.with_borrow(|answers| answers.get(&key).cloned()) {
        return Some(known);
    }
    let host = horizon_java_host::get()?;
    // The caller holds borrows of the game: no native Java reaches may find one.
    let reply = horizon_java_host::bridge::without_active(|| {
        host.request("item_attribute_modifiers", &key)
    })?;
    let rows: Arc<[ItemAttributeModifier]> = parse(&reply)?.into();
    ANSWERS.with_borrow_mut(|answers| {
        if answers.len() >= REMEMBERED {
            answers.clear();
        }
        answers.insert(key, rows.clone());
    });
    Some(rows)
}

fn group(name: &str) -> Option<EquipmentSlotGroup> {
    Some(match name {
        "any" => EquipmentSlotGroup::Any,
        "mainhand" => EquipmentSlotGroup::MainHand,
        "offhand" => EquipmentSlotGroup::OffHand,
        "hand" => EquipmentSlotGroup::Hand,
        "feet" => EquipmentSlotGroup::Feet,
        "legs" => EquipmentSlotGroup::Legs,
        "chest" => EquipmentSlotGroup::Chest,
        "head" => EquipmentSlotGroup::Head,
        "armor" => EquipmentSlotGroup::Armor,
        "body" => EquipmentSlotGroup::Body,
        "saddle" => EquipmentSlotGroup::Saddle,
        _ => return None,
    })
}

/// `int m` of (`text attribute`, `text id`, `double amount`, `byte operation`, `text slot group`), as `ItemAttributes.java` writes them.
fn parse(reply: &[u8]) -> Option<Vec<ItemAttributeModifier>> {
    let mut reply = In(reply);
    let count = usize::try_from(reply.int()?).ok()?;
    let mut rows = Vec::with_capacity(count.min(256));
    for _ in 0..count {
        let attribute = Identifier::parse(reply.text()?).ok()?;
        let id = Identifier::parse(reply.text()?).ok()?;
        let amount = reply.double()?;
        let operation = match reply.byte()? {
            0 => AttributeOperation::AddValue,
            1 => AttributeOperation::AddMultipliedBase,
            2 => AttributeOperation::AddMultipliedTotal,
            _ => return None,
        };
        let slot = group(reply.text()?)?;
        rows.push(ItemAttributeModifier::new(
            attribute,
            AttributeModifier::new(id, amount, operation),
            slot,
        ));
    }
    Some(rows)
}

/// Leaves `modifiers` as Java computed them; untouched when they already are.
fn apply(modifiers: &mut ItemAttributeModifiers, java: &[ItemAttributeModifier]) {
    if modifiers.iter().eq(java.iter()) {
        return;
    }
    modifiers.clear();
    for entry in java {
        modifiers.replace(entry.attribute.clone(), entry.modifier.clone(), entry.slot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_java_host::wire::Out;

    fn step() -> ItemAttributeModifier {
        ItemAttributeModifier::new(
            Identifier::parse("minecraft:step_height").unwrap(),
            AttributeModifier::new(
                Identifier::parse("horizon_java_test:step").unwrap(),
                0.5,
                AttributeOperation::AddValue,
            ),
            EquipmentSlotGroup::Feet,
        )
    }

    fn reply(rows: &[(&str, &str, f64, u8, &str)]) -> Vec<u8> {
        let mut out = Out::default();
        out.int(rows.len() as i32);
        for (attribute, id, amount, operation, slot) in rows {
            out.text(attribute);
            out.text(id);
            out.double(*amount);
            out.0.push(*operation);
            out.text(slot);
        }
        out.0
    }

    #[test]
    fn a_reply_reads_as_the_modifiers_java_wrote() {
        let bytes = reply(&[(
            "minecraft:step_height",
            "horizon_java_test:step",
            0.5,
            0,
            "feet",
        )]);
        assert_eq!(parse(&bytes), Some(vec![step()]));
        assert_eq!(parse(&reply(&[])), Some(Vec::new()));
    }

    #[test]
    fn every_operation_and_a_malformed_reply_are_read_or_refused() {
        for (byte, operation) in [
            (1, AttributeOperation::AddMultipliedBase),
            (2, AttributeOperation::AddMultipliedTotal),
        ] {
            let bytes = reply(&[("minecraft:luck", "a:b", 1.0, byte, "any")]);
            assert_eq!(parse(&bytes).unwrap()[0].modifier.operation, operation);
        }
        assert_eq!(parse(&reply(&[("minecraft:luck", "a:b", 1.0, 3, "any")])), None);
        assert_eq!(parse(&reply(&[("minecraft:luck", "a:b", 1.0, 0, "pocket")])), None);
        assert_eq!(parse(&reply(&[("minecraft:luck", "a:b", 1.0, 0, "any")])[..20]), None);
    }

    #[test]
    fn modifiers_java_agrees_with_are_left_unchanged() {
        let mut modifiers = mod_api::__private::game::item_attribute_modifiers(vec![step()]);
        apply(&mut modifiers, &[step()]);
        assert_eq!(mod_api::__private::game::changed_item_attribute_modifiers(modifiers), None);
    }

    #[test]
    fn modifiers_java_adds_or_drops_replace_the_list() {
        let mut modifiers = mod_api::__private::game::item_attribute_modifiers(Vec::new());
        apply(&mut modifiers, &[step()]);
        assert_eq!(
            mod_api::__private::game::changed_item_attribute_modifiers(modifiers),
            Some(vec![step()])
        );
        let mut modifiers = mod_api::__private::game::item_attribute_modifiers(vec![step()]);
        apply(&mut modifiers, &[]);
        assert_eq!(
            mod_api::__private::game::changed_item_attribute_modifiers(modifiers),
            Some(Vec::new())
        );
    }
}
