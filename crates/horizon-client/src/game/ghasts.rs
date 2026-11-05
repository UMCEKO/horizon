//! Happy ghasts on the game side: putting a harness on (and shearing it
//! off), riding, getting off on top, and the ghastling a dried ghast block
//! hatches.
use super::Game;
use horizon_client::{
    items::ItemStack,
    mobs::{MobEvent, happy_ghast},
    riding::Ent,
};
use horizon_core::geometry::Aabb;
use horizon_world::registry::BlockStateId;

impl Game<'_> {
    /// Client `EntityGetter.getEntityCollisions` for the player: the
    /// entities that `canBeCollidedWith` it, solid happy ghasts, living
    /// shulkers (`Shulker.canBeCollidedWith`) and boats
    /// (`AbstractBoat.canBeCollidedWith`), except the player's own vehicle.
    pub(super) fn player_entity_collisions(&self) -> Vec<Aabb> {
        let Some(p) = &self.player else {
            return Vec::new();
        };
        self.mobs
            .iter()
            .filter(|m| {
                (m.species() == horizon_client::mobs::Species::HappyGhast
                    && Some(m.id) != self.vehicle()
                    && happy_ghast::collides_with_player(m, p.position.y))
                    || (m.species() == horizon_client::mobs::Species::Shulker && m.alive())
            })
            .map(|m| m.bounds())
            .chain(
                self.boats
                    .iter()
                    .filter(|b| Some(b.id) != self.vehicle())
                    .map(super::boats::Boat::bounds),
            )
            .collect()
    }
    /// `HappyGhast.mobInteract`. Returns whether the click was used.
    pub(super) fn interact_happy_ghast(&mut self, index: usize, item: &str) -> bool {
        let m = &self.mobs[index];
        if !m.alive() || Some(m.id) == self.vehicle() {
            return false;
        }
        if m.is_baby() {
            // Animal.mobInteract: food grows it up.
            return false;
        }
        let at = m.center();
        let sneaking = self.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
        let harnessed = m.extra.head.is_some();
        // Equippable.equipOnInteract: a harness goes on a bare adult.
        if item.ends_with("_harness") && item.starts_with("minecraft:") && !harnessed {
            self.mobs[index].extra.head = Some(item.into());
            self.audio
                .play("minecraft:entity.happy_ghast.equip", Some(at), 1.0, 1.0);
            self.consume_held();
            return true;
        }
        // Equippable.canBeSheared: shears take the harness off again.
        if item == "minecraft:shears"
            && harnessed
            && !sneaking
            && let Some(harness) = self.mobs[index].extra.head.take()
        {
            self.audio
                .play("minecraft:entity.happy_ghast.unequip", Some(at), 1.0, 1.0);
            self.spawn_item(ItemStack::new(&harness, 1), at, [0.0, 0.2, 0.0], 10);
            self.damage_held_tool();
            return true;
        }
        if harnessed && !sneaking {
            // doPlayerRide.
            let id = self.mobs[index].id;
            self.break_leash(id, true, false);
            let was_empty = !self.is_vehicle(Ent::Id(id));
            // addPassenger: the rider is not standing on top, so no still
            // timeout.
            self.mobs[index].extra.counter = 0;
            self.yaw = self.mobs[index].yaw;
            if self.start_riding(self.acting_ent(), Ent::Id(id), false, true) {
                self.mount.reset_jump();
                if was_empty {
                    let down = happy_ghast::goggles(true, &self.mobs[index]);
                    let _ = self.handle_mob_events(vec![down]);
                }
            }
            return true;
        }
        false
    }
    /// `HappyGhast.removePassenger`.
    pub(super) fn happy_ghast_passenger_left(&mut self, id: u32) {
        // removePassenger: held still for half a second, and with no one
        // aboard the ghast forgets its home.
        let empty = !self.is_vehicle(Ent::Id(id));
        if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
            m.extra.counter = 10;
            if empty {
                m.ai.home = None;
            }
        }
        let Some(m) = self.mobs.iter().find(|m| m.id == id) else {
            return;
        };
        if empty {
            let up: MobEvent = happy_ghast::goggles(false, m);
            let _ = self.handle_mob_events(vec![up]);
        }
    }
    /// `DriedGhastBlock.spawnGhastling`: a ghastling facing the way the block did.
    pub(super) fn spawn_ghastling(&mut self, pos: [i32; 3], state: BlockStateId) {
        let at = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]),
            f64::from(pos[2]) + 0.5,
        ];
        let facing = self
            .pack
            .assets
            .registry
            .state(state)
            .and_then(|s| s.properties.get("facing").cloned());
        let yaw = match facing.as_deref() {
            Some("north") => 180.0,
            Some("east") => 270.0,
            Some("west") => 90.0,
            _ => 0.0,
        };
        if let Some(id) = self.spawn_mob("minecraft:happy_ghast", at, true)
            && let Some(m) = self.mobs.iter_mut().find(|m| m.id == id)
        {
            m.extra.age = -24000;
            m.yaw = yaw;
            m.body_yaw = yaw;
            m.head_yaw = yaw;
            let at = m.center();
            self.audio
                .play("minecraft:entity.ghastling.spawn", Some(at), 1.0, 1.0);
        }
    }
}
