use super::*;

impl World<'_> {
    /// A right click on the top of `clicked` under NeoForge's snapshot capture.
    fn place_captured(&mut self, item: &str, clicked: BlockPos) -> (PlaceOutcome, Vec<Snapshot>) {
        let ctx = PlaceContext {
            clicked_pos: clicked,
            face: Direction::Up,
            hit: horizon_core::geometry::Vec3::new(
                f64::from(clicked.x) + 0.5,
                f64::from(clicked.y) + 1.0,
                f64::from(clicked.z) + 0.5,
            ),
            inside: false,
            yaw: 180.0,
            pitch: 30.0,
            sneaking: false,
            item: format!("minecraft:{item}"),
            can_use_game_master_blocks: false,
        };
        self.f
            .behaviours
            .place_block_captured(&mut self.level, &mut self.ticks, &ctx)
    }
    /// The level's blocks other than air, for comparing two levels.
    fn solid_blocks(&self) -> HashMap<BlockPos, BlockStateId> {
        let air = self.b().air();
        self.level
            .blocks
            .iter()
            .filter(|(_, state)| **state != air)
            .map(|(pos, state)| (*pos, *state))
            .collect()
    }
}

#[test]
fn a_captured_placement_defers_on_place_and_updates() {
    let setup = |w: &mut World| {
        w.floor(0, 3, "stone");
        let lamp = w.id("redstone_lamp", &[("lit", "false")]);
        w.put(BlockPos::new(1, 1, 0), lamp);
    };
    let mut w = world!();
    setup(&mut w);
    let ground = BlockPos::new(0, 0, 0);
    let torch = BlockPos::new(0, 1, 0);
    let lamp = BlockPos::new(1, 1, 0);
    let (outcome, snapshots) = w.place_captured("redstone_torch", ground);
    assert!(matches!(outcome, PlaceOutcome::Placed { pos, .. } if pos == torch));
    assert_eq!(w.name(torch), "redstone_torch[lit=true]");
    assert_eq!(
        w.prop(lamp, "lit"),
        "false",
        "no neighbour heard the torch yet"
    );
    assert_eq!(
        snapshots,
        [Snapshot {
            pos: torch,
            old: w.b().air(),
            flags: flags::UPDATE_ALL_IMMEDIATE,
        }]
    );
    let b = w.b();
    b.finish_captured(&mut w.level, &mut w.ticks, &snapshots);
    assert_eq!(w.prop(lamp, "lit"), "true");

    let mut vanilla = World::new(w.f);
    setup(&mut vanilla);
    vanilla.place("redstone_torch", ground, Direction::Up, 180.0, 30.0, 0.0);
    assert_eq!(w.solid_blocks(), vanilla.solid_blocks());
}

#[test]
fn a_door_records_two_snapshots() {
    let mut w = world!();
    w.floor(0, 3, "grass_block");
    let (outcome, snapshots) = w.place_captured("oak_door", BlockPos::new(0, 0, 0));
    assert!(matches!(outcome, PlaceOutcome::Placed { .. }));
    let (lower, upper) = (BlockPos::new(0, 1, 0), BlockPos::new(0, 2, 0));
    let positions: Vec<_> = snapshots.iter().map(|s| s.pos).collect();
    assert_eq!(
        positions,
        [lower, upper],
        "the bottom by place, the top by setPlacedBy"
    );
    assert!(snapshots.iter().all(|s| s.old == w.b().air()));
    assert_eq!(w.prop(lower, "half"), "lower");
    assert_eq!(w.prop(upper, "half"), "upper");
}

#[test]
fn restore_puts_back_the_old_states_last_first() {
    let mut w = world!();
    w.floor(0, 3, "grass_block");
    let before = w.solid_blocks();
    let (_, snapshots) = w.place_captured("oak_door", BlockPos::new(0, 0, 0));
    assert_eq!(snapshots.len(), 2);
    let b = w.b();
    b.restore_captured(&mut w.level, &mut w.ticks, &snapshots);
    assert_eq!(w.get(BlockPos::new(0, 1, 0)), w.b().air());
    assert_eq!(w.get(BlockPos::new(0, 2, 0)), w.b().air());
    assert_eq!(w.solid_blocks(), before);
    assert!(
        !w.level
            .events
            .iter()
            .any(|e| matches!(e, LevelEvent::DropResources { .. })),
        "a restore drops nothing"
    );
}

#[test]
fn capture_set_block_skips_updates() {
    let mut w = world!();
    w.floor(0, 3, "stone");
    let lamp = BlockPos::new(1, 1, 0);
    let at = BlockPos::new(0, 1, 0);
    let unlit = w.id("redstone_lamp", &[("lit", "false")]);
    w.put(lamp, unlit);
    let air = w.b().air();
    let redstone = w.id("redstone_block", &[]);
    let b = w.b();
    assert!(b.set_block_captured(&mut w.level, &mut w.ticks, at, redstone, flags::UPDATE_ALL));
    assert_eq!(w.get(at), redstone);
    assert_eq!(w.prop(lamp, "lit"), "false", "the lamp hears nothing yet");
    b.on_place_at(&mut w.level, &mut w.ticks, at, redstone, air, false);
    b.mark_and_notify(
        &mut w.level,
        &mut w.ticks,
        at,
        air,
        redstone,
        flags::UPDATE_ALL,
        flags::UPDATE_LIMIT,
    );
    assert_eq!(w.prop(lamp, "lit"), "true");
}
