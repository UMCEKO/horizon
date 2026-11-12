// game-module-cfg: feature java-mods
//! What the loaded mods change in crafting, loot and stations, asked of Java once per process (`CraftFacts`).

// The seams that ask these facts land in later W11 specs.
#![allow(dead_code)]

use horizon_java_host::wire::In;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

/// A vanilla station's recipe type; the discriminant is its bit in `recipeTypeBits`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::game) enum Station {
    Crafting = 0,
    Smithing = 1,
    Smelting = 2,
    Blasting = 3,
    Smoking = 4,
    CampfireCooking = 5,
    Stonecutting = 6,
}

/// An event mods may hear; the discriminant is its bit in `eventBits`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::game) enum Event {
    ItemCrafted = 0,
    ItemSmelted = 1,
    ItemSmithed = 2,
    AnvilUpdate = 3,
    AnvilCraftPre = 4,
    AnvilCraftPost = 5,
    GrindstoneOnPlaceItem = 6,
    GrindstoneOnTakeItem = 7,
    EnchantmentLevelSet = 8,
    PlayerEnchantItem = 9,
    GetEnchantmentLevel = 10,
}

/// A method an item's own class declares; the discriminant is its bit in an item's bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::game) enum ItemMethod {
    OnCraftedBy = 0,
    GetCraftingRemainder = 1,
    GetEnchantmentLevel = 2,
    GetAllEnchantments = 3,
    SupportsEnchantment = 4,
    IsPrimaryItemFor = 5,
    IsCombineRepairable = 6,
    GetXpRepairRatio = 7,
    CanGrindstoneRepair = 8,
    GetMaxDamage = 9,
    IsDamageable = 10,
    ApplyEnchantments = 11,
}

/// A loot modifier: the tables it can apply to, `None` for any.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::game) struct Modifier {
    pub id: String,
    pub tables: Option<HashSet<String>>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::game) struct CraftFacts {
    recipe_types: u32,
    events: u32,
    items: HashMap<Arc<str>, u32>,
    modifiers: Vec<Modifier>,
    functions: HashSet<String>,
    conditions: HashSet<String>,
}

static FACTS: OnceLock<CraftFacts> = OnceLock::new();

fn names(input: &mut In) -> Option<HashSet<String>> {
    let mut set = HashSet::new();
    for _ in 0..input.int()? {
        set.insert(input.text()?.to_string());
    }
    Some(set)
}

fn parse(bytes: &[u8]) -> Option<CraftFacts> {
    let mut input = In(bytes);
    let recipe_types = input.int()? as u32;
    let events = input.int()? as u32;
    let mut items = HashMap::new();
    for _ in 0..input.int()? {
        let id = input.text()?;
        items.insert(Arc::from(id), input.int()? as u32);
    }
    let mut modifiers = Vec::new();
    for _ in 0..input.int()? {
        let id = input.text()?.to_string();
        let any = input.bool()?;
        let tables = names(&mut input)?;
        modifiers.push(Modifier {
            id,
            tables: (!any).then_some(tables),
        });
    }
    let functions = names(&mut input)?;
    let conditions = names(&mut input)?;
    Some(CraftFacts {
        recipe_types,
        events,
        items,
        modifiers,
        functions,
        conditions,
    })
}

/// Asks Java once per process; a second server start keeps the first answer.
pub(in crate::game) fn load() {
    if FACTS.get().is_some() {
        return;
    }
    let Some(host) = horizon_java_host::get() else {
        return;
    };
    let Some(bytes) = host.request("craft_facts", &[]) else {
        log::error!("Horizon: Java answered no craft facts");
        return;
    };
    match parse(&bytes) {
        Some(facts) => {
            let _ = FACTS.set(facts);
        }
        None => log::error!("Horizon: the craft facts Java answered are malformed"),
    }
}

pub(in crate::game) fn craft_facts() -> Option<&'static CraftFacts> {
    FACTS.get()
}

impl CraftFacts {
    pub(in crate::game) fn has_recipe_code(&self, station: Station) -> bool {
        self.recipe_types >> station as u32 & 1 != 0
    }

    pub(in crate::game) fn heard(&self, event: Event) -> bool {
        self.events >> event as u32 & 1 != 0
    }

    pub(in crate::game) fn item_bits(&self, id: &str) -> u32 {
        self.items.get(id).copied().unwrap_or(0)
    }

    pub(in crate::game) fn item_has(&self, id: &str, method: ItemMethod) -> bool {
        self.item_bits(id) >> method as u32 & 1 != 0
    }

    /// Whether any loot modifier can apply to this table.
    pub(in crate::game) fn modifiers_for(&self, table: &str) -> bool {
        self.modifiers
            .iter()
            .any(|m| m.tables.as_ref().is_none_or(|t| t.contains(table)))
    }

    pub(in crate::game) fn owns_function(&self, id: &str) -> bool {
        self.functions.contains(id)
    }

    pub(in crate::game) fn owns_condition(&self, id: &str) -> bool {
        self.conditions.contains(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_java_host::wire::Out;

    fn buffer(any_modifier: bool) -> Vec<u8> {
        let mut out = Out::default();
        out.int(0b101);
        out.int(1 << 10 | 1);
        out.int(1);
        out.text("t:charm");
        out.int(0b1101);
        out.int(if any_modifier { 2 } else { 1 });
        out.text("t:sand");
        out.bool(false);
        out.int(1);
        out.text("minecraft:blocks/sand");
        if any_modifier {
            out.text("t:everything");
            out.bool(true);
            out.int(0);
        }
        out.int(1);
        out.text("t:stamp");
        out.int(1);
        out.text("t:always");
        out.0
    }

    #[test]
    fn parse_reads_every_section() {
        let facts = parse(&buffer(false)).unwrap();
        assert!(facts.has_recipe_code(Station::Crafting));
        assert!(!facts.has_recipe_code(Station::Smithing));
        assert!(facts.has_recipe_code(Station::Smelting));
        assert!(facts.heard(Event::ItemCrafted));
        assert!(facts.heard(Event::GetEnchantmentLevel));
        assert!(!facts.heard(Event::AnvilUpdate));
        assert_eq!(facts.item_bits("t:charm"), 0b1101);
        assert!(facts.item_has("t:charm", ItemMethod::GetAllEnchantments));
        assert!(!facts.item_has("t:charm", ItemMethod::GetCraftingRemainder));
        assert_eq!(facts.item_bits("t:other"), 0);
        assert!(facts.owns_function("t:stamp") && !facts.owns_function("t:always"));
        assert!(facts.owns_condition("t:always"));
        assert_eq!(facts.modifiers.len(), 1);
    }

    #[test]
    fn a_truncated_buffer_is_rejected() {
        let bytes = buffer(false);
        assert!(parse(&bytes[..bytes.len() - 1]).is_none());
    }

    #[test]
    fn modifiers_apply_to_their_tables_and_an_any_modifier_to_all() {
        let facts = parse(&buffer(false)).unwrap();
        assert!(facts.modifiers_for("minecraft:blocks/sand"));
        assert!(!facts.modifiers_for("minecraft:blocks/dirt"));
        let facts = parse(&buffer(true)).unwrap();
        assert!(facts.modifiers_for("minecraft:blocks/dirt"));
    }

    // Java's `CraftFacts.STATIONS` and `EVENTS` name the bits in this order.
    #[test]
    fn bit_order_matches_the_java_tables() {
        let stations = [
            ("CRAFTING", Station::Crafting),
            ("SMITHING", Station::Smithing),
            ("SMELTING", Station::Smelting),
            ("BLASTING", Station::Blasting),
            ("SMOKING", Station::Smoking),
            ("CAMPFIRE_COOKING", Station::CampfireCooking),
            ("STONECUTTING", Station::Stonecutting),
        ];
        for (bit, (_, station)) in stations.iter().enumerate() {
            assert_eq!(*station as usize, bit);
        }
        let events = [
            ("ItemCraftedEvent", Event::ItemCrafted),
            ("ItemSmeltedEvent", Event::ItemSmelted),
            ("ItemSmithedEvent", Event::ItemSmithed),
            ("AnvilUpdateEvent", Event::AnvilUpdate),
            ("AnvilCraftEvent.Pre", Event::AnvilCraftPre),
            ("AnvilCraftEvent.Post", Event::AnvilCraftPost),
            ("GrindstoneEvent.OnPlaceItem", Event::GrindstoneOnPlaceItem),
            ("GrindstoneEvent.OnTakeItem", Event::GrindstoneOnTakeItem),
            ("EnchantmentLevelSetEvent", Event::EnchantmentLevelSet),
            ("PlayerEnchantItemEvent", Event::PlayerEnchantItem),
            ("GetEnchantmentLevelEvent", Event::GetEnchantmentLevel),
        ];
        for (bit, (_, event)) in events.iter().enumerate() {
            assert_eq!(*event as usize, bit);
        }
    }
}
