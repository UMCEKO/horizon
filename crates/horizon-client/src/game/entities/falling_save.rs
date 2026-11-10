//! Save visibility while authoritative rows are lent out.
use super::*;

impl Game<'_> {
    /// FallingBlockEntity.tick: fall with 0.04 gravity and 0.98F drag, then land.
    /// One falling block's `tick`: places or drops its block in its own tick,
    /// so a block falling onto one that just landed lands on top of it. `i` is
    /// its place in the list; the caller has checked it is ticking.
    /// `entities` are the boxes that stop its move.
    pub(in crate::game) fn tick_one_falling(&mut self, i: usize, entities: &[Aabb]) -> Result<()> {
        let entity_drops = self
            .ui
            .rules
            .bool_at(horizon_client::gamerules::vanilla::ENTITY_DROPS);
        let _save_lease = horizon_client::join_seq::visibility::PassLease::enter();
        let mut f = self.falling.extract(self.falling[i].join).unwrap();
        let (id, join) = (f.id, f.join);
        let original = f.clone();
        match self.fall_step(&mut f, entity_drops, entities) {
            Ok(FallStep::Falling) => {
                self.falling.put_back(i.min(self.falling.len()), f);
                return Ok(());
            }
            Ok(FallStep::Gone) => {}
            Ok(FallStep::Dropped(at)) => self.drop_falling_block(f.state, at),
            Ok(FallStep::Landed(pos)) => self.finish_falling_block(f, pos),
            Err(error) => {
                horizon_client::join_seq::store_back(&mut f, original);
                self.falling.put_back(i.min(self.falling.len()), f);
                return Err(error);
            }
        }
        self.falling.retire_extracted(id, join);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_client::chunk::Chunk;
    use horizon_core::position::ChunkPos;
    use std::sync::Arc;

    #[test]
    #[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES and scratch homes/cwd"]
    fn terminal_falling_ticks_finish_residency_and_preserve_outcomes() {
        crate::game::native_entity_refs::tests::with_game(|game| {
            game.build_registries();
            game.set_allow_commands(true);
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            game.block_ticks
                .add_empty_chunk(ChunkPos::new(0, 0))
                .unwrap();
            assert!(game.run_command("fill 0 199 0 15 199 15 stone"));
            for command in [
                "summon falling_block 2.5 204 4.5 {BlockState:\"minecraft:air\",Time:1}",
                "summon falling_block 4.5 204 4.5 {BlockState:\"minecraft:sand\",Time:600}",
                "summon falling_block 6.5 200 4.5 {BlockState:\"minecraft:sand\",Time:1}",
                "summon falling_block 8.5 204 4.5 {BlockState:\"minecraft:sand\",Time:1,NoGravity:1b}",
            ] {
                assert!(game.run_command(command));
            }
            let ids: Vec<_> = game.falling.iter().map(|f| f.id).collect();
            assert_eq!(ids.len(), 4);
            for id in &ids {
                game.with_residency_tick(|game| {
                    let index = game.falling.iter().position(|f| f.id == *id).unwrap();
                    game.tick_one_falling(index, &[])
                })
                .unwrap();
            }
            assert_eq!(game.falling.len(), 1);
            assert_eq!(game.falling[0].id, ids[3]);
            assert_eq!(game.falling[0].age, 2);
            assert_eq!(game.falling[0].position, Vec3::new(8.5, 204.0, 4.5));
            assert_eq!(
                &*game.res.facts().get(game.world.block(6, 200, 4)).name,
                "minecraft:sand"
            );
            assert_eq!(game.dropped.len(), 1);
            assert_eq!(&*game.dropped[0].stack.item, "minecraft:sand");
            assert_eq!(game.dropped[0].stack.count, 1);
        });
    }
}
