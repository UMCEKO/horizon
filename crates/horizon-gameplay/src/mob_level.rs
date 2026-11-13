//! What mob code calls on `ServerLevel` and `Level`: the level's data as one
//! borrow ([`MobParts`]) and its services, which the game implements once.
use horizon_client::{
    blocks::BlockFacts,
    entities::ItemEntity,
    fast_hash::FxHashSet,
    items::{ItemStack, Items},
    join_seq::JoinList,
    mobs::{Mob, MobWorld},
    world::World,
};
use horizon_core::geometry::Vec3;

/// The level's data as mob code reads it: borrows only, nothing computed.
pub struct MobParts<'a> {
    pub mobs: &'a mut JoinList<Mob>,
    pub dropped: &'a mut JoinList<ItemEntity>,
    /// The id the next entity will take (`ItemAreas` keys its cache on it).
    pub next_entity: u32,
    /// The mobs that ticked this tick.
    pub ticked: &'a FxHashSet<u32>,
    pub items: &'a Items,
    pub facts: &'a BlockFacts,
    pub world: &'a mut World,
}

/// The services `Mob` code calls on its level beyond its data.
pub trait MobLevel {
    fn parts(&mut self) -> MobParts<'_>;
    /// The mobs, shared: for reads beside [`MobLevel::mob_world`].
    fn mobs(&self) -> &[Mob];
    /// The item entities, shared: for reads beside [`MobLevel::mob_world`].
    fn dropped(&self) -> &[ItemEntity];
    /// The level as mob rules read it (collisions, line of sight).
    fn mob_world(&self) -> MobWorld<'_>;
    /// `Level.dimension()`.
    fn dimension(&self) -> &'static str;
    /// The UUID of the level's own player, when it has one (a window's world).
    fn host_uuid(&self) -> Option<[i32; 4]>;
    /// `ServerLevel.getPlayerByUUID`, spectators skipped: the player's feet and eyes.
    fn player_by_uuid(&self, uuid: [i32; 4]) -> Option<(Vec3, [f64; 3])>;
    /// `ServerLevel.getGameRules().get(rule)` for a boolean rule.
    fn rule(&self, name: &str) -> bool;
    /// `new ItemEntity(level, x, y, z, stack)` and `addFreshEntity`: the item's
    /// own random draws its motion, which this returns.
    fn spawn_item_own(&mut self, stack: ItemStack, at: [f64; 3], pickup_delay: u32) -> [f64; 3];
    /// `new ItemEntity(level, x, y, z, stack, dx, dy, dz)` and `addFreshEntity`.
    fn spawn_item(&mut self, stack: ItemStack, at: [f64; 3], velocity: [f64; 3], pickup_delay: u32);
    /// The item entity the last spawn added, unless a mod kept it out of the level.
    fn just_spawned_item(&mut self) -> Option<&mut ItemEntity>;
    /// `Entity.getUUID` of a mob, made up for a new one.
    fn mob_uuid(&mut self, id: u32) -> [i32; 4];
    /// `Level.playSound` at a position, no player excluded.
    fn play_sound(&mut self, event: &str, at: [f64; 3], volume: f32, pitch: f32);
    /// `Level.playSound(except, mob, ...)` for mob `index`, from `source`; `except_actor`
    /// leaves out the acting player.
    fn play_mob_sound(
        &mut self,
        index: usize,
        event: &str,
        source: usize,
        volume: f32,
        pitch: f32,
        except_actor: bool,
    );
    /// A block criterion trigger for the player `uuid`, unless a spectator.
    fn trigger_block_for(&mut self, uuid: [i32; 4], name: &str, pos: [i32; 3], stack: &ItemStack);
    /// `LivingEntity.onItemPickup`: the thrower's advancement trigger.
    fn on_item_pickup(&mut self, mob: usize, item: &ItemEntity);
    /// `LivingEntity.take`: the pickup animation toward the mob `collector`.
    fn take(&mut self, collector: u32, item: ItemEntity);
    /// `Entity.discard` of item entity `index`: it leaves the level.
    fn discard_item(&mut self, index: usize) -> ItemEntity;
    /// `LivingEntity.onEquipItem`'s game event for the mob `id`: EQUIP, or UNEQUIP.
    fn on_equip_item(&mut self, id: u32, at: [f64; 3], equip: bool);
}
