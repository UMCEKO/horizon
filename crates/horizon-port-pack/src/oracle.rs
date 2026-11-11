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
    /// The mods' recipes NeoForge loaded: those whose `neoforge:conditions` passed.
    #[serde(default)]
    pub recipes: BTreeMap<String, serde::de::IgnoredAny>,
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
    /// `BlockBehaviour.canOcclude` (`noOcclusion` clears it); absent in older facts.
    #[serde(default)]
    pub can_occlude: Option<bool>,
    /// What breaking the state drops, per tool (`hand`, `diamond_pickaxe`,
    /// `shears`, `silk_touch_pickaxe`...): item id → count.
    #[serde(default)]
    pub drops: BTreeMap<String, BTreeMap<String, u32>>,
}

/// Reads one per-state value of a block file's field from the oracle's state.
type ReadState = fn(&State) -> Option<Value>;

/// The content format's per-state fields the oracle gives, by their block file key.
const STATE_FIELDS: [(&str, ReadState); 8] = [
    ("shapes", |s| Some(s.outline.clone())),
    ("collision_shapes", |s| Some(s.collision.clone())),
    ("occlusion", |s| s.occlusion.clone()),
    ("light_dampening", |s| s.light_dampening.map(Value::from)),
    ("skylight_down", |s| s.skylight_down.map(Value::from)),
    ("solid_render", |s| s.solid_render.map(Value::from)),
    ("can_occlude", |s| s.can_occlude.map(Value::from)),
    ("shape_occlusion", |s| s.shape_occlusion.map(Value::from)),
];

/// The most boxes the content format takes in one state's shape.
const MAX_SHAPE_BOXES: usize = 16;

/// One field as the content format's state values: the commonest value as `default`, the rest keyed by their states' values of `names`.
fn state_values(names: &[&String], rows: &[(BTreeSet<(String, String)>, Value)]) -> Value {
    let mut counts: Vec<(&Value, usize)> = Vec::new();
    for (_, v) in rows {
        match counts.iter_mut().find(|(c, _)| same(c, v)) {
            Some(c) => c.1 += 1,
            None => counts.push((v, 1)),
        }
    }
    let Some(default) = counts
        .iter()
        .fold(None, |best: Option<(&Value, usize)>, &(v, n)| {
            if best.is_none_or(|(_, b)| n > b) {
                Some((v, n))
            } else {
                best
            }
        })
        .map(|(v, _)| v)
    else {
        return json!({});
    };
    let mut values = Map::new();
    for (state, v) in rows.iter().filter(|(_, v)| !same(v, default)) {
        let key: Vec<&str> = names
            .iter()
            .map(|n| {
                state
                    .iter()
                    .find(|(k, _)| k == *n)
                    .map_or("", |(_, v)| v.as_str())
            })
            .collect();
        values.insert(key.join(","), v.clone());
    }
    if values.is_empty() {
        json!({"default": default})
    } else {
        json!({"properties": names, "values": values, "default": default})
    }
}

/// `RandomTicks` for a block whose states tick unevenly: the one own property whose values pick exactly the ticking states.
fn ticking_property(
    names: &[&String],
    rows: &[(BTreeSet<(String, String)>, bool)],
) -> Option<Value> {
    names.iter().find_map(|name| {
        let value = |state: &BTreeSet<(String, String)>| {
            state
                .iter()
                .find(|(k, _)| k == *name)
                .map(|(_, v)| v.clone())
        };
        let picked: BTreeSet<String> = rows
            .iter()
            .filter(|(_, ticks)| *ticks)
            .filter_map(|(state, _)| value(state))
            .collect();
        rows.iter()
            .all(|(state, ticks)| value(state).is_some_and(|v| picked.contains(&v) == *ticks))
            .then(|| json!({"property": name, "values": picked}))
    })
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
    /// A block file's `components`: the patch, with the stack size and rarity a block's item can only set there.
    pub fn block_components(&self) -> Map<String, Value> {
        let mut out = self.components.clone();
        if let Some(n) = self.max_stack_size {
            out.insert("minecraft:max_stack_size".into(), json!(n));
        }
        if let Some(r) = &self.rarity {
            out.insert("minecraft:rarity".into(), json!(r));
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
    /// The armour material's id: the 26.3 equipment asset it becomes.
    #[serde(default)]
    pub material: String,
    #[serde(default)]
    pub equip_sound: Option<String>,
    #[serde(default)]
    pub layers: Vec<ArmorLayer>,
}

/// A 1.21.1 armour layer's textures: `<ns>:textures/models/armor/<name>_layer_1.png` and `_layer_2`.
#[derive(Debug, Deserialize)]
pub struct ArmorLayer {
    pub texture: String,
    pub texture_legs: String,
    #[serde(default)]
    pub dyeable: bool,
}

impl Armor {
    /// Whether carry writes the material's equipment asset: in the mod's own namespaces, from layers it can place.
    pub fn asset_carried(&self, owned: &[&str]) -> bool {
        self.material
            .split_once(':')
            .is_some_and(|(ns, _)| owned.contains(&ns))
            && self.equipment().is_some()
    }

    /// The 26.3 equipment asset of the material (`layers` by kind), and the 1.21.1 textures to copy, as (from, to) paths in the mod.
    pub fn equipment(&self) -> Option<(Value, Vec<(String, String)>)> {
        self.material.split_once(':')?;
        let mut copies = Vec::new();
        let mut kinds = Map::new();
        for (kind, legs) in [("humanoid", false), ("humanoid_leggings", true)] {
            let mut list = Vec::new();
            for layer in &self.layers {
                let texture = if legs {
                    &layer.texture_legs
                } else {
                    &layer.texture
                };
                let (ns, path) = texture.split_once(':')?;
                let file = path.strip_prefix("textures/models/armor/")?;
                let name = file.strip_suffix(if legs { "_layer_2.png" } else { "_layer_1.png" })?;
                copies.push((
                    format!("assets/{ns}/{path}"),
                    format!("assets/{ns}/textures/entity/equipment/{kind}/{name}.png"),
                ));
                let mut entry = json!({"texture": format!("{ns}:{name}")});
                if layer.dyeable {
                    entry["dyeable"] = json!({});
                }
                list.push(entry);
            }
            kinds.insert(kind.into(), Value::Array(list));
        }
        Some((json!({"layers": kinds}), copies))
    }
}

/// The vanilla item standing in for an item of a vanilla class in 1.21 facts, for the behaviour the class gives.
fn class_template(item: &Item) -> &'static str {
    let class = item.class.strip_prefix("net.minecraft.world.item.");
    match (class, item.armor.as_ref().map(|a| a.slot.as_str())) {
        (Some("SwordItem"), _) => "minecraft:iron_sword",
        (Some("PickaxeItem"), _) => "minecraft:iron_pickaxe",
        (Some("AxeItem"), _) => "minecraft:iron_axe",
        (Some("ShovelItem"), _) => "minecraft:iron_shovel",
        (Some("HoeItem"), _) => "minecraft:iron_hoe",
        (Some("ArmorItem"), Some("head")) => "minecraft:iron_helmet",
        (Some("ArmorItem"), Some("chest")) => "minecraft:iron_chestplate",
        (Some("ArmorItem"), Some("legs")) => "minecraft:iron_leggings",
        (Some("ArmorItem"), Some("feet")) => "minecraft:iron_boots",
        _ => "minecraft:stick",
    }
}

/// 1.21.1's `minecraft:food` as 26.3's components: `food` keeps the numbers, `consumable` the eating
/// time and effects (one `apply_effects` per 1.21.1 effect, each with its own chance), `use_remainder`
/// what eating leaves. Effects of a mod's own are left out (said in `differences`): the game would
/// refuse the item for an effect it doesn't know.
fn food_26_3(f: &Value, differences: &mut Vec<String>) -> Map<String, Value> {
    let mut food = json!({"nutrition": f["nutrition"], "saturation": f["saturation"]});
    if f["can_always_eat"].as_bool() == Some(true) {
        food["can_always_eat"] = json!(true);
    }
    let mut consumable = json!({});
    // 1.6 s is both versions' default.
    if let Some(s) = f["eat_seconds"].as_f64().filter(|s| (s - 1.6).abs() > 1e-6) {
        consumable["consume_seconds"] = json!(s);
    }
    let mut effects = Vec::new();
    for e in f["effects"].as_array().into_iter().flatten() {
        let mut effect = e["effect"].clone();
        let id = effect["id"].as_str().unwrap_or("");
        if !id.starts_with("minecraft:") {
            differences.push(format!("eating gives {id}, an effect of its own"));
            continue;
        }
        effect.as_object_mut().map(|o| o.remove("neoforge:cures"));
        let mut entry = json!({"type": "minecraft:apply_effects", "effects": [effect]});
        if let Some(p) = e["probability"].as_f64().filter(|p| *p < 1.0) {
            entry["probability"] = json!(p);
        }
        effects.push(entry);
    }
    if !effects.is_empty() {
        consumable["on_consume_effects"] = Value::Array(effects);
    }
    let mut out = Map::new();
    out.insert("minecraft:food".into(), food);
    out.insert("minecraft:consumable".into(), consumable);
    if let Some(r) = f.get("using_converts_to") {
        out.insert("minecraft:use_remainder".into(), r.clone());
    }
    out
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
    /// Its per-state fields (`state_light`, `shapes`, `light_dampening`...): only what the rest of the block doesn't already give.
    pub fields: Map<String, Value>,
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
            // 26.3's MapColor ids end at 61; the game refuses the whole mod for one past them.
            if s.map_color_id <= 61 {
                properties.insert("map_color".into(), json!(s.map_color_id));
            } else {
                differences.push(format!("map colour {} (not vanilla's)", s.map_color_id));
            }
            // `noOcclusion` (a block its block entity draws): neighbours' faces stay drawn.
            if s.can_occlude == Some(false) && theirs.is_none_or(|t| t.can_occlude != Some(false)) {
                properties.insert("occludes".into(), json!(false));
            }
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
        let names: Vec<&String> = block.properties.keys().collect();
        // A template with states of its own copies whichever agrees on what they share, so name them all.
        let unsure = own_states && !t.properties.is_empty();
        let rows: Vec<(BTreeSet<(String, String)>, &State, Option<&State>)> = block
            .by_key()
            .into_iter()
            .map(|(k, s)| (pairs(&k), s, theirs.get(&k).copied().or(fallback)))
            .collect();
        let mut fields = Map::new();
        let mut varying = BTreeSet::new();
        let first = ours.map(|s| (s.hardness, s.requires_tool, s.map_color_id));
        for (_, s, _) in &rows {
            if Some((s.hardness, s.requires_tool, s.map_color_id)) != first {
                varying.insert("per-state hardness, tool or map colour");
            }
        }
        if rows
            .iter()
            .any(|(_, s, _)| Some(s.emission) != ours.map(|o| o.emission))
        {
            let light: Vec<_> = rows
                .iter()
                .map(|(state, s, _)| (state.clone(), json!(s.emission)))
                .collect();
            fields.insert("state_light".into(), state_values(&names, &light));
        }
        for (key, read) in STATE_FIELDS {
            let Some(values) = rows
                .iter()
                .map(|(state, s, _)| read(s).map(|v| (state.clone(), v)))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let differs = unsure
                || rows
                    .iter()
                    .zip(&values)
                    .any(|((_, _, base), (_, v))| base.and_then(read).is_none_or(|b| !same(&b, v)));
            if !differs {
                continue;
            }
            let boxes = values
                .iter()
                .filter_map(|(_, v)| v.as_array().map(Vec::len))
                .max()
                .unwrap_or(0);
            if matches!(key, "shapes" | "collision_shapes" | "occlusion") && boxes > MAX_SHAPE_BOXES
            {
                differences.push(format!(
                    "{key}: {boxes} boxes in a state, more than the {MAX_SHAPE_BOXES} a block file holds: the template's"
                ));
                continue;
            }
            fields.insert(key.into(), state_values(&names, &values));
        }
        let ticks: Vec<_> = rows
            .iter()
            .map(|(state, s, _)| (state.clone(), s.random_ticks))
            .collect();
        if unsure
            || rows
                .iter()
                .any(|(_, s, base)| base.is_none_or(|b| b.random_ticks != s.random_ticks))
        {
            let all = ticks.iter().all(|(_, t)| *t);
            if all || ticks.iter().all(|(_, t)| !*t) {
                properties.insert("random_ticks".into(), json!(all));
            } else if let Some(by) = own_states
                .then(|| ticking_property(&names, &ticks))
                .flatten()
            {
                properties.insert("random_ticks".into(), by);
            } else {
                differences.push("random ticks in some of its states only".into());
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
            states: (own_states && !block.properties.is_empty()).then(|| block.properties.clone()),
            default_state: pairs(&block.default)
                .into_iter()
                .map(|(k, v)| (k, json!(v)))
                .collect(),
            fields,
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

    /// Facts exported by a 1.21 game, whose components aren't 26.3's; facts from before the version was exported were all 1.21.1's.
    fn legacy(&self) -> bool {
        self.minecraft.is_empty() || self.minecraft.starts_with("1.")
    }

    /// A mod item that isn't a declared block's: its template, and its components as a patch over it.
    pub fn item(&self, id: &str, owned: &[&str]) -> Result<(Value, Vec<String>), String> {
        let item = self.items.get(id).ok_or("not in the oracle")?;
        let template = if self.legacy() {
            class_template(item).to_owned()
        } else {
            self.item_template(&item.components)
        };
        let decl = self.item_decl(item, &template, owned);
        let mut v = json!({"template": decl.template});
        // A block item whose block has another name (seeds), or a block's item declared alone: its `BlockItem.getBlock`.
        if let Some(block) = &item.block {
            v["block"] = json!(block);
        }
        if let Some(n) = decl.max_stack_size {
            v["max_stack_size"] = json!(n);
        }
        if let Some(r) = &decl.rarity {
            v["rarity"] = json!(r);
        }
        if !decl.components.is_empty() {
            v["components"] = Value::Object(decl.components);
        }
        Ok((v, decl.differences))
    }

    /// A declared block's item: the block template's item stands in, with the mod item's components.
    pub fn block_item(&self, id: &str, template_item: &str, owned: &[&str]) -> Option<ItemDecl> {
        let item = self.items.get(id)?;
        Some(self.item_decl(item, template_item, owned))
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

    fn item_decl(&self, item: &Item, template: &str, owned: &[&str]) -> ItemDecl {
        let mut decl = ItemDecl {
            template: template.to_owned(),
            ..Default::default()
        };
        if !item.class.starts_with(VANILLA_PACKAGE) {
            decl.differences
                .push(format!("class {} (its code runs in Java)", item.class));
        }
        if let Some(armor) = item.armor.as_ref().filter(|a| !a.material.is_empty())
            && !self.armor_asset_exists(armor, owned)
        {
            decl.differences.push(format!(
                "armour material {} has no equipment asset: it draws the template's",
                armor.material
            ));
        }
        let base = self.vanilla_items.get(template).and_then(Value::as_object);
        let had = |key: &str| base.and_then(|b| b.get(key));
        // Without the game's tables the template is taken to have the defaults.
        let stack = had("minecraft:max_stack_size").map_or(Some(64), Value::as_u64);
        if stack != Some(item.max_stack_size.into()) {
            decl.max_stack_size = Some(item.max_stack_size);
        }
        let rarity = had("minecraft:rarity").map_or(Some("common"), Value::as_str);
        match item.rarity.as_str() {
            r if rarity == Some(r) => {}
            r @ ("common" | "uncommon" | "rare" | "epic") => decl.rarity = Some(r.to_owned()),
            // A loader lets a mod extend `Rarity`; a Horizon item has vanilla's only.
            other => decl
                .differences
                .push(format!("rarity {other} (not vanilla's)")),
        }
        decl.components = if self.legacy() {
            let mut components = self.legacy_components(item, base, owned);
            if let Some(food) = item.components.get("minecraft:food").or(item.food.as_ref()) {
                components.extend(food_26_3(food, &mut decl.differences));
            }
            components
        } else {
            self.component_patch(item, base, &mut decl.differences)
        };
        decl
    }

    /// Whether the armour's material has an equipment asset: carried from the mod, or vanilla's.
    fn armor_asset_exists(&self, armor: &Armor, owned: &[&str]) -> bool {
        armor.asset_carried(owned)
            || self.vanilla_items.values().any(|i| {
                i["minecraft:equippable"]["asset_id"].as_str() == Some(armor.material.as_str())
            })
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
        owned: &[&str],
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
        if let Some(armor) = item.armor.as_ref().filter(|a| !a.material.is_empty()) {
            let mut equippable = base.and_then(|b| b.get("minecraft:equippable")).cloned();
            if self.armor_asset_exists(armor, owned) {
                equippable.get_or_insert_with(|| json!({"slot": armor.slot}))["asset_id"] =
                    json!(armor.material);
            }
            if let Some(mut equippable) = equippable {
                if let Some(sound) = &armor.equip_sound {
                    equippable["equip_sound"] = json!(sound);
                }
                want.insert("minecraft:equippable".into(), equippable);
            }
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
        assert_eq!(path.fields["shapes"], json!({"default": low}));
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
    fn a_stateless_block_over_its_class_chain_declares_no_states() {
        let cube = json!([[0, 0, 0, 1, 1, 1]]);
        let block_class = "net.minecraft.world.level.block.Block";
        let entity_block = "net.minecraft.world.level.block.BaseEntityBlock";
        let facts: Facts = serde_json::from_value(json!({
            "blocks": {
                "minecraft:stone": block(&[block_class], json!({}), json!({"": state(1.5, "stone", cube.clone())})),
                "m:anchor": block(&["m.Anchor", entity_block, block_class], json!({}), json!({"": state(0.1, "stone", cube)})),
            },
            "items": {}
        }))
        .unwrap();
        let anchor = facts.template("m:anchor", &|_: &str| true).unwrap();
        assert_eq!(anchor.template, "minecraft:stone");
        assert_eq!(anchor.states, None);
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

    /// `noOcclusion` on a full-cube block (a cannon its block entity draws): the oracle's
    /// `can_occlude: false` carries, so the neighbours' faces behind it are drawn.
    #[test]
    fn a_block_that_cannot_occlude_hides_no_neighbour_faces() {
        let cube = json!([[0, 0, 0, 1, 1, 1]]);
        let plain = ["net.minecraft.world.level.block.Block"];
        let mut open = state(5.0, "anvil", cube.clone());
        open["can_occlude"] = json!(false);
        let mut solid = state(5.0, "metal", cube.clone());
        solid["can_occlude"] = json!(true);
        let facts: Facts = serde_json::from_value(json!({
            "blocks": {
                "minecraft:iron_block": block(&plain, json!({}), json!({"": solid.clone()})),
                "m:cannon": block(&["m.Cannon", plain[0]], json!({}), json!({"": open})),
                "m:plate": block(&["m.Plate", plain[0]], json!({}), json!({"": solid})),
            },
            "items": {}
        }))
        .unwrap();
        let cannon = facts.template("m:cannon", &|_| true).unwrap();
        assert_eq!(cannon.template, "minecraft:iron_block");
        assert_eq!(cannon.properties.get("occludes"), Some(&json!(false)));
        let plate = facts.template("m:plate", &|_| true).unwrap();
        assert_eq!(plate.properties.get("occludes"), None);
    }

    /// A rarity or map colour a mod adds past vanilla's would make the game refuse the whole mod:
    /// the template's stands in, and the report says so.
    #[test]
    fn values_past_vanillas_keep_the_templates() {
        let cube = json!([[0, 0, 0, 1, 1, 1]]);
        let plain = ["net.minecraft.world.level.block.Block"];
        let mut gas = state(0.0, "stone", cube.clone());
        gas["map_color_id"] = json!(63);
        let facts: Facts = serde_json::from_value(json!({
            "blocks": {
                "minecraft:stone": block(&plain, json!({}), json!({"": state(1.5, "stone", cube)})),
                "m:gas": block(&["m.Gas", plain[0]], json!({}), json!({"": gas})),
            },
            "items": {"m:relic": {"class": "net.minecraft.world.item.Item", "max_stack_size": 1,
                "rarity": "m:masterwork"}}
        }))
        .unwrap();
        let gas = facts.template("m:gas", &|_| true).unwrap();
        assert!(!gas.properties.contains_key("map_color"));
        assert!(
            gas.differences.iter().any(|d| d.contains("map colour 63")),
            "{:?}",
            gas.differences
        );
        let (relic, differences) = facts.item("m:relic", &[]).unwrap();
        assert_eq!(relic.get("rarity"), None);
        assert!(
            differences.iter().any(|d| d.contains("m:masterwork")),
            "{differences:?}"
        );
    }

    /// 1.21.1's `minecraft:food` held what 26.3 splits into `food`, `consumable` and `use_remainder`
    /// (as 26.3's golden apple and mushroom stew have them).
    #[test]
    fn food_becomes_26_3s_food_and_consumable() {
        let food = |f: Value| {
            json!({"class": "net.minecraft.world.item.Item", "max_stack_size": 16,
            "rarity": "common", "food": {"nutrition": f["nutrition"], "saturation": f["saturation"]},
            "components": {"minecraft:food": f}})
        };
        let facts: Facts = serde_json::from_value(json!({"blocks": {}, "items": {
            "m:bread": food(json!({"nutrition": 2, "saturation": 1.6})),
            "m:stew": food(json!({"nutrition": 6, "saturation": 7.2, "can_always_eat": true, "eat_seconds": 0.8,
                "using_converts_to": {"id": "minecraft:bowl", "count": 1},
                "effects": [
                    {"effect": {"id": "minecraft:hunger", "duration": 600, "show_icon": true,
                        "neoforge:cures": ["milk"]}, "probability": 0.3},
                    {"effect": {"id": "minecraft:regeneration", "amplifier": 1, "duration": 100}, "probability": 1.0},
                    {"effect": {"id": "m:cozy", "duration": 100}, "probability": 1.0}]})),
        }}))
        .unwrap();
        let (bread, _) = facts.item("m:bread", &[]).unwrap();
        assert_eq!(
            bread["components"]["minecraft:food"],
            json!({"nutrition": 2, "saturation": 1.6})
        );
        assert_eq!(bread["components"]["minecraft:consumable"], json!({}));
        assert_eq!(bread["components"].get("minecraft:use_remainder"), None);
        let (stew, differences) = facts.item("m:stew", &[]).unwrap();
        let c = &stew["components"];
        assert_eq!(
            c["minecraft:food"],
            json!({"nutrition": 6, "saturation": 7.2, "can_always_eat": true})
        );
        assert_eq!(
            c["minecraft:consumable"],
            json!({"consume_seconds": 0.8, "on_consume_effects": [
                {"type": "minecraft:apply_effects", "probability": 0.3,
                    "effects": [{"id": "minecraft:hunger", "duration": 600, "show_icon": true}]},
                {"type": "minecraft:apply_effects",
                    "effects": [{"id": "minecraft:regeneration", "amplifier": 1, "duration": 100}]}]})
        );
        assert_eq!(
            c["minecraft:use_remainder"],
            json!({"id": "minecraft:bowl", "count": 1})
        );
        assert!(
            differences.iter().any(|d| d.contains("m:cozy")),
            "{differences:?}"
        );
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
        let (sword, _) = facts.item("m:sword", &["m"]).unwrap();
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
        let (boots, _) = facts.item("m:boots", &["m"]).unwrap();
        assert_eq!(boots["template"], "minecraft:iron_boots");
        assert_eq!(boots["components"]["!minecraft:enchantable"], json!({}));
        assert_eq!(boots["components"]["!minecraft:repairable"], json!({}));
        // Facts without components (exported before they were) keep the plain template.
        let (gem, _) = facts.item("m:gem", &["m"]).unwrap();
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
        let (pie, differences) = facts.item("m:pie", &["m"]).unwrap();
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
        let (vest, _) = facts.item("m:vest", &["m"]).unwrap();
        assert_eq!(vest["template"], "minecraft:iron_chestplate");
        assert_eq!(vest["rarity"], "rare");
        assert!(vest.get("max_stack_size").is_none());
        assert_eq!(vest["components"]["minecraft:max_damage"], 500);
        assert_eq!(
            vest["components"]["minecraft:equippable"]["asset_id"],
            "m:hazmat"
        );
        let (gadget, _) = facts.item("m:gadget", &["m"]).unwrap();
        assert_eq!(gadget["template"], "minecraft:stick");
        assert_eq!(gadget["max_stack_size"], 16);
        assert_eq!(gadget["rarity"], "epic");
        assert_eq!(gadget["components"], json!({"!minecraft:cooking_fuel": {}}));
    }

    #[test]
    fn block_items_keep_their_numbers() {
        let facts = current_facts();
        let item = facts
            .block_item("m:block", "minecraft:stone", &["m"])
            .unwrap();
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
    fn armour_materials_become_equipment_assets() {
        let armor: Armor = serde_json::from_value(json!({"slot": "legs", "material": "m:ruby",
            "equip_sound": "minecraft:item.armor.equip_diamond", "layers": [
                {"texture": "m:textures/models/armor/ruby_layer_1.png",
                 "texture_legs": "m:textures/models/armor/ruby_layer_2.png", "dyeable": false},
                {"texture": "m:textures/models/armor/ruby_overlay_layer_1.png",
                 "texture_legs": "m:textures/models/armor/ruby_overlay_layer_2.png", "dyeable": true}]}))
        .unwrap();
        let (asset, copies) = armor.equipment().unwrap();
        assert_eq!(
            asset["layers"]["humanoid_leggings"],
            json!([{"texture": "m:ruby"}, {"texture": "m:ruby_overlay", "dyeable": {}}])
        );
        assert_eq!(
            copies[2],
            (
                "assets/m/textures/models/armor/ruby_layer_2.png".to_owned(),
                "assets/m/textures/entity/equipment/humanoid_leggings/ruby.png".to_owned()
            )
        );
        let mut facts: Facts = serde_json::from_value(
            json!({"minecraft": "1.21.1", "blocks": {}, "items": {"m:ruby_leggings": {
            "class": "net.minecraft.world.item.ArmorItem", "max_stack_size": 1, "rarity": "common",
            "components": {"minecraft:max_damage": 300}, "armor": armor_json()}}}),
        )
        .unwrap();
        facts.vanilla_items = serde_json::from_value(json!({"minecraft:iron_leggings": {
            "minecraft:equippable": {"asset_id": "minecraft:iron", "equip_sound": "minecraft:item.armor.equip_iron", "slot": "legs"}}}))
        .unwrap();
        let (decl, _) = facts.item("m:ruby_leggings", &["m"]).unwrap();
        assert_eq!(
            decl["components"]["minecraft:equippable"],
            json!({"asset_id": "m:ruby", "equip_sound": "minecraft:item.armor.equip_diamond", "slot": "legs"})
        );
        // Layers outside models/armor/ have no 26.3 place to go.
        let odd: Armor =
            serde_json::from_value(json!({"slot": "head", "material": "m:odd", "layers": [
            {"texture": "m:textures/odd.png", "texture_legs": "m:textures/odd.png"}]}))
            .unwrap();
        assert!(odd.equipment().is_none());
    }

    #[test]
    fn armour_only_names_an_equipment_asset_that_exists() {
        let leggings = |material: &str, layers: Value| {
            json!({"class": "net.minecraft.world.item.ArmorItem", "max_stack_size": 1, "rarity": "common",
                "components": {"minecraft:max_damage": 300},
                "armor": {"slot": "legs", "material": material,
                    "equip_sound": "minecraft:item.armor.equip_diamond", "layers": layers}})
        };
        let odd = json!([{"texture": "m:textures/odd.png", "texture_legs": "m:textures/odd.png"}]);
        let mut facts: Facts =
            serde_json::from_value(json!({"minecraft": "1.21.1", "blocks": {}, "items": {
            "m:ruby": leggings("m:ruby", json!([])),
            "m:odd": leggings("m:odd", odd),
            "m:other": leggings("other:steel", json!([])),
            "m:unknown": leggings("minecraft:steel", json!([])),
            "m:diamond": leggings("minecraft:diamond", json!([]))}}))
            .unwrap();
        facts.vanilla_items = serde_json::from_value(json!({
            "minecraft:iron_leggings": {"minecraft:equippable": {"asset_id": "minecraft:iron", "slot": "legs"}},
            "minecraft:diamond_leggings": {"minecraft:equippable": {"asset_id": "minecraft:diamond", "slot": "legs"}}}))
        .unwrap();
        let asset = |id: &str| {
            let (decl, differences) = facts.item(id, &["m"]).unwrap();
            let asset = decl["components"]["minecraft:equippable"]["asset_id"].clone();
            (asset, !differences.is_empty())
        };
        assert_eq!(asset("m:ruby"), (json!("m:ruby"), false));
        assert_eq!(asset("m:diamond"), (json!("minecraft:diamond"), false));
        // 1.21 leggings stand on iron_leggings, which keep their own asset.
        for id in ["m:odd", "m:other", "m:unknown"] {
            assert_eq!(asset(id), (json!("minecraft:iron"), true), "{id}");
        }
        // Without the game's tables the template's own component stays whole.
        facts.vanilla_items.clear();
        let (decl, _) = facts.item("m:odd", &["m"]).unwrap();
        assert!(decl["components"].get("minecraft:equippable").is_none());
    }

    fn armor_json() -> Value {
        json!({"slot": "legs", "material": "m:ruby", "equip_sound": "minecraft:item.armor.equip_diamond"})
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
