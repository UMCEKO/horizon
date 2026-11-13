// game-module-cfg: feature java-mods
//! NeoForge's global loot modifiers on native rolls: `CommonHooks.modifyLoot` runs in Java on the rolled stacks (`loot_modify`).

use horizon_core::random::RandomState;
use horizon_java_host::wire::{In, Out};
use horizon_world::loot::{LootContext, LootStack};
use horizon_world::registry::BlockStateId;
use serde_json::Value;

const OVERWORLD: &str = "minecraft:overworld";
const LEGACY: u8 = 1;
const XOROSHIRO: u8 = 2;

/// Whether a loaded modifier can apply to `table`; an inline table (`""`) only to an "any" modifier.
pub(in crate::game) fn loot_wanted(table: &str) -> bool {
    super::java_crafting::craft_facts().is_some_and(|f| f.modifiers_for(table))
}

/// `CommonHooks.modifyLoot` on a roll's stacks; the roll stays as it was when Java threw.
pub(in crate::game) fn loot_rolled(
    table: &str,
    ctx: &LootContext<'_>,
    tool: Option<&LootStack>,
    random: &mut Option<RandomState>,
    items: &mut Vec<LootStack>,
) {
    let (Some(host), Some(state)) = (horizon_java_host::get(), *random) else {
        return;
    };
    let sent: Vec<Value> = items.iter().map(LootStack::to_codec_json).collect();
    let request = request(table, ctx, tool, state, &sent, java_state);
    let Some(reply) =
        horizon_java_host::bridge::without_active(|| host.request("loot_modify", &request.0))
    else {
        return;
    };
    if reply.is_empty() {
        return;
    }
    match read_reply(&reply, &sent, items) {
        Some((after, stacks)) => {
            *random = Some(after);
            *items = stacks;
        }
        None => log::error!("Horizon: a malformed loot_modify reply for {table}; kept the roll"),
    }
}

fn java_state(state: BlockStateId) -> i32 {
    horizon_java_host::bridge::STATES.get().map_or(-1, |m| {
        m.to_java
            .get(state.0 as usize)
            .copied()
            .unwrap_or(horizon_java_host::bridge::UNMAPPED)
    })
}

fn uuid(out: &mut Out, id: u128) {
    out.long((id >> 64) as i64);
    out.long(id as i64);
}

/// The `loot_modify` request: see `B/Loot.java` for the fields' order.
fn request(
    table: &str,
    ctx: &LootContext<'_>,
    tool: Option<&LootStack>,
    random: RandomState,
    stacks: &[Value],
    to_java: impl Fn(BlockStateId) -> i32,
) -> Out {
    let place = ctx.place.as_deref();
    let mut out = Out::default();
    out.text(table);
    out.text(
        place
            .and_then(|p| p.level.as_ref())
            .map_or(OVERWORLD, |l| l.dimension.as_str()),
    );
    match random {
        RandomState::Legacy(state) => {
            out.0.push(LEGACY);
            out.long(state as i64);
        }
        RandomState::Xoroshiro(lo, hi) => {
            out.0.push(XOROSHIRO);
            out.long(lo as i64);
            out.long(hi as i64);
        }
    }
    let origin = place.and_then(|p| p.origin);
    out.bool(origin.is_some());
    for v in origin.unwrap_or_default() {
        out.double(v);
    }
    let (id, this) = place.and_then(|p| p.this_entity).unwrap_or((-1, 0));
    out.int(id);
    uuid(&mut out, this);
    let killer = place.and_then(|p| p.killer);
    out.bool(killer.is_some());
    uuid(&mut out, killer.unwrap_or(0));
    out.bool(ctx.damage.is_some());
    if let Some(damage) = ctx.damage {
        out.text(&damage.kind);
    }
    out.text(&tool.map_or_else(String::new, |t| t.to_codec_json().to_string()));
    out.int(
        place
            .and_then(|p| p.block_state)
            .map_or(-1, |s| to_java(s).max(-1)),
    );
    let block_entity = place
        .and_then(|p| p.block_entity.clone())
        .and_then(super::java_mods::write_tag)
        .unwrap_or_default();
    out.bytes(&block_entity);
    out.bool(false);
    out.float(ctx.luck);
    out.int(stacks.len() as i32);
    for stack in stacks {
        out.text(&stack.to_string());
    }
    out
}

/// The random after the modifiers and their stacks: one the modifiers left as sent keeps its native form.
fn read_reply(
    reply: &[u8],
    sent: &[Value],
    original: &[LootStack],
) -> Option<(RandomState, Vec<LootStack>)> {
    let mut input = In(reply);
    let random = match input.byte()? {
        LEGACY => RandomState::Legacy(input.long()? as u64),
        XOROSHIRO => RandomState::Xoroshiro(input.long()? as u64, input.long()? as u64),
        _ => return None,
    };
    let n = usize::try_from(input.int()?).ok()?;
    let mut stacks = Vec::with_capacity(n);
    for i in 0..n {
        let text = input.text()?;
        let Ok(json) = serde_json::from_str::<Value>(text) else {
            log::warn!("Horizon: a loot modifier's stack is not JSON, dropped: {text}");
            continue;
        };
        if sent.get(i) == Some(&json)
            && let Some(kept) = original.get(i)
        {
            stacks.push(kept.clone());
            continue;
        }
        match LootStack::from_codec_json(&json) {
            Some(stack) => stacks.push(stack),
            None => log::warn!("Horizon: a loot modifier's stack does not parse, dropped: {text}"),
        }
    }
    Some((random, stacks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_world::loot::{ExplorationMap, LootLevel, LootPlace};
    use std::sync::Arc;

    fn stack(json: Value) -> LootStack {
        LootStack::from_codec_json(&json).unwrap()
    }

    #[test]
    fn a_request_carries_the_rolls_parameters_in_the_wire_order() {
        let player: u128 = 0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10;
        let place = LootPlace {
            origin: Some([1.5, 64.0, -2.5]),
            level: Some(LootLevel {
                dimension: "minecraft:the_nether".into(),
                ..LootLevel::default()
            }),
            this_entity: Some((42, player)),
            block_state: Some(BlockStateId(9)),
            ..LootPlace::default()
        };
        let ctx = LootContext {
            entity: None,
            damage: None,
            player: None,
            tool: None,
            block: None,
            luck: 1.5,
            biome: None,
            seed: 0,
            place: Some(Arc::new(place)),
        };
        let tool = stack(serde_json::json!({"id": "minecraft:iron_pickaxe", "count": 1}));
        let stacks = [
            serde_json::json!({"id": "minecraft:sand", "count": 1}),
            serde_json::json!({"id": "minecraft:stick", "count": 3}),
        ];
        let got = request(
            "minecraft:blocks/sand",
            &ctx,
            Some(&tool),
            RandomState::Legacy(12345),
            &stacks,
            |s| s.0 as i32 + 100,
        );

        let mut want = Out::default();
        want.text("minecraft:blocks/sand");
        want.text("minecraft:the_nether");
        want.0.push(1);
        want.long(12345);
        want.bool(true);
        want.double(1.5);
        want.double(64.0);
        want.double(-2.5);
        want.int(42);
        want.long(0x0102_0304_0506_0708);
        want.long(0x090a_0b0c_0d0e_0f10);
        want.bool(false);
        want.long(0);
        want.long(0);
        want.bool(false);
        want.text(&serde_json::json!({"id": "minecraft:iron_pickaxe", "count": 1}).to_string());
        want.int(109);
        want.int(0);
        want.bool(false);
        want.float(1.5);
        want.int(2);
        for stack in &stacks {
            want.text(&stack.to_string());
        }
        assert_eq!(got.0, want.0);
    }

    #[test]
    fn a_stack_returned_unchanged_keeps_its_native_form_and_a_new_one_parses() {
        let mut map = stack(serde_json::json!({"id": "minecraft:map", "count": 1}));
        map.exploration = Some(Box::new(ExplorationMap {
            destination: "minecraft:on_treasure_maps".into(),
            decoration: "minecraft:red_x".into(),
            zoom: 1,
            search_radius: 50,
            skip_existing_chunks: false,
        }));
        let sent = vec![map.to_codec_json()];
        let mut reply = Out::default();
        reply.0.push(2);
        reply.long(7);
        reply.long(-8);
        reply.int(2);
        reply.text(&sent[0].to_string());
        reply.text(r#"{"id":"minecraft:gold_nugget","count":2}"#);

        let (random, stacks) = read_reply(&reply.0, &sent, std::slice::from_ref(&map)).unwrap();
        assert_eq!(random, RandomState::Xoroshiro(7, (-8i64) as u64));
        assert_eq!(stacks.len(), 2);
        assert_eq!(stacks[0], map);
        assert_eq!(stacks[1].item, "minecraft:gold_nugget");
        assert_eq!(stacks[1].count, 2);
        assert!(stacks[1].exploration.is_none());
    }
}
