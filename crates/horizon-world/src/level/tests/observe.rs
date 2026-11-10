use super::*;

#[test]
fn block_observation_follows_successful_commits_only() {
    use std::cell::RefCell;

    #[derive(Debug, PartialEq)]
    enum Observation {
        Block(BlockPos, BlockStateId, BlockStateId, i32),
        Neighbor,
    }
    struct Observer {
        inner: TestLevel,
        loaded: bool,
        seen: RefCell<Vec<Observation>>,
    }
    impl LevelAccess for Observer {
        fn block_state(&self, pos: BlockPos) -> BlockStateId {
            self.inner.block_state(pos)
        }
        fn set_raw(&mut self, pos: BlockPos, state: BlockStateId) -> Option<BlockStateId> {
            self.inner.set_raw(pos, state)
        }
        fn is_loaded(&self, _pos: BlockPos) -> bool {
            self.loaded
        }
        fn random(&mut self) -> &mut LegacyRandom {
            self.inner.random()
        }
        fn game_time(&self) -> i64 {
            self.inner.game_time()
        }
        fn observe_block_changed(
            &self,
            pos: BlockPos,
            old: BlockStateId,
            new: BlockStateId,
            flags: i32,
        ) {
            assert_eq!(self.block_state(pos), new);
            self.seen
                .borrow_mut()
                .push(Observation::Block(pos, old, new, flags));
        }
        fn observe_neighbor_changed(
            &self,
            _: &mut Ticks,
            _: BlockPos,
            _: BlockStateId,
            _: BlockId,
            _: bool,
        ) {
            self.seen.borrow_mut().push(Observation::Neighbor);
        }
    }
    let Some(f) = fixture() else { return };
    let b = &f.behaviours;
    let mut level = Observer {
        inner: TestLevel::new(b),
        loaded: true,
        seen: RefCell::new(Vec::new()),
    };
    let mut ticks = Ticks::new(0);
    let pos = BlockPos::new(0, 80, 0);
    let stone = f.registry.default_state("minecraft:stone").unwrap();
    assert!(b.set_block(&mut level, &mut ticks, pos, stone, flags::UPDATE_ALL));
    let seen = level.seen.borrow();
    assert_eq!(
        seen[0],
        Observation::Block(pos, b.air(), stone, flags::UPDATE_ALL)
    );
    assert!(seen[1..].contains(&Observation::Neighbor));
    drop(seen);
    level.seen.borrow_mut().clear();
    assert!(!b.set_block(&mut level, &mut ticks, pos, stone, flags::UPDATE_ALL));
    level.loaded = false;
    assert!(!b.set_block(&mut level, &mut ticks, pos, b.air(), flags::UPDATE_ALL));
    level.loaded = true;
    assert!(!b.set_block(
        &mut level,
        &mut ticks,
        BlockPos::new(0, 320, 0),
        stone,
        flags::UPDATE_ALL
    ));
    assert!(level.seen.borrow().is_empty());
    let quiet = flags::UPDATE_CLIENTS | flags::UPDATE_SKIP_ON_PLACE;
    assert!(b.set_block(&mut level, &mut ticks, pos, b.air(), quiet));
    assert_eq!(
        level.seen.borrow()[0],
        Observation::Block(pos, stone, b.air(), quiet)
    );
}

#[test]
fn behaviours_signal_answers_the_level_signal_query() {
    let mut w = world!();
    let glass = w.id("glass", &[]);
    let pos = BlockPos::new(4, 1, 0);
    w.put(pos, glass);
    w.level.signals.insert(pos, (7, 15, None));
    let b = &w.f.behaviours;
    assert_eq!(
        b.signal(&mut w.level, &mut w.ticks, pos, Direction::East),
        7
    );
}
