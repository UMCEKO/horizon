//! `MobLevel` for the game: mob code's level services, forwarded to the game's own.
use super::super::{
    Game,
    pickups::{Collector, Taken},
    world_entry::world_items,
};
use horizon_client::{
    entities::ItemEntity,
    items::ItemStack,
    mobs::{Mob, MobWorld},
};
use horizon_core::geometry::Vec3;
use horizon_gameplay::mob_level::{MobLevel, MobParts};

impl MobLevel for Game<'_> {
    fn parts(&mut self) -> MobParts<'_> {
        MobParts {
            mobs: &mut self.mobs,
            dropped: &mut self.dropped,
            next_entity: self.next_entity,
            ticked: &self.creatures.ticked,
            items: world_items(&self.world_tables, &self.remote, &self.res),
            facts: self.res.facts(),
            world: &mut self.world,
        }
    }
    fn mobs(&self) -> &[Mob] {
        &self.mobs
    }
    fn dropped(&self) -> &[ItemEntity] {
        &self.dropped
    }
    fn mob_world(&self) -> MobWorld<'_> {
        self.mob_world_base(&[])
    }
    fn dimension(&self) -> &'static str {
        self.dim.id()
    }
    fn host_uuid(&self) -> Option<[i32; 4]> {
        self.level.player_uuid
    }
    fn player_by_uuid(&self, uuid: [i32; 4]) -> Option<(Vec3, [f64; 3])> {
        let host = self
            .player_view()
            .filter(|p| !p.spectator && self.level.player_uuid.unwrap_or([0, 0, 0, 1]) == uuid)
            .map(|p| (p.position, p.eye));
        host.or_else(|| {
            self.lan_present()
                .into_iter()
                .find(|p| p.uuid == uuid && !p.spectator)
                .map(|p| {
                    let [x, y, z] = p.position;
                    (Vec3::new(x, y, z), p.eye)
                })
        })
    }
    fn rule(&self, name: &str) -> bool {
        self.ui.rules.bool(name)
    }
    fn spawn_item_own(&mut self, stack: ItemStack, at: [f64; 3], pickup_delay: u32) -> [f64; 3] {
        Game::spawn_item_own(self, stack, at, pickup_delay)
    }
    fn spawn_item(
        &mut self,
        stack: ItemStack,
        at: [f64; 3],
        velocity: [f64; 3],
        pickup_delay: u32,
    ) {
        Game::spawn_item(self, stack, at, velocity, pickup_delay);
    }
    fn just_spawned_item(&mut self) -> Option<&mut ItemEntity> {
        Game::just_spawned_item(self)
    }
    fn mob_uuid(&mut self, id: u32) -> [i32; 4] {
        Game::mob_uuid(self, id)
    }
    fn play_sound(&mut self, event: &str, at: [f64; 3], volume: f32, pitch: f32) {
        self.audio.play(event, Some(at), volume, pitch);
    }
    fn play_mob_sound(
        &mut self,
        index: usize,
        event: &str,
        source: usize,
        volume: f32,
        pitch: f32,
        except_actor: bool,
    ) {
        self.play_mob_bound_sound(index, event, source, volume, pitch, except_actor);
    }
    fn trigger_block_for(&mut self, uuid: [i32; 4], name: &str, pos: [i32; 3], stack: &ItemStack) {
        self.for_each_player(|g| {
            if g.acting_uuid() == uuid && !g.spectator() {
                g.trigger_block(name, pos, stack);
            }
        });
    }
    fn on_item_pickup(&mut self, mob: usize, item: &ItemEntity) {
        self.mob_item_pickup_trigger(mob, item);
    }
    fn take(&mut self, collector: u32, item: ItemEntity) {
        self.add_pickup(Taken::Item(Box::new(item)), Collector::Mob(collector));
    }
    fn discard_item(&mut self, index: usize) -> ItemEntity {
        self.remove_dropped(index)
    }
    fn on_equip_item(&mut self, id: u32, at: [f64; 3], equip: bool) {
        self.mob_equip_event(id, at, equip);
    }
}
