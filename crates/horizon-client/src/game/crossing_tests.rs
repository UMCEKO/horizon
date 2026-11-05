//! The crossing points of the native entity loops (SPEC-1a), run on a server `Game` over a flat world.
use super::crossing::CrossPoint;
use super::entity_life::{Kind, Removal};
use super::*;
use horizon_client::riding::Ent;
use std::cell::RefCell;
use std::rc::Rc;

fn root() -> Option<PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/horizon-client")
        .join(horizon_core::VANILLA_VERSION);
    root.join("entity-types.json").exists().then_some(root)
}

/// A server `Game` over a flat, lit world, with a player so its chunks tick; none without the vanilla data.
pub(super) fn flat_game() -> Option<Game<'static>> {
    let root = root()?;
    let mut options = Options::parse(
        [
            "--resources".to_owned(),
            root.to_string_lossy().into_owned(),
        ]
        .into_iter(),
    )
    .ok()??;
    options.execution_role = execution_role::ExecutionRole::IntegratedServer;
    options.dedicated = Some(Vec::new());
    options.server_thread = false;
    let settings = Settings::default();
    let loaded = load_resources(
        root.clone(),
        &root,
        &settings,
        Instant::now(),
        false,
        true,
        &Progress::new(),
        &mut |_, _| {},
    )
    .ok()?;
    let res: &'static Arc<Resources> = Box::leak(Box::new(loaded.resources));
    let gui = UiResources::server(&root).ok()?;
    let audio = Audio::silent(&root);
    let mut game = Game::new(res, loaded.pack, gui, options, settings, audio, None).ok()?;
    game.world = World::new(game.res.facts().air, 0);
    let plains = u32::from(game.res.biomes().id("minecraft:plains"));
    let registry = game.pack.assets.registry;
    let stone = registry.default_state("minecraft:stone").unwrap();
    for cx in -3..=3 {
        for cz in -3..=3 {
            let mut chunk = horizon_client::chunk::Chunk::empty(cx, cz);
            for section in &mut chunk.sections {
                Arc::make_mut(section).biomes = horizon_core::paletted::PalettedContainer::new(
                    horizon_client::chunk::BIOME_STRATEGY,
                    plains,
                );
            }
            chunk.lit = true;
            game.world.chunks.insert((cx, cz), Arc::new(chunk));
            for x in 0..16 {
                for z in 0..16 {
                    game.world.set(cx * 16 + x, 60, cz * 16 + z, stone).unwrap();
                }
            }
        }
    }
    game.mobs.clear();
    game.ui
        .rules
        .set("spawn_mobs", horizon_client::gamerules::Value::Bool(false));
    game.player = Some(
        horizon_client::motion::PlayerMotion::new(game.motion, Vec3::new(0.5, 61.0, 0.5)).unwrap(),
    );
    game.update_chunk_tickets();
    Some(game)
}

fn mob(game: &mut Game<'_>, x: f64, z: f64) -> u32 {
    let id = game.spawn_mob("zombie", [x, 61.0, z], true).unwrap();
    let m = game.mobs.iter_mut().find(|m| m.id == id).unwrap();
    m.no_ai = true;
    id
}

fn on_cross(game: &mut Game<'_>, hook: impl for<'g> FnMut(&mut Game<'g>, CrossPoint) + 'static) {
    game.test_cross = Some(Box::new(hook));
}

fn age(game: &Game<'_>, id: u32) -> Option<u32> {
    game.mobs.iter().find(|m| m.id == id).map(|m| m.age)
}

#[test]
fn a_crossing_sees_every_entity_in_place() {
    let Some(mut game) = flat_game() else { return };
    let ids = [
        mob(&mut game, 2.5, 2.5),
        mob(&mut game, 6.5, 2.5),
        mob(&mut game, 10.5, 2.5),
    ];
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    on_cross(&mut game, move |g, point| {
        if let CrossPoint::MobAfter(id) = point {
            let present: Vec<u32> = g.mobs.iter().map(|m| m.id).collect();
            log.borrow_mut()
                .push((id, present, g.lent().has(super::crossing::LentKinds::MOBS)));
        }
    });
    game.tick_creature_entities().unwrap();
    let seen = seen.borrow();
    assert_eq!(seen.len(), 3);
    for (id, present, lent) in seen.iter() {
        assert_eq!(present, &ids.to_vec(), "mob {id}");
        assert!(lent);
    }
    assert_eq!(game.mobs.len(), 3);
    assert!(!game.lent().has(super::crossing::LentKinds::MOBS));
}

#[test]
fn an_entity_added_in_a_crossing_ticks_from_the_next_tick() {
    let Some(mut game) = flat_game() else { return };
    let first = mob(&mut game, 2.5, 2.5);
    let added = Rc::new(RefCell::new(None));
    let slot = added.clone();
    on_cross(&mut game, move |g, point| {
        if point == CrossPoint::MobAfter(first) && slot.borrow().is_none() {
            *slot.borrow_mut() = Some(mob(g, 8.5, 8.5));
        }
    });
    game.tick_creature_entities().unwrap();
    let id = added.borrow().unwrap();
    assert_eq!(age(&game, first), Some(1));
    assert_eq!(age(&game, id), Some(0));
    game.tick_creature_entities().unwrap();
    assert_eq!(age(&game, id), Some(1));
}

#[test]
fn an_entity_removed_in_a_crossing_is_skipped() {
    let Some(mut game) = flat_game() else { return };
    let first = mob(&mut game, 2.5, 2.5);
    let second = mob(&mut game, 6.5, 2.5);
    let third = mob(&mut game, 10.5, 2.5);
    on_cross(&mut game, move |g, point| {
        if point == CrossPoint::MobAfter(first) {
            g.mobs.iter_mut().find(|m| m.id == second).unwrap().removed = true;
            g.note_touched(Some(second));
        }
    });
    game.tick_creature_entities().unwrap();
    assert_eq!(age(&game, second), None);
    assert_eq!(age(&game, first), Some(1));
    assert_eq!(age(&game, third), Some(1));
}

/// A's velocity when its turn ends, and after the whole tick, with or without the move.
fn velocity_of_a_over_a_tick(far: bool) -> Option<(Vec3, Vec3)> {
    let mut game = flat_game()?;
    let a = game.spawn_mob("zombie", [2.5, 61.0, 2.5], true).unwrap();
    game.spawn_mob("zombie", [2.6, 61.0, 2.5], true).unwrap();
    let at_turn_end = Rc::new(RefCell::new(None));
    let slot = at_turn_end.clone();
    on_cross(&mut game, move |g, point| {
        if point == CrossPoint::MobAfter(a) {
            let m = g.mobs.iter_mut().find(|m| m.id == a).unwrap();
            if far {
                m.position = Vec3::new(40.5, 61.0, 40.5);
            }
            *slot.borrow_mut() = Some(m.velocity);
            if far {
                g.note_touched(Some(a));
            }
        }
    });
    game.tick_creature_entities().unwrap();
    let end = game.mobs.iter().find(|m| m.id == a).unwrap().velocity;
    let turn_end = at_turn_end.borrow().unwrap();
    Some((turn_end, end))
}

#[test]
fn a_moved_mob_is_seen_moved_by_the_next() {
    let Some((turn_end, end)) = velocity_of_a_over_a_tick(false) else {
        return;
    };
    assert_ne!(turn_end, end, "the next mob pushes an overlapping one");
    let Some((turn_end, end)) = velocity_of_a_over_a_tick(true) else {
        return;
    };
    assert_eq!(turn_end, end, "the next mob saw it moved away");
}

#[test]
fn items_in_a_crossing_are_in_place_added_ones_wait_and_removed_ones_are_skipped() {
    let Some(mut game) = flat_game() else { return };
    let item = |game: &mut Game<'_>, x: f64| {
        game.next_entity += 1;
        game.dropped.push(ItemEntity {
            health: 5,
            fire_ticks: 0,
            id: game.next_entity,
            stack: ItemStack::new("minecraft:stick", 1),
            thrower: None,
            target: None,
            position: Vec3::new(x, 61.0, 0.5),
            previous: Vec3::new(x, 61.0, 0.5),
            velocity: Vec3::ZERO,
            on_ground: true,
            age: 0,
            pickup_delay: 10,
            bob_offset: 0.0,
            ticks: 0,
            removed: false,
            invulnerable: false,
        });
        game.next_entity
    };
    let (a, b, c) = (
        item(&mut game, 5.5),
        item(&mut game, 9.5),
        item(&mut game, 13.5),
    );
    let seen = Rc::new(RefCell::new(Vec::new()));
    let added = Rc::new(RefCell::new(None));
    let (log, slot) = (seen.clone(), added.clone());
    on_cross(&mut game, move |g, point| {
        if let CrossPoint::ItemAfter(id) = point {
            log.borrow_mut().push((id, g.dropped.len()));
            if id == a {
                *slot.borrow_mut() = Some(item(g, 3.5));
                g.dropped.iter_mut().find(|i| i.id == b).unwrap().removed = true;
                g.note_touched(Some(b));
            }
        }
    });
    game.tick_items();
    assert_eq!(*seen.borrow(), vec![(a, 3), (c, 4)]);
    let id = added.borrow().unwrap();
    let ticks =
        |game: &Game<'_>, id: u32| game.dropped.iter().find(|i| i.id == id).map(|i| i.ticks);
    assert_eq!(ticks(&game, b), None);
    assert_eq!(ticks(&game, id), Some(0));
    assert_eq!(ticks(&game, a), Some(1));
    game.test_cross = None;
    game.tick_items();
    assert_eq!(ticks(&game, id), Some(1));
}

#[test]
fn orbs_in_a_crossing_are_in_place_added_ones_wait_and_removed_ones_are_skipped() {
    let Some(mut game) = flat_game() else { return };
    let orb = |game: &mut Game<'_>, x: f64| {
        game.next_entity += 1;
        game.orbs.push(ExperienceOrb {
            id: game.next_entity,
            position: Vec3::new(x, 61.0, 20.5),
            previous: Vec3::new(x, 61.0, 20.5),
            velocity: Vec3::ZERO,
            value: 1,
            age: 0,
            moving: horizon_world::entity_move::MoveState {
                ground: horizon_world::ground_support::GroundState {
                    on_ground: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            count: 1,
            ticks: 0,
            removed: false,
        });
        game.next_entity
    };
    let (a, b, c) = (
        orb(&mut game, 5.5),
        orb(&mut game, 9.5),
        orb(&mut game, 13.5),
    );
    let seen = Rc::new(RefCell::new(Vec::new()));
    let added = Rc::new(RefCell::new(None));
    let (log, slot) = (seen.clone(), added.clone());
    on_cross(&mut game, move |g, point| {
        if let CrossPoint::OrbAfter(id) = point {
            log.borrow_mut().push((id, g.orbs.len()));
            if id == a {
                *slot.borrow_mut() = Some(orb(g, 3.5));
                g.orbs.iter_mut().find(|o| o.id == b).unwrap().removed = true;
                g.note_touched(Some(b));
            }
        }
    });
    game.tick_experience();
    assert_eq!(*seen.borrow(), vec![(a, 3), (c, 4)]);
    let id = added.borrow().unwrap();
    let age = |game: &Game<'_>, id: u32| game.orbs.iter().find(|o| o.id == id).map(|o| o.age);
    assert_eq!(age(&game, b), None);
    assert_eq!(age(&game, id), Some(0));
    assert_eq!(age(&game, a), Some(1));
    game.test_cross = None;
    game.tick_experience();
    assert_eq!(age(&game, id), Some(1));
}

#[test]
fn a_converted_mob_keeps_its_vehicle_and_its_passenger() {
    let Some(mut game) = flat_game() else { return };
    let pig = game.spawn_mob("pig", [2.5, 61.0, 2.5], true).unwrap();
    let zombie = game.spawn_mob("zombie", [2.5, 62.0, 2.5], true).unwrap();
    let skeleton = game.spawn_mob("skeleton", [2.5, 63.0, 2.5], true).unwrap();
    assert!(game.start_riding(Ent::Id(zombie), Ent::Id(pig), false, false));
    assert!(game.start_riding(Ent::Id(skeleton), Ent::Id(zombie), false, false));
    let from = game.mobs.iter().find(|m| m.id == zombie).unwrap().clone();
    let (vehicle, rider) = (
        game.mob_vehicle(zombie),
        game.first_passenger(Ent::Id(zombie)),
    );
    game.level_remove(Kind::Mob, zombie, Removal::Discarded);
    let drowned = game
        .convert_to(
            &from,
            "minecraft:drowned",
            horizon_client::mobs::conversion::Params::KEEP_ALL,
            vehicle,
            rider,
        )
        .unwrap();
    assert_eq!(game.mob_vehicle(drowned), Some(pig));
    assert_eq!(game.mob_vehicle(skeleton), Some(drowned));
    assert_eq!(game.first_passenger(Ent::Id(pig)), Some(Ent::Id(drowned)));
    assert_eq!(
        game.first_passenger(Ent::Id(drowned)),
        Some(Ent::Id(skeleton))
    );
}
