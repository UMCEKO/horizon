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
    assert!(game.run_command("summon zombie 4.5 100 4.5 {NoAI:1b}"));
    let id = game.mobs[0].id;
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
    assert!(!game.entity_move(Target::Mob(4242), MoverType::SelfMovement, delta));
    assert!(!game.entity_move(Target::Item(4242), MoverType::SelfMovement, delta));
}
