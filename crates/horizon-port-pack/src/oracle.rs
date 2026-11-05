//! The NeoForge oracle's facts (`tools/neoforge_oracle.py export`, in
//! `<work>/oracle/facts.json`): every block, item, recipe and tag the pack's
//! mods register, read from the running game, and the vanilla blocks beside
//! them. Carry-over uses them in place of guesses: a block's template is the
//! vanilla block of the same class and states whose shapes and sounds match
//! best, and the declared properties are the mod's own numbers.
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug, Deserialize)]
pub struct Facts {
    /// The game version that exported them (`1.21.1` facts keep components in that version's form).
    #[serde(default)]
    pub minecraft: String,
    /// What the mods registered; absent in facts from before it was exported.
    #[serde(default)]
    pub owned: Option<Owned>,
    pub blocks: BTreeMap<String, Block>,
    pub items: BTreeMap<String, Item>,
    /// The mods' entries of `minecraft:data_component_type`; empty in facts from before they were exported.
    #[serde(default)]
    pub data_component_types: BTreeMap<String, DataComponentType>,
    #[serde(default)]
    pub tags: Tags,
    /// The game's items' default components (`items.json`), so item ports leave out what their template has.
    #[serde(skip)]
    pub vanilla_items: BTreeMap<String, Value>,
    /// The game's items with a class behaviour (`block-behaviours.json`'s `items`), never a template.
    #[serde(skip)]
    pub vanilla_item_classes: BTreeSet<String>,
}

/// The ids the mods registered, by registry.
#[derive(Debug, Default, Deserialize)]
pub struct Owned {
    #[serde(default)]
    pub block: BTreeSet<String>,
    #[serde(default)]
    pub item: BTreeSet<String>,
    #[serde(default)]
    pub block_entity_type: BTreeSet<String>,
    #[serde(default)]
    pub data_component_type: BTreeSet<String>,
}

/// A JSON object read with its keys in file order.
#[derive(Debug)]
pub struct Ordered<T>(pub Vec<(String, T)>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Ordered<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visit<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Visit<T> {
            type Value = Ordered<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut m: A,
            ) -> Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(entry) = m.next_entry()? {
                    out.push(entry);
                }
                Ok(Ordered(out))
            }
        }
        d.deserialize_map(Visit(std::marker::PhantomData))
    }
}

/// The oracle's baked block models (`<work>/oracle/baked-models.json`): for blocks whose
/// geometry a mod's code builds, each state's quads as the game baked them.
#[derive(Debug, Deserialize)]
pub struct BakedModels {
    pub blocks: Ordered<Ordered<BakedState>>,
}

#[derive(Debug, Deserialize)]
pub struct BakedState {
    #[serde(default)]
    pub particle: String,
    pub quads: Vec<Value>,
}

impl BakedModels {
    pub fn read(path: &Path) -> Result<BakedModels, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct Tags {
    #[serde(default)]
    pub block: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub item: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct Block {
    /// The class and its superclasses, most derived first.
    pub class: Vec<String>,
    pub properties: BTreeMap<String, Vec<String>>,
    pub default: String,
    pub item: String,
    pub explosion_resistance: f32,
    pub friction: f32,
    pub speed_factor: f32,
    pub jump_factor: f32,
    #[serde(default)]
    pub block_set_type: Option<SetType>,
    #[serde(default)]
    pub wood_type: Option<String>,
    pub states: BTreeMap<String, State>,
    /// Its loot table in 26.3's form, from the game's loaded data (boot facts only).
    #[serde(default)]
    pub loot_table: Option<Value>,
    #[serde(default)]
    pub loot_table_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetType {
    pub name: String,
    pub can_open_by_hand: bool,
}

#[derive(Debug, Deserialize)]
pub struct State {
    pub hardness: f32,
    pub emission: u8,
    pub requires_tool: bool,
    pub map_color_id: u8,
    #[serde(default = "occludes")]
    pub can_occlude: bool,
    #[serde(default)]
    pub random_ticks: bool,
    /// Light: absent in facts exported before these were.
    #[serde(default)]
    pub light_dampening: Option<u8>,
    #[serde(default)]
    pub skylight_down: Option<bool>,
    #[serde(default)]
    pub solid_render: Option<bool>,
    #[serde(default)]
    pub shape_occlusion: Option<bool>,
    #[serde(default)]
    pub occlusion: Option<Value>,
    pub sound: Value,
    pub outline: Value,
    pub collision: Value,
    /// What breaking the state drops, per tool (`hand`, `diamond_pickaxe`,
    /// `shears`, `silk_touch_pickaxe`...): item id → count.
    #[serde(default)]
    pub drops: BTreeMap<String, BTreeMap<String, u32>>,
}

fn occludes() -> bool {
    true
}

/// The light values a state's copy of `base` would get wrong, which the light engine reads per state.
fn light_overrides(over: &mut Map<String, Value>, s: &State, base: Option<&State>, all: bool) {
    let differs =
        |ours: Option<Value>, theirs: Option<Value>| ours.is_some() && (all || ours != theirs);
    let pick = |f: fn(&State) -> Option<Value>| (f(s), base.and_then(f));
    for (key, (ours, theirs)) in [
        (
            "light_dampening",
            pick(|v| v.light_dampening.map(Value::from)),
        ),
        ("skylight_down", pick(|v| v.skylight_down.map(Value::from))),
        ("solid_render", pick(|v| v.solid_render.map(Value::from))),
        ("can_occlude", pick(|v| Some(Value::from(v.can_occlude)))),
        (
            "shape_occlusion",
            pick(|v| v.shape_occlusion.map(Value::from)),
        ),
        ("occlusion", pick(|v| v.occlusion.clone())),
    ] {
        if differs(ours.clone(), theirs) {
            over.insert(key.into(), ours.unwrap_or_default());
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Item {
    pub class: String,
    pub max_stack_size: u8,
    pub rarity: String,
    #[serde(default)]
    pub block: Option<String>,
    #[serde(default)]
    pub food: Option<Value>,
    /// The default data components in 1.21.1's form; empty in facts from before they were exported.
    #[serde(default)]
    pub components: Map<String, Value>,
    #[serde(default)]
    pub attribute_modifiers: Option<Value>,
    #[serde(default)]
    pub enchantment_value: Option<u32>,
    #[serde(default)]
    pub repair_items: Option<Vec<String>>,
    #[serde(default)]
    pub armor: Option<Armor>,
}

/// An item's declaration in a content mod: its template and what it changes.
#[derive(Debug, Default, PartialEq)]
pub struct ItemDecl {
    pub template: String,
    pub max_stack_size: Option<u8>,
    pub rarity: Option<String>,
    /// The component patch over the template's.
    pub components: Map<String, Value>,
    /// What the declaration can't carry, for the report.
    pub differences: Vec<String>,
}

impl ItemDecl {
    /// The fields of an item file (`template` left to the caller), or of a block file's `item_properties`.
    pub fn fields(&self) -> Map<String, Value> {
        let mut out = Map::new();
        if let Some(n) = self.max_stack_size {
            out.insert("max_stack_size".into(), json!(n));
        }
        if let Some(r) = &self.rarity {
            out.insert("rarity".into(), json!(r));
        }
        if !self.components.is_empty() {
            out.insert("components".into(), Value::Object(self.components.clone()));
        }
        out
    }
}

#[derive(Debug, Deserialize)]
pub struct DataComponentType {
    /// `!DataComponentType.isTransient()`: it has a codec, so stacks save it and commands name it.
    pub persistent: bool,
}

#[derive(Debug, Deserialize)]
pub struct Armor {
    /// `head`, `chest`, `legs` or `feet`.
    pub slot: String,
}

/// 1.21.1's `{"modifiers": [...]}` as 26.3's list, with attribute ids' `generic.` and `player.` dropped.
fn modifiers_26_3(v: &Value) -> Value {
    let list = v["modifiers"].as_array().cloned().unwrap_or_default();
    Value::Array(
        list.into_iter()
            .map(|mut m| {
                if let Some(t) = m["type"].as_str() {
                    let t = t.replacen(":generic.", ":", 1).replacen(":player.", ":", 1);
                    m["type"] = json!(t);
                }
                m
            })
            .collect(),
    )
}

const VANILLA_PACKAGE: &str = "net.minecraft.";

/// A state key with its properties sorted, so keys compare across blocks.
/// Which states of a block drop the block itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelfDrop {
    /// Every state: `dropSelf`.
    Always,
    /// Only the states where this property has this value.
    When(String, String),
}

/// A state key's `property=value` pairs.
fn pairs(key: &str) -> BTreeSet<(String, String)> {
    key.split(',')
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

pub fn state_key(key: &str) -> String {
    let mut parts: Vec<&str> = key.split(',').filter(|p| !p.is_empty()).collect();
    parts.sort_unstable();
    parts.join(",")
}

impl Block {
    /// The first vanilla class in the chain.
    pub fn vanilla_class(&self) -> &str {
        self.class
            .iter()
            .find(|c| c.starts_with(VANILLA_PACKAGE))
            .map_or("", String::as_str)
    }
    /// Its own class is a mod class: behaviour code the template may not have.
    pub fn custom_class(&self) -> bool {
        self.class
            .first()
            .is_some_and(|c| !c.starts_with(VANILLA_PACKAGE))
    }
    /// Every state drops one `item` (the block's own) with every tool, except
    /// where the state needs a tool and gets none: vanilla's `dropSelf`. Or
    /// only the states where one property has one value do, and the others
    /// drop nothing with any tool: a door's lower half (`createDoorTable`).
    /// Mods often register these tables in code, so their jar has no file.
    pub fn drops_itself(&self, item: &str) -> Option<SelfDrop> {
        let one = BTreeMap::from([(item.to_owned(), 1)]);
        let (mut dropping, mut nothing) = (Vec::new(), Vec::new());
        for (key, state) in &self.states {
            if state.drops.is_empty() {
                continue;
            }
            let mut drops = false;
            let mut short = false;
            for got in state.drops.values() {
                if *got == one {
                    drops = true;
                } else if got.is_empty() {
                    short = true;
                } else {
                    return None;
                }
            }
            match (drops, short) {
                (true, true) if !state.requires_tool => return None,
                (true, _) => dropping.push(pairs(key)),
                (false, _) => nothing.push(pairs(key)),
            }
        }
        let (first, rest) = dropping.split_first()?;
        if nothing.is_empty() {
            return Some(SelfDrop::Always);
        }
        first
            .iter()
            .filter(|pair| rest.iter().all(|s| s.contains(*pair)))
            .find(|pair| nothing.iter().all(|s| !s.contains(*pair)))
            .map(|(p, v)| SelfDrop::When(p.clone(), v.clone()))
    }
    fn default_state(&self) -> Option<&State> {
        self.states
            .get(&self.default)
            .or_else(|| self.states.values().next())
    }
    fn same_states(&self, other: &Block) -> bool {
        self.properties.len() == other.properties.len()
            && self.properties.iter().all(|(k, v)| {
                other.properties.get(k).is_some_and(|w| {
                    v.iter().collect::<BTreeSet<_>>() == w.iter().collect::<BTreeSet<_>>()
                })
            })
    }
    fn by_key(&self) -> BTreeMap<String, &State> {
        self.states.iter().map(|(k, s)| (state_key(k), s)).collect()
    }
}

/// The template carry-over gives a mod block.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub template: String,
    /// `data/<mod>/horizon/block/<name>.json`'s `properties`.
    pub properties: Map<String, Value>,
    /// The block's own states, when no template has them: its `states`.
    pub states: Option<BTreeMap<String, Vec<String>>>,
    /// Its `default_state`.
    pub default_state: Map<String, Value>,
    /// Its `per_state`: only what the rest of the block doesn't already give.
    pub per_state: Map<String, Value>,
    /// What the template does not reproduce, for the report.
    pub differences: Vec<String>,
}

/// Equal JSON, numbers compared as f64 (the oracle writes `6.0` where the game's table has `6`).
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

impl Facts {
    pub fn read(path: &Path) -> Result<Facts, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The vanilla block that best stands in for `id`: the same vanilla
    /// class and states, then the most states with the same outline and
    /// collision, then the same sounds, set type, wood type and hardness.
    /// `None` when no vanilla block has its class and states.
    pub fn template(&self, id: &str, allowed: &dyn Fn(&str) -> bool) -> Option<Choice> {
        let block = self.blocks.get(id)?;
        let class = block.vanilla_class();
        let own = block.by_key();
        let ours = block.default_state()?;
        let mut best: Option<(i64, &str, &Block)> = None;
        for (name, v) in &self.blocks {
            if self.owns_block(name)
                || v.vanilla_class() != class
                || v.custom_class()
                || !block.same_states(v)
                || !allowed(name)
            {
                continue;
            }
            let theirs = v.by_key();
            let shapes = own
                .iter()
                .filter(|(k, s)| {
                    theirs
                        .get(*k)
                        .is_some_and(|t| t.outline == s.outline && t.collision == s.collision)
                })
                .count() as i64;
            let Some(td) = v.default_state() else {
                continue;
            };
            let same_set = match (&block.block_set_type, &v.block_set_type) {
                (Some(a), Some(b)) => {
                    (a.name == b.name) as i64 * 2
                        + (a.can_open_by_hand == b.can_open_by_hand) as i64 * 8
                }
                _ => 0,
            };
            let score = shapes * 1000
                + (td.sound == ours.sound) as i64 * 100
                + same_set * 10
                + (block.wood_type.is_some() && block.wood_type == v.wood_type) as i64 * 20
                + (td.hardness == ours.hardness) as i64 * 5
                + (td.map_color_id == ours.map_color_id) as i64;
            if best.is_none_or(|(s, n, _)| score > s || (score == s && name.as_str() < n)) {
                best = Some((score, name, v));
            }
        }
        if let Some((_, template, t)) = best {
            return Some(self.choice(block, template, t, false, allowed));
        }
        self.own_states_template(block, ours, allowed)
    }

    /// No vanilla block has its states, so it declares them over the nearest vanilla block
    /// up its class chain that has none it doesn't: `Block` always does.
    fn own_states_template(
        &self,
        block: &Block,
        ours: &State,
        allowed: &dyn Fn(&str) -> bool,
    ) -> Option<Choice> {
        let chain = block
            .class
            .iter()
            .filter(|c| c.starts_with(VANILLA_PACKAGE));
        for class in chain {
            let mut best: Option<(i64, &str, &Block)> = None;
            for (name, v) in &self.blocks {
                if self.owns_block(name)
                    || v.class.first() != Some(class)
                    || !v
                        .properties
                        .keys()
                        .all(|k| block.properties.contains_key(k))
                    || !allowed(name)
                {
                    continue;
                }
                let Some(td) = v.default_state() else {
                    continue;
                };
                let score = (td.outline == ours.outline && td.collision == ours.collision) as i64
                    * 1000
                    + (td.can_occlude == ours.can_occlude) as i64 * 500
                    + (td.sound == ours.sound) as i64 * 100
                    + (td.hardness == ours.hardness) as i64 * 5
                    + (td.map_color_id == ours.map_color_id) as i64;
                if best.is_none_or(|(s, n, _)| score > s || (score == s && name.as_str() < n)) {
                    best = Some((score, name, v));
                }
            }
            if let Some((_, template, t)) = best {
                return Some(self.choice(block, template, t, true, allowed));
            }
        }
        None
    }

    fn choice(
        &self,
        block: &Block,
        template: &str,
        t: &Block,
        own_states: bool,
        allowed: &dyn Fn(&str) -> bool,
    ) -> Choice {
        let (ours, theirs) = (block.default_state(), t.default_state());
        let mut properties = Map::new();
        let mut differences = Vec::new();
        if let Some(s) = ours {
            properties.insert("hardness".into(), json!(s.hardness));
            properties.insert(
                "explosion_resistance".into(),
                json!(block.explosion_resistance),
            );
            properties.insert("light".into(), json!(s.emission));
            properties.insert("requires_tool".into(), json!(s.requires_tool));
            properties.insert("friction".into(), json!(block.friction));
            properties.insert("map_color".into(), json!(s.map_color_id));
            if theirs.is_none_or(|t| t.sound != s.sound) {
                match self.sound_block(&s.sound, allowed) {
                    Some(b) => {
                        properties.insert("sounds".into(), json!(b));
                    }
                    None => differences.push("sounds: no vanilla block has them".into()),
                }
            }
        }
        if (block.speed_factor, block.jump_factor) != (t.speed_factor, t.jump_factor) {
            differences.push(format!(
                "speed and jump factors {} {} (template {} {})",
                block.speed_factor, block.jump_factor, t.speed_factor, t.jump_factor
            ));
        }
        let theirs = t.by_key();
        let fallback = t.default_state();
        let block_light = ours.map(|s| s.emission);
        let block_ticks = fallback.map(|s| s.random_ticks);
        let mut per_state = Map::new();
        let mut varying = BTreeSet::new();
        let first = ours.map(|s| (s.hardness, s.requires_tool, s.map_color_id));
        for (k, s) in block.by_key() {
            let base = theirs.get(&k).copied().or(fallback);
            let mut over = Map::new();
            if Some(s.emission) != block_light {
                over.insert("light".into(), json!(s.emission));
            }
            if Some(s.random_ticks) != block_ticks {
                over.insert("random_ticks".into(), json!(s.random_ticks));
            }
            // A template with states of its own copies whichever agrees on what they share, so name them all.
            let unsure = own_states && !t.properties.is_empty();
            if unsure || base.is_none_or(|v| v.outline != s.outline) {
                over.insert("outline".into(), s.outline.clone());
            }
            if unsure || base.is_none_or(|v| v.collision != s.collision) {
                over.insert("collision".into(), s.collision.clone());
            }
            light_overrides(&mut over, s, base, unsure);
            if !over.is_empty() {
                per_state.insert(k, Value::Object(over));
            }
            if Some((s.hardness, s.requires_tool, s.map_color_id)) != first {
                varying.insert("per-state hardness, tool or map colour");
            }
        }
        if own_states {
            differences.push(format!(
                "its own states on {template}, whose behaviour knows none of them"
            ));
        }
        differences.extend(varying.into_iter().map(str::to_owned));
        if block.custom_class() {
            differences.push(format!(
                "class {} (behaviour beyond {})",
                block.class[0],
                block.vanilla_class()
            ));
        }
        Choice {
            template: template.to_owned(),
            properties,
            states: own_states.then(|| block.properties.clone()),
            default_state: pairs(&block.default)
                .into_iter()
                .map(|(k, v)| (k, json!(v)))
                .collect(),
            per_state,
            differences,
        }
    }

    /// A vanilla block whose default state has these sounds.
    fn sound_block(&self, sound: &Value, allowed: &dyn Fn(&str) -> bool) -> Option<&str> {
        self.blocks
            .iter()
            .filter(|(n, _)| !self.owns_block(n) && allowed(n))
            .find(|(_, b)| b.default_state().is_some_and(|s| &s.sound == sound))
            .map(|(n, _)| n.as_str())
    }

    /// Whether a mod registered the block; facts without `owned` know only vanilla's namespace.
    pub fn owns_block(&self, id: &str) -> bool {
        match &self.owned {
            Some(o) => o.block.contains(id),
            None => !id.starts_with("minecraft:"),
        }
    }

    /// Whether a mod registered the item, as [`Facts::owns_block`].
    pub fn owns_item(&self, id: &str) -> bool {
        match &self.owned {
            Some(o) => o.item.contains(id),
            None => !id.starts_with("minecraft:"),
        }
    }

    /// Facts exported by a 1.21 game, whose components aren't 26.3's.
    fn legacy(&self) -> bool {
        self.minecraft.starts_with("1.")
    }

    /// A mod item that isn't a declared block's: a template derived from its components, and its components as a patch over it.
    pub fn item(&self, id: &str) -> Result<(Value, Vec<String>), String> {
        let item = self.items.get(id).ok_or("not in the oracle")?;
        if item.block.is_some() {
            return Err("a block item without a block".into());
        }
        self.item_alone(id)
    }

    /// A block's item declared as an item of its own, for a block whose template has no item to copy.
    pub fn item_alone(&self, id: &str) -> Result<(Value, Vec<String>), String> {
        let item = self.items.get(id).ok_or("not in the oracle")?;
        let template = self.item_template(&self.template_components(item));
        let decl = self.item_decl(item, &template);
        let mut v = json!({"template": decl.template});
        v.as_object_mut().expect("an object").extend(decl.fields());
        Ok((v, decl.differences))
    }

    /// A declared block's item: the block template's item stands in, with the mod item's components.
    pub fn block_item(&self, id: &str, template_item: &str) -> Option<ItemDecl> {
        let item = self.items.get(id)?;
        Some(self.item_decl(item, template_item))
    }

    /// The components a template is derived from; 1.21 armor names its slot outside them.
    fn template_components(&self, item: &Item) -> Map<String, Value> {
        let mut c = item.components.clone();
        if let Some(armor) = item.armor.as_ref().filter(|_| self.legacy()) {
            c.insert("minecraft:equippable".into(), json!({"slot": armor.slot}));
        }
        c
    }

    /// The class-less vanilla item whose components are nearest: the same equipment slot, then the most types shared, the fewest extra.
    pub fn item_template(&self, components: &Map<String, Value>) -> String {
        let slot = |c: &Map<String, Value>| {
            c.get("minecraft:equippable")
                .and_then(|e| e.get("slot"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let ours = slot(components);
        let mut best: Option<(i64, &str)> = None;
        for (name, theirs) in &self.vanilla_items {
            let Some(theirs) = theirs.as_object() else {
                continue;
            };
            // Air is the empty stack's item, never a stack's template.
            if self.vanilla_item_classes.contains(name) || name == "minecraft:air" {
                continue;
            }
            let shared = theirs
                .keys()
                .filter(|k| components.contains_key(*k))
                .count() as i64;
            let extra = theirs.len() as i64 - shared;
            let score = (slot(theirs) == ours) as i64 * 1000 + shared * 2 - extra;
            if best.is_none_or(|(s, n)| score > s || (score == s && name.as_str() < n)) {
                best = Some((score, name));
            }
        }
        best.map_or("minecraft:stick", |(_, n)| n).to_owned()
    }

    fn item_decl(&self, item: &Item, template: &str) -> ItemDecl {
        let mut decl = ItemDecl {
            template: template.to_owned(),
            ..Default::default()
        };
        if !item.class.starts_with(VANILLA_PACKAGE) {
            decl.differences
                .push(format!("class {} (its code runs in Java)", item.class));
        }
        let base = self.vanilla_items.get(template).and_then(Value::as_object);
        let had = |key: &str| base.and_then(|b| b.get(key));
        if had("minecraft:max_stack_size").and_then(Value::as_u64)
            != Some(item.max_stack_size.into())
        {
            decl.max_stack_size = Some(item.max_stack_size);
        }
        match item.rarity.as_str() {
            r if had("minecraft:rarity").and_then(Value::as_str) == Some(r) => {}
            r @ ("common" | "uncommon" | "rare" | "epic") => decl.rarity = Some(r.to_owned()),
            // A loader lets a mod extend `Rarity`; a Horizon item has vanilla's only.
            other => decl.differences.push(format!("rarity {other}")),
        }
        decl.components = if self.legacy() {
            self.legacy_components(item, base)
        } else {
            self.component_patch(item, base, &mut decl.differences)
        };
        decl
    }

    /// Every default component as the patch over the template's that gives exactly them.
    fn component_patch(
        &self,
        item: &Item,
        base: Option<&Map<String, Value>>,
        differences: &mut Vec<String>,
    ) -> Map<String, Value> {
        let known: BTreeSet<&str> = self
            .vanilla_items
            .values()
            .filter_map(Value::as_object)
            .flat_map(|c| c.keys().map(String::as_str))
            .collect();
        let own = |k: &str| {
            self.data_component_types.contains_key(k)
                || self
                    .owned
                    .as_ref()
                    .is_some_and(|o| o.data_component_type.contains(k))
        };
        let mut patch = Map::new();
        for (key, value) in &item.components {
            if matches!(
                key.as_str(),
                "minecraft:max_stack_size" | "minecraft:rarity"
            ) || own(key)
            {
                continue;
            }
            if !known.contains(key.as_str()) {
                differences.push(format!("component {key}: no item of the game has it"));
                continue;
            }
            if base
                .and_then(|b| b.get(key))
                .is_none_or(|h| !same(value, h))
            {
                patch.insert(key.clone(), value.clone());
            }
        }
        for key in base.into_iter().flat_map(|b| b.keys()) {
            if !item.components.contains_key(key)
                && !matches!(
                    key.as_str(),
                    "minecraft:max_stack_size" | "minecraft:rarity"
                )
            {
                patch.insert(format!("!{key}"), json!({}));
            }
        }
        patch
    }

    /// 1.21.1's numbers as a 26.3 component patch over the template's, leaving out what already matches.
    fn legacy_components(
        &self,
        item: &Item,
        base: Option<&Map<String, Value>>,
    ) -> Map<String, Value> {
        if item.components.is_empty() {
            return Map::new();
        }
        let mut want = Map::new();
        want.insert(
            "minecraft:max_damage".into(),
            item.components
                .get("minecraft:max_damage")
                .cloned()
                .unwrap_or(Value::Null),
        );
        if let Some(m) = &item.attribute_modifiers {
            want.insert("minecraft:attribute_modifiers".into(), modifiers_26_3(m));
        }
        // The mod's rules as they are; what 1.21.1 kept in the class (can_destroy_blocks_in_creative) stays the template's.
        let tool = item.components.get("minecraft:tool").map(|t| {
            let mut tool = base
                .and_then(|b| b.get("minecraft:tool"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            for key in ["rules", "default_mining_speed", "damage_per_block"] {
                match t.get(key) {
                    Some(v) => tool[key] = v.clone(),
                    None => _ = tool.as_object_mut().map(|o| o.remove(key)),
                }
            }
            tool
        });
        want.insert("minecraft:tool".into(), tool.unwrap_or(Value::Null));
        if let Some(value) = item.enchantment_value {
            let enchantable = (value > 0).then(|| json!({"value": value}));
            want.insert(
                "minecraft:enchantable".into(),
                enchantable.unwrap_or(Value::Null),
            );
        }
        if let Some(repair) = &item.repair_items {
            let items = match repair.as_slice() {
                [] => Value::Null,
                [one] => json!({"items": one}),
                many => json!({"items": many}),
            };
            want.insert("minecraft:repairable".into(), items);
        }
        let mut patch = Map::new();
        for (key, value) in want {
            let had = base.and_then(|b| b.get(&key));
            match (value, had) {
                (Value::Null, None) => {}
                (Value::Null, Some(_)) => _ = patch.insert(format!("!{key}"), json!({})),
                (v, Some(h)) if same(&v, h) => {}
                (v, _) => _ = patch.insert(key, v),
            }
        }
        patch
    }

    /// A tag's members the content mod may name: vanilla's and its own.
    pub fn tag_members(&self, kind: &str, tag: &str, own: &str) -> Option<Vec<String>> {
        let map = if kind == "block" {
            &self.tags.block
        } else {
            &self.tags.item
        };
        let members = map.get(tag)?;
        Some(
            members
                .iter()
                .filter(|m| {
                    m.split_once(':')
                        .is_some_and(|(ns, _)| ns == "minecraft" || ns == own)
                })
                .cloned()
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(hardness: f32, sound: &str, outline: Value) -> Value {
        json!({"hardness": hardness, "emission": 0, "requires_tool": false, "map_color_id": 13,
            "sound": {"break": sound}, "outline": outline, "collision": outline})
    }
    fn block(class: &[&str], props: Value, states: Value) -> Value {
        let default = states
            .as_object()
            .unwrap()
            .keys()
            .next()
            .unwrap()
            .to_owned();
        json!({"class": class, "properties": props, "default": default, "item": "x",
            "explosion_resistance": 3.0, "friction": 0.6, "speed_factor": 1.0,
            "jump_factor": 1.0, "states": states})
    }

    #[test]
    fn templates_follow_class_shapes_and_sounds() {
        let cube = json!([[0, 0, 0, 1, 1, 1]]);
        let low = json!([[0, 0, 0, 1, 0.9375, 1]]);
        let block_class = ["net.minecraft.world.level.block.Block"];
        let path = ["net.minecraft.world.level.block.DirtPathBlock"];
        let facts: Facts = serde_json::from_value(json!({
            "blocks": {
                "minecraft:stone": block(&block_class, json!({}), json!({"": state(1.5, "stone", cube.clone())})),
                "minecraft:oak_planks": block(&block_class, json!({}), json!({"": state(2.0, "wood", cube.clone())})),
                "minecraft:dirt_path": block(&path, json!({}), json!({"": state(0.65, "grass", low.clone())})),
                "m:wood_tile": block(&["m.Tile", block_class[0]], json!({}), json!({"": state(2.5, "wood", cube.clone())})),
                "m:path": block(&["m.Path", block_class[0]], json!({}), json!({"": state(1.0, "stone", low.clone())})),
                "m:gadget": block(&["m.Gadget", block_class[0]], json!({"mode": ["a", "b"]}),
                    json!({"mode=a": state(1.0, "stone", cube.clone()), "mode=b": state(1.0, "stone", cube)})),
            },
            "items": {}
        }))
        .unwrap();
        let any = |_: &str| true;
        let tile = facts.template("m:wood_tile", &any).unwrap();
        assert_eq!(tile.template, "minecraft:oak_planks");
        assert_eq!(tile.properties["hardness"], json!(2.5));
        assert!(!tile.properties.contains_key("sounds"));
        assert_eq!(
            tile.differences,
            ["class m.Tile (behaviour beyond net.minecraft.world.level.block.Block)"]
        );
        // A plain Block is never templated on a DirtPathBlock: it keeps its own shape on a stone.
        let path = facts.template("m:path", &any).unwrap();
        assert_eq!(path.template, "minecraft:stone");
        assert_eq!(path.per_state[""]["outline"], low);
        // No vanilla Block has `mode`, so the block declares it over one that has no states.
        let gadget = facts.template("m:gadget", &any).unwrap();
        assert_eq!(gadget.template, "minecraft:stone");
        assert_eq!(gadget.states.unwrap()["mode"], ["a", "b"]);
        assert_eq!(gadget.default_state["mode"], json!("a"));
        assert!(gadget.differences[0].starts_with("its own states"));
        let tile = facts
            .template("m:wood_tile", &|n: &str| n != "minecraft:oak_planks")
            .unwrap();
        assert_eq!(tile.template, "minecraft:stone");
        assert_eq!(tile.differences[0], "sounds: no vanilla block has them");
    }

    #[test]
    fn drops_itself_is_vanillas_drop_self() {
        let cube = json!([[0, 0, 0, 1, 1, 1]]);
        let with = |requires_tool: bool, drops: Value| {
            let mut s = state(1.5, "stone", cube.clone());
            s["requires_tool"] = json!(requires_tool);
            s["drops"] = drops;
            let b: Block = serde_json::from_value(block(&[], json!({}), json!({"": s}))).unwrap();
            b.drops_itself("m:tile")
        };
        let me = json!({"m:tile": 1});
        let always = Some(SelfDrop::Always);
        assert_eq!(with(false, json!({"hand": me, "shears": me})), always);
        // A state that needs a tool drops nothing by hand, as dropSelf does.
        assert_eq!(
            with(true, json!({"hand": {}, "diamond_pickaxe": me})),
            always
        );
        assert_eq!(
            with(false, json!({"hand": {}, "diamond_pickaxe": me})),
            None
        );
        // Two of itself (a double slab), or something else, is not dropSelf.
        assert_eq!(with(false, json!({"hand": {"m:tile": 2}})), None);
        assert_eq!(with(false, json!({"hand": {"m:other": 1}})), None);
        assert_eq!(with(false, json!({})), None);
    }

    #[test]
    fn a_door_drops_from_its_lower_half() {
        let cube = json!([[0, 0, 0, 1, 1, 1]]);
        let me = json!({"hand": {"m:door": 1}, "shears": {"m:door": 1}});
        let none = json!({"hand": {}, "shears": {}});
        let door = |upper: &Value| {
            let mut states = serde_json::Map::new();
            for (key, drops) in [
                ("half=lower,open=false", &me),
                ("half=lower,open=true", &me),
                ("half=upper,open=false", upper),
                ("half=upper,open=true", upper),
            ] {
                let mut s = state(3.0, "wood", cube.clone());
                s["drops"] = drops.clone();
                states.insert(key.to_owned(), s);
            }
            let props = json!({"half": ["upper", "lower"], "open": ["true", "false"]});
            let b: Block =
                serde_json::from_value(block(&[], props, Value::Object(states))).unwrap();
            b.drops_itself("m:door")
        };
        assert_eq!(
            door(&none),
            Some(SelfDrop::When("half".to_owned(), "lower".to_owned()))
        );
        assert_eq!(door(&me), Some(SelfDrop::Always));
        // Something else from the upper half: not a door table.
        assert_eq!(door(&json!({"hand": {"m:other": 1}})), None);
    }

    #[test]
    fn legacy_items_take_a_template_and_their_numbers() {
        let mut facts: Facts = serde_json::from_value(json!({
            "minecraft": "1.21.1",
            "blocks": {},
            "items": {
                "m:sword": {"class": "net.minecraft.world.item.SwordItem", "max_stack_size": 1, "rarity": "common",
                    "components": {"minecraft:max_damage": 905, "minecraft:tool": {"rules": [
                        {"blocks": "minecraft:cobweb", "speed": 15.0, "correct_for_drops": true}], "damage_per_block": 2}},
                    "attribute_modifiers": {"modifiers": [{"type": "minecraft:generic.attack_damage",
                        "id": "minecraft:base_attack_damage", "amount": 5.5, "operation": "add_value", "slot": "mainhand"}]},
                    "enchantment_value": 14, "repair_items": ["minecraft:emerald"]},
                "m:boots": {"class": "net.minecraft.world.item.ArmorItem", "max_stack_size": 1, "rarity": "common",
                    "components": {"minecraft:max_damage": 1}, "enchantment_value": 0, "repair_items": [],
                    "armor": {"slot": "feet"}},
                "m:gem": {"class": "net.minecraft.world.item.Item", "max_stack_size": 64, "rarity": "common"},
            }
        }))
        .unwrap();
        facts.vanilla_items = serde_json::from_value(json!({
            "minecraft:iron_sword": {"minecraft:max_damage": 250, "minecraft:enchantable": {"value": 14},
                "minecraft:repairable": {"items": "#minecraft:iron_tool_materials"},
                "minecraft:tool": {"can_destroy_blocks_in_creative": false, "damage_per_block": 2,
                    "rules": [{"blocks": "#minecraft:sword_efficient", "speed": 1.5}]}},
            "minecraft:iron_boots": {"minecraft:max_damage": 195, "minecraft:enchantable": {"value": 9},
                "minecraft:repairable": {"items": "#minecraft:repairs_iron_armor"},
                "minecraft:equippable": {"slot": "feet"}},
            "minecraft:stick": {"minecraft:max_stack_size": 64, "minecraft:rarity": "common"},
        }))
        .unwrap();
        let (sword, _) = facts.item("m:sword").unwrap();
        assert_eq!(sword["template"], "minecraft:iron_sword");
        let c = &sword["components"];
        assert_eq!(c["minecraft:max_damage"], 905);
        assert_eq!(
            c["minecraft:attribute_modifiers"][0]["type"],
            "minecraft:attack_damage"
        );
        assert_eq!(
            c["minecraft:repairable"],
            json!({"items": "minecraft:emerald"})
        );
        // The mod's rules replace the template's; the creative flag the class gave stays.
        assert_eq!(
            c["minecraft:tool"],
            json!({"can_destroy_blocks_in_creative": false, "damage_per_block": 2,
                "rules": [{"blocks": "minecraft:cobweb", "speed": 15.0, "correct_for_drops": true}]})
        );
        // Equal to the template's: left out.
        assert!(c.get("minecraft:enchantable").is_none());
        let (boots, _) = facts.item("m:boots").unwrap();
        assert_eq!(boots["template"], "minecraft:iron_boots");
        assert_eq!(boots["components"]["!minecraft:enchantable"], json!({}));
        assert_eq!(boots["components"]["!minecraft:repairable"], json!({}));
        // Facts without components (exported before they were) keep the plain template.
        let (gem, _) = facts.item("m:gem").unwrap();
        assert_eq!(gem, json!({"template": "minecraft:stick"}));
    }

    fn current_facts() -> Facts {
        let mut facts: Facts = serde_json::from_value(json!({
            "minecraft": "26.3",
            "owned": {"item": ["m:pie", "m:vest", "m:gadget"], "data_component_type": ["m:energy"]},
            "blocks": {},
            "items": {
                "m:pie": {"class": "m.Pie", "max_stack_size": 64, "rarity": "common", "components": {
                    "minecraft:max_stack_size": 64, "minecraft:rarity": "common",
                    "minecraft:food": {"nutrition": 6, "saturation": 7.2}, "minecraft:consumable": {},
                    "m:energy": 0}},
                "m:vest": {"class": "m.Hazmat", "max_stack_size": 1, "rarity": "rare", "components": {
                    "minecraft:max_stack_size": 1, "minecraft:rarity": "rare", "minecraft:max_damage": 500,
                    "minecraft:equippable": {"slot": "chest", "asset_id": "m:hazmat"}}},
                "m:gadget": {"class": "m.Gadget", "max_stack_size": 16, "rarity": "epic", "components": {
                    "minecraft:max_stack_size": 16, "minecraft:rarity": "epic"}},
                "m:block": {"class": "net.minecraft.world.item.BlockItem", "max_stack_size": 16, "rarity": "uncommon",
                    "block": "m:block", "components": {"minecraft:max_stack_size": 16, "minecraft:rarity": "uncommon",
                    "minecraft:lore": []}},
            }
        }))
        .unwrap();
        facts.vanilla_items = serde_json::from_value(json!({
            "minecraft:air": {"minecraft:max_stack_size": 64, "minecraft:rarity": "common"},
            "minecraft:stick": {"minecraft:max_stack_size": 64, "minecraft:rarity": "common",
                "minecraft:cooking_fuel": {"burn_time": 100}},
            "minecraft:apple": {"minecraft:max_stack_size": 64, "minecraft:rarity": "common",
                "minecraft:food": {"nutrition": 4, "saturation": 2.4}, "minecraft:consumable": {}},
            "minecraft:iron_chestplate": {"minecraft:max_stack_size": 1, "minecraft:rarity": "common",
                "minecraft:max_damage": 240, "minecraft:equippable": {"slot": "chest", "asset_id": "minecraft:iron"}},
            "minecraft:golden_carrot": {"minecraft:max_stack_size": 64, "minecraft:rarity": "common",
                "minecraft:food": {"nutrition": 6, "saturation": 14.4}, "minecraft:consumable": {}},
            "minecraft:stone": {"minecraft:max_stack_size": 64, "minecraft:rarity": "common", "minecraft:lore": []},
        }))
        .unwrap();
        facts.vanilla_item_classes = [
            "minecraft:golden_carrot".to_owned(),
            "minecraft:stone".to_owned(),
        ]
        .into();
        facts
    }

    #[test]
    fn templates_follow_components_not_classes() {
        let facts = current_facts();
        let (pie, differences) = facts.item("m:pie").unwrap();
        assert_eq!(
            pie["template"], "minecraft:apple",
            "food, and golden_carrot has a class"
        );
        assert_eq!(
            pie["components"],
            json!({"minecraft:food": {"nutrition": 6, "saturation": 7.2}}),
            "the mod's own component stays in Java"
        );
        assert!(pie.get("max_stack_size").is_none() && pie.get("rarity").is_none());
        assert_eq!(differences, ["class m.Pie (its code runs in Java)"]);
        let (vest, _) = facts.item("m:vest").unwrap();
        assert_eq!(vest["template"], "minecraft:iron_chestplate");
        assert_eq!(vest["rarity"], "rare");
        assert!(vest.get("max_stack_size").is_none());
        assert_eq!(vest["components"]["minecraft:max_damage"], 500);
        assert_eq!(
            vest["components"]["minecraft:equippable"]["asset_id"],
            "m:hazmat"
        );
        let (gadget, _) = facts.item("m:gadget").unwrap();
        assert_eq!(gadget["template"], "minecraft:stick");
        assert_eq!(gadget["max_stack_size"], 16);
        assert_eq!(gadget["rarity"], "epic");
        assert_eq!(gadget["components"], json!({"!minecraft:cooking_fuel": {}}));
    }

    #[test]
    fn block_items_keep_their_numbers() {
        let facts = current_facts();
        let item = facts.block_item("m:block", "minecraft:stone").unwrap();
        assert_eq!(item.max_stack_size, Some(16));
        assert_eq!(item.rarity.as_deref(), Some("uncommon"));
        assert!(item.components.is_empty(), "{:?}", item.components);
        assert!(
            facts.owns_item("m:pie")
                && !facts.owns_item("m:block")
                && !facts.owns_item("minecraft:stick")
        );
    }

    #[test]
    fn state_keys_sort() {
        assert_eq!(
            state_key("half=lower,facing=north"),
            "facing=north,half=lower"
        );
        assert_eq!(state_key(""), "");
    }
}
