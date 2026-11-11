// game-module-cfg: feature java-mods
#![cfg(test)]
//! `java_target`: `ServerLevel.getEntity(int)` for Java's views, through the residency table.

use super::native_entity_refs::tests::with_game;
use super::{Dim, Game, boats::Boat, commands::Target, minecarts::{CartKind, Minecart}};
use horizon_client::mobs::Mob;
use horizon_core::geometry::Vec3;
use mod_api::RemovalReason;

fn pig(game: &Game<'_>, id: u32, x: f64) -> Mob {
    let kind = game.res.mob_types().get("minecraft:pig").unwrap().clone();
    Mob::new(id, kind, Vec3::new(x, 64.0, 0.5), 0.0)
}

#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn java_target_finds_entities_of_the_level_in_place() {
    with_game(|game| {
        game.mobs.push(pig(game, 300, 0.5));
        game.boats
            .push(Boat::new(301, "oak".into(), Vec3::new(2.5, 64.0, 0.5), 0.0));
        game.carts
            .push(Minecart::new(302, CartKind::Rideable, Vec3::new(4.5, 64.0, 0.5)));
        assert_eq!(game.java_target(300), Some(Target::Mob(300)));
        assert_eq!(game.java_target(301), Some(Target::Boat(301)));
        assert_eq!(game.java_target(302), Some(Target::Cart(302)));
    });
}

#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn java_target_of_an_entity_in_another_level_is_none() {
    with_game(|game| {
        game.boats
            .push(Boat::new(310, "oak".into(), Vec3::new(0.5, 64.0, 0.5), 0.0));
        game.enter_level(Dim::Nether).unwrap();
        assert_eq!(game.java_target(310), None);
        game.enter_level(Dim::Overworld).unwrap();
        assert_eq!(game.java_target(310), Some(Target::Boat(310)));
    });
}

#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn java_target_of_a_missing_id_or_a_player_is_none() {
    with_game(|game| {
        game.boats
            .push(Boat::new(320, "oak".into(), Vec3::new(0.5, 64.0, 0.5), 0.0));
        assert_eq!(game.java_target(0), None);
        assert_eq!(game.java_target(321), None);
        assert_eq!(game.java_target(u32::MAX), None);
        let own = game.own_entity_id() as u32;
        assert_eq!(game.java_target(own), None);
    });
}

#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn java_target_of_an_entity_removed_this_tick_is_none() {
    with_game(|game| {
        game.boats
            .push(Boat::new(330, "oak".into(), Vec3::new(0.5, 64.0, 0.5), 0.0));
        game.mobs.push(pig(game, 331, 2.5));
        for id in [330, 331] {
            let handle = game.entity(id).unwrap();
            assert!(game.entity_remove(handle, RemovalReason::Discarded));
            assert_eq!(game.java_target(id), None, "entity {id}");
        }
        assert!(game.boats.iter().any(|b| b.id == 330), "still in its list");
    });
}

/// Ari keeps no reachable copy of an entity lifted out for its turn, so no bridge resolves it (ari#2778 item 2).
#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn java_target_of_an_entity_lifted_for_its_turn_is_none() {
    with_game(|game| {
        game.mobs.push(pig(game, 340, 0.5));
        game.mobs.push(pig(game, 341, 2.5));
        let join = game.mobs[0].join;
        let lifted = game.mobs.extract(join).unwrap();
        assert_eq!(game.java_target(340), None);
        assert_eq!(game.java_target(341), Some(Target::Mob(341)));
        game.mobs.put_back(0, lifted);
        assert_eq!(game.java_target(340), Some(Target::Mob(340)));
        let all = game.mobs.lift();
        assert_eq!(game.java_target(341), None);
        game.mobs
            .restore(all, horizon_client::join_seq::RestoreOrder::SurvivorsFirst);
        assert_eq!(game.java_target(341), Some(Target::Mob(341)));
    });
}

#[test]
#[ignore = "timing; needs prepared resources in HORIZON_TEST_RESOURCES"]
fn java_target_bench() {
    with_game(|game| {
        for id in 1000..1400 {
            game.mobs.push(pig(game, id, f64::from(id - 1000)));
        }
        for id in 1400..1500 {
            let at = Vec3::new(f64::from(id - 1400), 64.0, 8.5);
            game.boats.push(Boat::new(id, "oak".into(), at, 0.0));
        }
        for id in 1500..1600 {
            let at = Vec3::new(f64::from(id - 1500), 64.0, 16.5);
            game.carts.push(Minecart::new(id, CartKind::Rideable, at));
        }
        const ROUNDS: u32 = 200;
        let time = |game: &mut Game<'_>, ids: std::ops::Range<u32>, found: bool| {
            let count = ids.len() as u32 * ROUNDS;
            let start = std::time::Instant::now();
            for _ in 0..ROUNDS {
                for id in ids.clone() {
                    assert_eq!(std::hint::black_box(game.java_target(id)).is_some(), found);
                }
            }
            start.elapsed().as_nanos() as f64 / f64::from(count)
        };
        let hit = time(game, 1000..1600, true);
        let miss = time(game, 5000..5600, false);
        println!("java_target over 600 entities: {hit:.0} ns found, {miss:.0} ns missing");
    });
}
