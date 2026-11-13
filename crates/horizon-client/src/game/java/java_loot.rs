// game-module-cfg: feature java-mods
//! A mod's loot functions and conditions, and the tool's enchantments, as Java gives them to the tables Horizon rolls.

use super::java_crafting::{CraftFacts, craft_facts};
use super::java_mods;
use super::*;
use horizon_java_host::wire::{In, Out};
use horizon_world::loot::{LootContext, LootStack};
use serde_json::{Value, json};
use std::cell::Cell;

impl Game<'_> {
    /// A copy of the tool with Java's enchantment levels, for one roll; None unless item code or a mod's event answers them.
    pub(super) fn java_tool_for_loot(&mut self, tool: &ItemStack) -> Option<ItemStack> {
        if !craft_facts()?.tool_levels_from_java(&tool.item) {
            return None;
        }
        let nbt = super::java_mods::write_tag(horizon_items::item_nbt::item_to_nbt(tool))?;
        let key = self.java_level_key();
        let time = self.world.game_time;
        let levels = self.java_call_with(
            "tool_enchantments",
            |out| {
                out.int(key as i32);
                out.long(time);
                out.bytes(&nbt);
            },
            read_levels,
        )?;
        let mut swapped = tool.clone();
        swapped.data_mut().enchantments = levels;
        Some(swapped)
    }
}

fn read_levels(input: &mut In<'_>) -> Option<Vec<(Arc<str>, u32)>> {
    let mut levels = Vec::new();
    for _ in 0..input.int()? {
        let id = input.text()?;
        levels.push((Arc::from(id), input.int()? as u32));
    }
    Some(levels)
}

thread_local! {
    static JAVA_LOOT_LIVE: Cell<bool> = const { Cell::new(false) };
}

/// While alive, loot kinds met in a roll use the `LootParams` Java's own `getDrops` was asked with.
pub(in crate::game) struct LiveLoot(bool);

impl LiveLoot {
    pub(in crate::game) fn enter() -> Self {
        Self(JAVA_LOOT_LIVE.replace(true))
    }
}

impl Drop for LiveLoot {
    fn drop(&mut self) {
        JAVA_LOOT_LIVE.set(self.0);
    }
}

fn is_live() -> bool {
    JAVA_LOOT_LIVE.get()
}

enum Target<'a> {
    Function(&'a LootStack),
    Condition,
}

fn without_conditions(data: &Value) -> Value {
    let mut data = data.clone();
    if let Some(object) = data.as_object_mut() {
        object.remove("conditions");
        object.remove("condition");
    }
    data
}

fn tool_json(item: &str, enchantments: &[(String, i32)]) -> String {
    let mut stack = json!({ "id": item, "count": 1 });
    if !enchantments.is_empty() {
        let levels = enchantments
            .iter()
            .map(|(id, level)| (id.clone(), Value::from(*level)))
            .collect::<serde_json::Map<_, _>>();
        stack["components"] = json!({ "minecraft:enchantments": levels });
    }
    stack.to_string()
}

/// The request for a kind a Java mod owns; None for any other kind.
fn request(
    facts: &CraftFacts,
    kind: &str,
    data: &Value,
    ctx: &LootContext<'_>,
    live: bool,
    target: Target<'_>,
) -> Option<Vec<u8>> {
    let owned = match target {
        Target::Function(_) => facts.owns_function(kind),
        Target::Condition => facts.owns_condition(kind),
    };
    if !owned {
        return None;
    }
    let mut out = Out::default();
    out.text(kind);
    out.text(&without_conditions(data).to_string());
    out.bool(live);
    let place = ctx.place.as_deref();
    let level = place.and_then(|p| p.level.as_ref());
    out.text(level.map_or("minecraft:overworld", |l| l.dimension.as_str()));
    out.long(level.map_or(0, |l| l.game_time));
    let origin = place.and_then(|p| p.origin);
    out.bool(origin.is_some());
    for v in origin.unwrap_or_default() {
        out.double(v);
    }
    let this = place.and_then(|p| p.this_entity);
    out.bool(this.is_some());
    if let Some((id, uuid)) = this {
        out.int(id);
        out.long((uuid >> 64) as i64);
        out.long(uuid as i64);
    }
    let killer = place.and_then(|p| p.killer);
    out.bool(killer.is_some());
    if let Some(uuid) = killer {
        out.long((uuid >> 64) as i64);
        out.long(uuid as i64);
    }
    out.text(
        &ctx.tool
            .as_ref()
            .map(|t| tool_json(&t.item, &t.enchantments))
            .unwrap_or_default(),
    );
    out.text(ctx.block.unwrap_or_default());
    let block_entity = place
        .and_then(|p| p.block_entity.clone())
        .and_then(java_mods::write_tag)
        .unwrap_or_default();
    out.int(block_entity.len() as i32);
    out.0.extend_from_slice(&block_entity);
    out.float(ctx.luck);
    out.long(ctx.seed);
    if let Target::Function(stack) = target {
        out.text(&stack.to_codec_json().to_string());
    }
    Some(out.0)
}

/// A mod's loot function changes the stack as its class does in Java.
pub(in crate::game) fn hook_function(
    kind: &str,
    data: &Value,
    ctx: &LootContext<'_>,
    stack: &mut LootStack,
) {
    let (Some(host), Some(facts)) = (horizon_java_host::get(), craft_facts()) else {
        return;
    };
    let Some(bytes) = request(facts, kind, data, ctx, is_live(), Target::Function(stack)) else {
        return;
    };
    let Some(reply) =
        horizon_java_host::bridge::without_active(|| host.request("loot_function", &bytes))
    else {
        return;
    };
    let Some(text) = In(&reply).text().filter(|t| !t.is_empty()) else {
        return;
    };
    let Ok(json) = serde_json::from_str::<Value>(text) else {
        log::error!("Horizon: loot function {kind} answered malformed JSON");
        return;
    };
    match LootStack::from_codec_json(&json) {
        Some(changed) => *stack = changed,
        None if json["count"] == 0 => stack.count = 0,
        None => log::error!("Horizon: loot function {kind} answered a stack that does not parse"),
    }
}

/// A mod's loot condition holds as its class decides in Java.
pub(in crate::game) fn hook_condition(
    kind: &str,
    data: &Value,
    ctx: &LootContext<'_>,
    holds: &mut bool,
) {
    let (Some(host), Some(facts)) = (horizon_java_host::get(), craft_facts()) else {
        return;
    };
    let Some(bytes) = request(facts, kind, data, ctx, is_live(), Target::Condition) else {
        return;
    };
    let reply =
        horizon_java_host::bridge::without_active(|| host.request("loot_condition", &bytes));
    match reply.as_deref().and_then(|r| In(r).byte()) {
        Some(0) => *holds = false,
        Some(1) => *holds = true,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_reply_reads_in_order() {
        let mut out = Out::default();
        out.int(2);
        out.text("minecraft:fortune");
        out.int(3);
        out.text("minecraft:silk_touch");
        out.int(1);
        let levels = read_levels(&mut In(&out.0)).unwrap();
        assert_eq!(
            levels,
            vec![
                (Arc::from("minecraft:fortune"), 3),
                (Arc::from("minecraft:silk_touch"), 1)
            ]
        );
    }

    fn ctx() -> LootContext<'static> {
        LootContext {
            entity: None,
            damage: None,
            player: None,
            tool: None,
            block: None,
            luck: 0.0,
            biome: None,
            seed: 7,
            place: None,
        }
    }

    #[test]
    fn a_kind_no_java_mod_owns_sends_nothing() {
        let facts = CraftFacts::owning(&["t:stamp"], &["t:always"]);
        let stack = horizon_world::loot::LootStack::from_codec_json(
            &json!({"id": "minecraft:stone", "count": 1}),
        )
        .unwrap();
        let data = json!({});
        assert!(
            request(
                &facts,
                "t:other",
                &data,
                &ctx(),
                false,
                Target::Function(&stack)
            )
            .is_none()
        );
        assert!(request(&facts, "t:stamp", &data, &ctx(), false, Target::Condition).is_none());
        assert!(
            request(
                &facts,
                "t:always",
                &data,
                &ctx(),
                false,
                Target::Function(&stack)
            )
            .is_none()
        );
        assert!(
            request(
                &facts,
                "t:stamp",
                &data,
                &ctx(),
                false,
                Target::Function(&stack)
            )
            .is_some()
        );
        assert!(request(&facts, "t:always", &data, &ctx(), false, Target::Condition).is_some());
    }

    #[test]
    fn the_request_is_the_wire_java_reads() {
        let facts = CraftFacts::owning(&[], &["t:always"]);
        let data = json!({"type": "t:always", "condition": {"x": 1}});
        let bytes = request(&facts, "t:always", &data, &ctx(), true, Target::Condition).unwrap();
        let mut expected = Out::default();
        expected.text("t:always");
        expected.text(r#"{"type":"t:always"}"#);
        expected.bool(true);
        expected.text("minecraft:overworld");
        expected.long(0);
        expected.bool(false);
        expected.bool(false);
        expected.bool(false);
        expected.text("");
        expected.text("");
        expected.int(0);
        expected.float(0.0);
        expected.long(7);
        assert_eq!(bytes, expected.0);
    }

    #[test]
    fn the_live_flag_resets_after_a_panic_in_the_roll() {
        assert!(!is_live());
        let unwound = std::panic::catch_unwind(|| {
            let _live = LiveLoot::enter();
            assert!(is_live());
            panic!("the roll failed");
        });
        assert!(unwound.is_err());
        assert!(!is_live());
    }
}
