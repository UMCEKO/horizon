//! `Entity#move` of mobs and dropped items on a server `Game` over a stone floor at y 99.
use super::commands::Target;
use super::*;
use horizon_client::chunk::Chunk;
use horizon_core::position::ChunkPos;
use horizon_world::entity_move::MoverType;
use std::sync::Arc;

/// A server's game on the prepared resources, with a stone floor at y 99; None without them.
fn game(resources: &mut Option<Arc<Resources>>) -> Option<Box<Game<'_>>> {
    let root = std::env::var_os("HORIZON_TEST_RESOURCES")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../.local/horizon-client")
                .join(horizon_core::VANILLA_VERSION)
        });
    if !root.join("movement-properties.nbt").exists() {
        return None;
    }
    let settings = Settings::default();
    let loaded = load_resources(
        horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
        Vec::new(),
        None,
        &root,
        &settings,
        std::time::Instant::now(),
        false,
        execution_role::ExecutionRole::IntegratedServer,
        &Progress::new(),
        &mut |_, _| unreachable!(),
    )
    .unwrap();
    let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
    options.resources = root.clone();
    options.execution_role = execution_role::ExecutionRole::IntegratedServer;
    let res = resources.insert(loaded.resources);
    let mut game = Box::new(
        Game::new(
            res,
            loaded.pack,
            loaded.gui.unwrap(),
            options,
            settings,
            Audio::silent(&horizon_assets::resource_source::ResourceSource::directory(
                root.clone(),
            )),
            None,
        )
        .unwrap(),
    );
    game.options.dedicated = Some(Vec::new());
    game.build_registries();
    game.set_allow_commands(true);
    game.world.chunks.insert((0, 0), Arc::new(Chunk::empty(0, 0)));
    game.block_ticks
        .add_empty_chunk(ChunkPos::new(0, 0))
        .unwrap();
    assert!(game.run_command("fill 0 99 0 15 99 15 stone"));
    Some(game)
}

#[test]
fn a_dropped_item_moved_down_lands_on_the_floor() {
    let mut resources = None;
    let Some(mut game) = game(&mut resources) else {
        return;
    };
    assert!(game.run_command(
        "summon item 4.5 103 4.5 {Item:{id:\"minecraft:stick\",count:1},Motion:[0d,0d,0d]}"
    ));
    let id = game.dropped[0].id;
    assert!(game.entity_move(
        Target::Item(id),
        MoverType::SelfMovement,
        Vec3::new(0.0, -5.0, 0.0)
    ));
    let item = game.dropped.iter().find(|i| i.id == id).unwrap();
    assert_eq!(item.position.y, 100.0);
    assert!(item.on_ground && item.vertical_collision);
    assert_eq!(item.fall_distance, 0.0);
}

#[test]
fn a_mob_moved_into_a_wall_stops_and_flags_it() {
    let mut resources = None;
    let Some(mut game) = game(&mut resources) else {
        return;
    };
    assert!(game.run_command("fill 8 100 0 8 102 15 stone"));
    let id = game.spawn_mob_raw("zombie", [4.5, 100.0, 4.5], true).unwrap();
    assert!(game.entity_move(
        Target::Mob(id),
        MoverType::SelfMovement,
        Vec3::new(9.0, 0.0, 0.0)
    ));
    let mob = game.mobs.iter().find(|m| m.id == id).unwrap();
    assert!(mob.horizontal_collision);
    assert!((mob.position.x - 7.7).abs() < 1.0e-6, "{}", mob.position.x);
}

#[test]
fn an_entity_not_in_the_level_is_not_moved() {
    let mut resources = None;
    let Some(mut game) = game(&mut resources) else {
        return;
    };
    let delta = Vec3::new(1.0, 0.0, 0.0);
    for target in [
        Target::Mob(4242),
        Target::Item(4242),
        Target::Orb(4242),
        Target::Stand(4242),
        Target::Projectile(4242),
        Target::Boat(4242),
        Target::Cart(4242),
        Target::Tnt(4242),
        Target::Frame(4242),
        Target::Crystal(4242),
        Target::Dragon(4242),
    ] {
        assert!(!game.entity_move(target, MoverType::SelfMovement, delta), "{target:?}");
    }
}

fn moved(game: &mut Game<'_>, target: Target, delta: Vec3) {
    assert!(game.entity_move(target, MoverType::SelfMovement, delta));
}

#[test]
fn an_orb_moved_into_a_wall_stops_and_flags_it_and_one_moved_down_lands() {
    let mut resources = None;
    let Some(mut game) = game(&mut resources) else {
        return;
    };
    assert!(game.run_command("fill 8 100 0 8 102 15 stone"));
    assert!(game.run_command("summon experience_orb 4.5 100 4.5"));
    let id = game.orbs.last().unwrap().id;
    moved(&mut game, Target::Orb(id), Vec3::new(9.0, 0.0, 0.0));
    let orb = game.orbs.iter().find(|o| o.id == id).unwrap();
    assert!((orb.position.x - 7.75).abs() < 1.0e-6, "{}", orb.position.x);
    assert!(orb.moving.collision.horizontal && !orb.moving.collision.vertical);
    moved(&mut game, Target::Orb(id), Vec3::new(0.0, 3.0, 0.0));
    moved(&mut game, Target::Orb(id), Vec3::new(0.0, -5.0, 0.0));
    let orb = game.orbs.iter().find(|o| o.id == id).unwrap();
    assert_eq!(orb.position.y, 100.0);
    assert!(orb.moving.ground.on_ground && orb.moving.collision.below);
    assert_eq!(orb.moving.fall_distance, 0.0);
}

#[test]
fn an_armor_stand_moved_down_lands_and_a_marker_goes_through_the_floor() {
    let mut resources = None;
    let Some(mut game) = game(&mut resources) else {
        return;
    };
    assert!(game.run_command("summon armor_stand 4.5 103 4.5"));
    let stand = game.armor_stands.last().unwrap().id;
    assert!(game.run_command("summon armor_stand 10.5 103 4.5 {Marker:1b}"));
    let marker = game.armor_stands.last().unwrap().id;
    for id in [stand, marker] {
        moved(&mut game, Target::Stand(id), Vec3::new(0.0, -5.0, 0.0));
    }
    let landed = game.armor_stands.iter().find(|s| s.id == stand).unwrap();
    assert_eq!(landed.position.y, 100.0);
    assert!(landed.moving.ground.on_ground);
    let through = game.armor_stands.iter().find(|s| s.id == marker).unwrap();
    assert_eq!(through.position.y, 98.0);
}

#[test]
fn an_arrow_stuck_in_the_air_falls_after_a_push_but_not_after_its_own_move() {
    let mut resources = None;
    let Some(mut game) = game(&mut resources) else {
        return;
    };
    assert!(game.run_command("summon arrow 4.5 103 4.5 {inGround:1b,Motion:[0d,0d,0d]}"));
    let id = game.creatures.projectiles.last().unwrap().id;
    let in_ground = |game: &Game<'_>| {
        let p = game.creatures.projectiles.iter().find(|p| p.id == id).unwrap();
        (p.arrow.as_ref().unwrap().in_ground, p.stuck, p.position.x)
    };
    assert!(game.entity_move(
        Target::Projectile(id),
        MoverType::SelfMovement,
        Vec3::new(0.1, 0.0, 0.0)
    ));
    assert!(in_ground(&game).0);
    assert!(game.entity_move(
        Target::Projectile(id),
        MoverType::Piston,
        Vec3::new(0.1, 0.0, 0.0)
    ));
    let (in_ground, stuck, x) = in_ground(&game);
    assert!(!in_ground && !stuck);
    assert!((x - 4.7).abs() < 1.0e-9, "{x}");
}

#[test]
fn a_boat_moved_down_onto_slime_bounces_and_one_pushed_into_a_wall_stops() {
    let mut resources = None;
    let Some(mut game) = game(&mut resources) else {
        return;
    };
    assert!(game.run_command("fill 0 99 0 6 99 15 slime_block"));
    assert!(game.run_command("fill 14 100 0 14 102 15 stone"));
    assert!(game.run_command("summon oak_boat 2.5 102 4.5"));
    let falling = game.boats.last().unwrap().id;
    assert!(game.run_command("summon oak_boat 10.5 100 4.5"));
    let pushed = game.boats.last().unwrap().id;
    let i = game.boats.iter().position(|b| b.id == falling).unwrap();
    game.boats
        .update(i, |b| b.velocity = Vec3::new(0.0, -1.0, 0.0));
    moved(&mut game, Target::Boat(falling), Vec3::new(0.0, -3.0, 0.0));
    let boat = game.boats.iter().find(|b| b.id == falling).unwrap();
    assert_eq!(boat.position.y, 100.0);
    assert!(boat.moving.collision.below && boat.moving.ground.on_ground);
    assert!(boat.velocity.y > 0.0, "{:?}", boat.velocity);
    moved(&mut game, Target::Boat(pushed), Vec3::new(6.0, 0.0, 0.0));
    let boat = game.boats.iter().find(|b| b.id == pushed).unwrap();
    assert!(boat.moving.collision.horizontal);
    assert!((boat.position.x - (14.0 - 1.375 / 2.0)).abs() < 1.0e-6, "{}", boat.position.x);
}

#[test]
fn a_cart_and_a_primed_tnt_moved_down_land_on_the_floor() {
    let mut resources = None;
    let Some(mut game) = game(&mut resources) else {
        return;
    };
    assert!(game.run_command("summon minecart 10.5 103 10.5"));
    let cart = game.carts.last().unwrap().id;
    assert!(game.run_command("summon tnt 4.5 103 4.5 {fuse:80}"));
    let tnt = game.tnts.last().unwrap().id;
    for target in [Target::Cart(cart), Target::Tnt(tnt)] {
        moved(&mut game, target, Vec3::new(0.0, -5.0, 0.0));
    }
    let cart = game.carts.iter().find(|c| c.id == cart).unwrap();
    assert_eq!(cart.position.y, 100.0);
    assert!(cart.on_ground);
    let tnt = game.tnts.iter().find(|t| t.id == tnt).unwrap();
    assert_eq!(tnt.position.y, 100.0);
    assert!(tnt.on_ground);
}

#[test]
fn an_end_crystal_moves_by_the_delta_until_a_wall_stops_it() {
    let mut resources = None;
    let Some(mut game) = game(&mut resources) else {
        return;
    };
    assert!(game.run_command("fill 8 100 0 8 102 15 stone"));
    assert!(game.run_command("summon end_crystal 4.5 100 4.5 {ShowBottom:0b}"));
    let id = game.crystals.last().unwrap().id;
    moved(&mut game, Target::Crystal(id), Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(game.crystals.last().unwrap().position.x, 5.5);
    moved(&mut game, Target::Crystal(id), Vec3::new(9.0, 0.0, 0.0));
    assert_eq!(game.crystals.last().unwrap().position.x, 7.0);
}

#[test]
fn the_dragon_moves_through_blocks() {
    let mut resources = None;
    let Some(mut game) = game(&mut resources) else {
        return;
    };
    game.spawn_remote_dragon(900, [4.5, 100.0, 4.5], 0.0);
    moved(&mut game, Target::Dragon(900), Vec3::new(0.0, -3.0, 0.0));
    assert_eq!(game.dragon.as_ref().unwrap().position.y, 97.0);
}

#[test]
fn an_item_frame_moved_breaks_and_one_not_moved_stays() {
    let mut resources = None;
    let Some(mut game) = game(&mut resources) else {
        return;
    };
    assert!(game.run_command("summon item_frame 4.5 100.03 4.5 {Facing:1b}"));
    let id = game.item_frames.last().unwrap().id;
    moved(&mut game, Target::Frame(id), Vec3::ZERO);
    assert!(game.item_frames.iter().any(|f| f.id == id));
    moved(&mut game, Target::Frame(id), Vec3::new(1.0, 0.0, 0.0));
    assert!(game.item_frames.iter().all(|f| f.id != id));
}
