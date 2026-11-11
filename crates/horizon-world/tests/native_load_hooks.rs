use horizon_core::random::{LegacyRandom, RandomSource, RandomState};
use horizon_world::{
    item_defaults, loot,
    native_load_context::{DataLoadOrigin, LoadContext, LoadInputs},
    native_load_hooks::{self, NativeLoadHooks},
};
use serde_json::{Map, Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};

static ROLLED: AtomicUsize = AtomicUsize::new(0);
const DUNGEON: &str = "minecraft:chests/simple_dungeon";

/// Vanilla's `Util.shuffle`.
fn shuffle<T>(list: &mut [T], random: &mut LegacyRandom) {
    for i in (2..=list.len()).rev() {
        list.swap(i - 1, random.next_int_bound(i as i32).unwrap() as usize);
    }
}

/// Vanilla's `LootTable.shuffleAndSplitItems`.
fn shuffle_and_split(result: &mut Vec<loot::LootStack>, slots: usize, random: &mut LegacyRandom) {
    let mut splittable = Vec::new();
    result.retain(|s| {
        if s.is_empty() {
            false
        } else if s.count > 1 {
            splittable.push(s.clone());
            false
        } else {
            true
        }
    });
    let between = |random: &mut LegacyRandom, min: i32, max: i32| {
        if min >= max {
            min
        } else {
            random.next_int_bound(max - min + 1).unwrap() + min
        }
    };
    while slots > result.len() + splittable.len() && !splittable.is_empty() {
        let index = between(random, 0, splittable.len() as i32 - 1) as usize;
        let mut stack = splittable.remove(index);
        let remove = between(random, 1, stack.count / 2).min(stack.count);
        let mut copy = stack.clone();
        copy.count = remove;
        stack.count -= remove;
        if stack.count > 1 && random.next_boolean() {
            splittable.push(stack);
        } else {
            result.push(stack);
        }
        if copy.count > 1 && random.next_boolean() {
            splittable.push(copy);
        } else {
            result.push(copy);
        }
    }
    result.append(&mut splittable);
    shuffle(result, random);
}

/// Loot tables for the loot hook: the dungeon's and the same pools under an id it doesn't want.
fn hooked_loot(root: &std::path::Path, context: &LoadContext) -> loot::LootData {
    let dir = root.join("data/minecraft/loot_table/chests");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(root.join("items.json"), r#"{"probe:token":{}}"#).unwrap();
    let count = json!({"function": "minecraft:set_count", "count": {"type": "minecraft:uniform", "min": 1, "max": 8}});
    let table = json!({"pools": [{"rolls": 4, "entries": [
        {"type": "minecraft:item", "name": "minecraft:bone", "weight": 2, "functions": [count]},
        {"type": "minecraft:item", "name": "minecraft:string", "functions": [count]}
    ]}]});
    for name in ["simple_dungeon", "other"] {
        std::fs::write(dir.join(format!("{name}.json")), table.to_string()).unwrap();
    }
    let item_data =
        item_defaults::ItemData::load(root, context.for_load(DataLoadOrigin::ClientPreload, []))
            .unwrap();
    loot::LootData::load(root, &loot::PackOverlay::NONE, item_data.defaults()).unwrap()
}

#[test]
fn load_hooks_are_noops_before_install_and_use_first_callbacks_after() {
    let root = std::env::temp_dir().join(format!("horizon-load-hooks-{}", std::process::id()));
    let context = LoadContext::new(
        DataLoadOrigin::ClientPreload,
        std::sync::Arc::new(LoadInputs::new(Default::default(), root.join("settings"))),
        [],
    );
    let server_context = context.for_load(DataLoadOrigin::ServerDataLoad, []);
    let loot_root = root.join("hooked");
    let unhooked = hooked_loot(&loot_root, &context);
    let mut after_roll = LegacyRandom::new(7);
    let plain_roll = unhooked.roll(DUNGEON, &mut after_roll, 0.0, None, None);
    let other = "minecraft:chests/other";
    let plain_other = unhooked.fill_with(other, &mut LegacyRandom::new(7), 27, None);
    let mut table = json!([]);
    let mut components = Map::new();
    loot::native_loot_table_load(&server_context, "table", &mut table);
    item_defaults::native_modify_default_components(&context, "item", &mut components);
    assert_eq!(table, json!([]));
    assert!(components.is_empty());

    let hooks = || NativeLoadHooks {
        loot_table_load: |context, id, table| {
            assert_eq!(context.origin(), DataLoadOrigin::ServerDataLoad);
            table.as_array_mut().unwrap().push(id.into());
        },
        modify_default_components: |context, item, components| {
            assert_eq!(context.origin(), DataLoadOrigin::ClientPreload);
            components.insert("item".into(), Value::String(item.into()));
        },
        loot_function: |_, _, _, stack| stack.count += 1,
        loot_condition: |kind, _, _, holds| *holds = kind == "mymod:held",
        loot_wanted: |table| table == DUNGEON,
        loot_rolled: |table, ctx, tool, state, items| {
            assert_eq!((table, ctx.seed, tool), (DUNGEON, 0, None));
            ROLLED.fetch_add(1, Ordering::Relaxed);
            items.extend(loot::LootStack::from_codec_json(
                &json!({"id": "minecraft:diamond"}),
            ));
            let Some(RandomState::Legacy(seed)) = *state else {
                panic!("a chest's random is legacy")
            };
            let mut random = LegacyRandom::from_state(seed);
            random.next_int();
            *state = random.snapshot();
        },
    };
    assert!(native_load_hooks::install(hooks()).is_ok());
    assert!(
        native_load_hooks::install(NativeLoadHooks {
            loot_table_load: |_, _, table| *table = json!("second"),
            modify_default_components: |_, _, components| components.clear(),
            loot_function: |_, _, _, stack| stack.count = 0,
            loot_condition: |_, _, _, holds| *holds = false,
            loot_wanted: |_| true,
            loot_rolled: |_, _, _, _, items| items.clear(),
        })
        .is_err()
    );
    let hooked = hooked_loot(&loot_root, &context);
    loot::native_loot_table_load(&server_context, "table", &mut table);
    item_defaults::native_modify_default_components(&context, "item", &mut components);
    assert_eq!(table, json!(["table"]));
    assert_eq!(components["item"], json!("item"));

    // Client/shared loads keep raw tables; each server load gets exactly one
    // callback per table, even when the raw map is already cached.
    let dir = root.join("data/minecraft/loot_table");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(root.join("items.json"), r#"{"probe:token":{}}"#).unwrap();
    std::fs::write(dir.join("test.json"), "[]").unwrap();
    let item_data = item_defaults::ItemData::load(&root, context).unwrap();
    // The server loot load owns its context even when the item data came from a client preload.
    assert_eq!(
        item_data.defaults().entries()["probe:token"]["item"],
        "probe:token"
    );
    let raw = loot::shared_tables(&root, &loot::PackOverlay::NONE).unwrap();
    assert_eq!(raw["minecraft:test"], json!([]));
    for _ in 0..2 {
        let mut server =
            loot::LootData::load(&root, &loot::PackOverlay::NONE, item_data.defaults()).unwrap();
        server.apply_native_table_loads(&server_context);
        assert_eq!(server.tables()["minecraft:test"], json!(["minecraft:test"]));
        assert_eq!(raw["minecraft:test"], json!([]));
    }

    // The hook sees the rolled stacks before the slot shuffle and moves the chest's random on.
    let slots = hooked.fill_with(DUNGEON, &mut LegacyRandom::new(7), 27, None);
    assert_eq!(ROLLED.load(Ordering::Relaxed), 1);
    let mut items = plain_roll;
    items.extend(loot::LootStack::from_codec_json(
        &json!({"id": "minecraft:diamond"}),
    ));
    let mut by_hand = after_roll;
    by_hand.next_int();
    let mut available: Vec<usize> = (0..27).collect();
    shuffle(&mut available, &mut by_hand);
    shuffle_and_split(&mut items, available.len(), &mut by_hand);
    let mut expected = vec![None; 27];
    for stack in items {
        let Some(slot) = available.pop() else { break };
        expected[slot] = Some(stack);
    }
    assert_eq!(slots, expected);
    assert!(
        slots
            .iter()
            .flatten()
            .any(|s| s.item == "minecraft:diamond")
    );
    // A table the hook doesn't want rolls as with no hook, and the hook is never called.
    assert_eq!(
        hooked.fill_with(other, &mut LegacyRandom::new(7), 27, None),
        plain_other
    );
    assert_eq!(ROLLED.load(Ordering::Relaxed), 1);
    std::fs::remove_dir_all(&root).unwrap();

    let ctx = loot::LootContext {
        entity: None,
        damage: None,
        player: None,
        tool: None,
        block: None,
        luck: 0.0,
        biome: None,
        seed: 0,
        place: None,
    };
    for (kind, expected) in [("mymod:held", true), ("mymod:other", false)] {
        let mut holds = !expected;
        loot::native_loot_condition(kind, &json!({}), &ctx, &mut holds);
        assert_eq!(holds, expected, "{kind}");
    }
}
