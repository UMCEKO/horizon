//! Riding horses, donkeys and mules: feeding (`handleEating`), saddles,
//! armour and chests, mounting, the rider's control and jump bar
//! (`LocalPlayer.aiStep`'s riding jump), positioning, dismounting, and the
//! mount's health and jump bar on the HUD.
use super::{
    Game,
    vehicle_rules::{Seat, Vehicle},
};
use horizon_client::actions::Action;
use horizon_client::riding::Ent;
use horizon_client::{
    blocks::Fluid,
    hud::HudBatch,
    items::ItemStack,
    mobs::{MobEvent, Species, Who, equine},
    motion::MotionWorld,
    sprint::Keys,
};
use horizon_core::{geometry::Vec3, random::RandomSource};

/// `ItemStack.hurtAndConvertOnBreak`: the stack after `wear` more damage, or -
/// when that wear breaks it - the copy it leaves behind
/// (`transmuteCopyIgnoreEmpty`: the components carry over, and a damageable
/// copy starts whole). None while it still holds, so the caller adds the wear.
fn stick_on_break(stack: &ItemStack, wear: u32, max: u32) -> Option<ItemStack> {
    if max == 0 || stack.damage + wear < max {
        return None;
    }
    let mut rod = stack.transmute("minecraft:fishing_rod", 1);
    rod.damage = 0;
    Some(rod)
}

pub(super) fn steering_item(species: Species, main: Option<&str>, offhand: Option<&str>) -> bool {
    let item = match species {
        Species::Pig => "minecraft:carrot_on_a_stick",
        Species::Strider => "minecraft:warped_fungus_on_a_stick",
        _ => return false,
    };
    main == Some(item) || offhand == Some(item)
}

#[derive(Default)]
pub(super) struct MountState {
    /// `jumpRidingTicks`: -10 cooling down after a jump, >= 0 charging.
    pub(super) jump_ticks: i32,
    /// `jumpRidingScale` (0..1).
    pub(super) jump_scale: f32,
    was_jumping: bool,
}
impl MountState {
    pub(super) fn reset_jump(&mut self) {
        self.jump_ticks = 0;
        self.jump_scale = 0.0;
    }
}
/// `AbstractHorse.isFood` and `handleEating`: what a mount eats, and what that
/// mouthful does — `(heal, age up seconds, temper, falls in love)`. The temper
/// is the `u8` the mob stores it in.
///
/// `AbstractHorse.isFood` is the `#minecraft:horse_food` tag, `ZombieHorse`
/// overrides it with `#minecraft:zombie_horse_food` (a red mushroom and
/// nothing else), and the horse food tag carries no red mushrooms; a skeleton
/// horse inherits the horse food tag but only a tamed one gets that far
/// (`SkeletonHorse.mobInteract` passes for an untamed one).
fn eating(species: Species, item: &str) -> Option<(f32, i32, u8, bool)> {
    let camel = species == Species::Camel;
    // CamelHusk.isFood: rabbit's feet (and it never falls in love).
    if species == Species::CamelHusk && item != "minecraft:rabbit_foot" {
        return None;
    }
    if species != Species::CamelHusk && camel != (item == "minecraft:cactus") {
        return None;
    }
    let food = match species {
        Species::ZombieHorse => item == "minecraft:red_mushroom",
        Species::Horse | Species::Donkey | Species::Mule | Species::SkeletonHorse => {
            item != "minecraft:red_mushroom"
        }
        // Camels and llamas answer their own tags above or in the table.
        _ => true,
    };
    if !food {
        return None;
    }
    Some(match item {
        "minecraft:rabbit_foot" if species == Species::CamelHusk => (2.0, 10, 0, false),
        // Camel.handleEating.
        "minecraft:cactus" => (2.0, 10, 0, true),
        "minecraft:wheat" => (2.0, 20, 3, false),
        "minecraft:sugar" => (1.0, 30, 3, false),
        "minecraft:hay_block" => (20.0, 180, 0, false),
        "minecraft:apple" => (3.0, 60, 3, false),
        "minecraft:red_mushroom" => (3.0, 0, 3, false),
        "minecraft:carrot" => (3.0, 60, 3, false),
        "minecraft:golden_carrot" => (4.0, 60, 5, true),
        "minecraft:golden_apple" | "minecraft:enchanted_golden_apple" => (10.0, 240, 10, true),
        _ => return None,
    })
}
/// `EntityTypeTags.DISMOUNTS_UNDERWATER` (`Entity.dismountsUnderwater`): the mounts that throw their
/// riders off when the rider's eyes are in water.
pub(super) fn dismounts_underwater(species: Species) -> bool {
    matches!(
        species,
        Species::Camel
            | Species::Chicken
            | Species::Donkey
            | Species::HappyGhast
            | Species::Horse
            | Species::Llama
            | Species::Mule
            | Species::Pig
            | Species::Ravager
            | Species::Spider
            | Species::Strider
            | Species::TraderLlama
            | Species::ZombieHorse
    )
}

impl Game<'_> {
    pub(super) fn mount_index(&self) -> Option<usize> {
        let id = self.vehicle()?;
        self.mobs.iter().position(|m| m.id == id)
    }
    /// `Entity.interact` / `Mob.canShearEquipment` / `Mob.shearItem`:
    /// the pig's ordinary saddle comes off before Pig.mobInteract can ride it.
    /// An occupied pig consumes no equipment interaction and cannot be mounted.
    pub(super) fn shear_pig_saddle(&mut self, index: usize, item: &str) -> Option<bool> {
        let m = &self.mobs[index];
        if item != "minecraft:shears" || m.species() != Species::Pig || m.is_baby() || !m.alive() {
            return None;
        }
        let id = m.id;
        if self.is_vehicle(Ent::Id(id)) || m.rider.is_some() {
            return Some(false);
        }
        if self.keys.held(Action::Sneak) {
            return None;
        }
        let saddle = m.saddle_item()?;
        if &*saddle.item != "minecraft:saddle" {
            return None;
        }
        let at = m.position;
        let offset = horizon_client::mobs::riders::passenger_attachment(m, 0, m.yaw, false)[1];
        self.damage_held_tool();
        self.mobs[index].set_saddle(None);
        let context = self.who_context(Some(Who::Player), None);
        self.post_game_event("minecraft:shear", [at.x, at.y, at.z], context);
        self.spawn_at_location(at, offset, saddle.clone());
        let target = self.mob_entity(&self.mobs[index]);
        self.trigger_item_entity("minecraft:player_sheared_equipment", &saddle, &target);
        Some(true)
    }
    /// `Pig` / `Strider.mobInteract`: saddle an adult, or ride a saddled one.
    fn interact_steerable(&mut self, index: usize, item: &str) -> bool {
        let held = self.inventory[self.selected].clone();
        let ridden = Some(self.mobs[index].id) == self.vehicle();
        let m = &mut self.mobs[index];
        if m.is_baby() || !m.alive() || ridden {
            return false;
        }
        let at = m.center();
        if item == "minecraft:saddle" && !m.extra.saddled {
            m.set_saddle(held);
            let sound = if m.species() == Species::Strider {
                "minecraft:entity.strider.saddle"
            } else {
                "minecraft:entity.pig.saddle"
            };
            self.audio.play(sound, Some(at), 0.5, 1.0);
            self.consume_held();
            self.world
                .modified
                .insert((at[0].floor() as i32 >> 4, at[2].floor() as i32 >> 4));
            return true;
        }
        // Pig.mobInteract: held food goes to Animal.mobInteract, not a ride.
        let food = m.kind.food.iter().any(|f| f.as_str() == item);
        if !m.extra.saddled || food || self.keys.held(Action::Sneak) {
            return false;
        }
        let id = m.id;
        if !self.may_board() {
            return true;
        }
        self.start_riding(self.acting_ent(), Ent::Id(id), false, true);
        true
    }
    /// `FoodOnAStickItem.use` while riding: boost the mount, wear the stick.
    pub(super) fn use_steering_stick(&mut self, item: &str) -> bool {
        let (species, wear) = match item {
            "minecraft:carrot_on_a_stick" => (Species::Pig, 7),
            "minecraft:warped_fungus_on_a_stick" => (Species::Strider, 1),
            _ => return false,
        };
        let Some(i) = self.mount_index() else {
            return false;
        };
        if self.mobs[i].species() != species || !equine::boost(&mut self.mobs[i]) {
            return false;
        }
        if !self.creative
            && let Some(stack) = self.inventory[self.selected].clone()
        {
            // `ItemStack.hurtAndBreak`: Unbreaking has its say first, and the
            // durability trigger only hears the wear it leaves.
            let wear = self.wear(&stack, wear);
            let max = self
                .res
                .items()
                .get(&stack.item)
                .map_or(25, |i| i.max_damage);
            match stick_on_break(&stack, wear, max) {
                Some(rod) => {
                    // `hurtAndConvertOnBreak`: the stick breaks (the broken
                    // statistic and `onEquippedItemBroken`), then what is
                    // left is the rod, not a fresh plain one.
                    self.award_stat("broken", &stack.item, 1);
                    self.player_equipment_broke(
                        horizon_world::player_inventory::EquipmentSlot::MainHand,
                        &stack,
                    );
                    self.inventory[self.selected] = Some(rod);
                }
                None => {
                    if let Some(stack) = self.inventory[self.selected].as_mut() {
                        stack.damage += wear;
                    }
                }
            }
        }
        self.swing_arm();
        true
    }
    /// `AbstractHorse.mobInteract`. Returns whether the click was used.
    pub(super) fn interact_mount(&mut self, index: usize, item: &str) -> bool {
        let held = self.held().cloned();
        let m = &self.mobs[index];
        let species = m.species();
        // A server player rides it already (`isVehicle`: no seat, no feeding).
        if self.ridden_by_guest(m.id) {
            return false;
        }
        if matches!(species, Species::Pig | Species::Strider) {
            return self.interact_steerable(index, item);
        }
        if horizon_client::mobs::llama::is_llama(species) {
            return self.interact_llama(index, item);
        }
        if species == Species::HappyGhast {
            return self.interact_happy_ghast(index, item);
        }
        if horizon_client::mobs::nautilus::is_nautilus(species) {
            return self.interact_nautilus(index, item);
        }
        if !equine::is_equine(species) || !m.alive() || Some(m.id) == self.vehicle() {
            return false;
        }
        // AbstractHorse.mobInteract: a horse a mob rides (`isVehicle`) has no
        // seat, inventory or feeding to give. Camels seat two and check their
        // passengers themselves.
        let id = m.id;
        if !matches!(species, Species::Camel | Species::CamelHusk)
            && self
                .passengers_of(Ent::Id(id))
                .iter()
                .any(|p| p.id().is_some())
        {
            return false;
        }
        // SkeletonHorse.mobInteract: nothing for an untamed one.
        if species == Species::SkeletonHorse && !m.extra.tame {
            return false;
        }
        let sneaking =
            self.keys.held(Action::Sneak) || self.keys.raw(&winit::keyboard::KeyCode::ShiftRight);
        // Horse / ZombieHorse.mobInteract: food is fed, and anything else in
        // hand makes an untamed one mad instead of seating the player.
        let dandelion = [self.held(), self.offhand.as_ref()]
            .into_iter()
            .flatten()
            .any(|s| &*s.item == "minecraft:golden_dandelion");
        let mut events = Vec::new();
        match equine::held_item(
            &mut self.mobs[index],
            item,
            sneaking,
            dandelion,
            &mut events,
        ) {
            Some(equine::HeldItem::Feed) => return self.feed_mount(index, item),
            Some(equine::HeldItem::Refused) => {
                let _ = self.handle_mob_events(events);
                return true;
            }
            None => {}
        }
        let m = &self.mobs[index];
        // AbstractHorse.mobInteract: shift + use opens a tamed adult's inventory.
        if !m.is_baby() && m.extra.tame && sneaking {
            self.open_mount_menu(index);
            return true;
        }
        if self.feed_mount(index, item) {
            return true;
        }
        let m = &mut self.mobs[index];
        if m.is_baby() {
            return false;
        }
        let at = m.center();
        let tamed = m.extra.tame;
        let sound = if item == "minecraft:saddle" && tamed && !m.extra.saddled {
            m.set_saddle(held);
            // Camel.getSaddleSound (the equip sound of a camel's saddle).
            Some(match species {
                Species::Camel => "minecraft:entity.camel.saddle",
                Species::CamelHusk => "minecraft:entity.camel_husk.saddle",
                _ => "minecraft:entity.horse.saddle",
            })
        } else if item.ends_with("_horse_armor")
            && species == Species::Horse
            && tamed
            && m.extra.body.is_none()
        {
            m.extra.body = Some(item.into());
            Some("minecraft:entity.horse.armor")
        } else if item == "minecraft:chest"
            && matches!(species, Species::Donkey | Species::Mule)
            && tamed
            && !m.extra.chested
        {
            m.extra.chested = true;
            Some("minecraft:entity.donkey.chest")
        } else {
            None
        };
        if let Some(sound) = sound {
            let pitch =
                (self.world.random.next_float() - self.world.random.next_float()) * 0.2 + 1.0;
            self.audio.play(sound, Some(at), 0.5, pitch);
            self.consume_held();
            self.world
                .modified
                .insert((at[0].floor() as i32 >> 4, at[2].floor() as i32 >> 4));
            return true;
        }
        // doPlayerRide
        let id = self.mobs[index].id;
        if !self.may_board() {
            return true;
        }
        let m = &mut self.mobs[index];
        m.extra.flag = false;
        m.extra.counter = 0;
        self.yaw = m.yaw;
        if self.start_riding(self.acting_ent(), Ent::Id(id), false, true) {
            self.mount.jump_ticks = 0;
            self.mount.jump_scale = 0.0;
        }
        true
    }
    /// `AbstractHorse.handleEating`.
    fn feed_mount(&mut self, index: usize, item: &str) -> bool {
        let Some((heal, age_up, temper, love)) = eating(self.mobs[index].species(), item) else {
            return false;
        };
        let m = &mut self.mobs[index];
        let mut used = false;
        let mut loved = false;
        if love && m.extra.tame && m.extra.age == 0 && m.extra.in_love <= 0 {
            m.extra.in_love = 600;
            m.extra.love_player = true;
            used = true;
            loved = true;
        }
        if m.health < m.attrs.max_health && heal > 0.0 {
            m.heal(heal);
            used = true;
        }
        if m.is_baby() && age_up > 0 && !m.extra.age_locked {
            m.extra.age = (m.extra.age + age_up * 20).min(0);
            used = true;
        }
        if temper > 0 && (used || !m.extra.tame) && m.extra.variant3 < 100 {
            m.extra.variant3 = (m.extra.variant3 + temper).min(100);
            used = true;
        }
        if !used {
            return false;
        }
        // eating(): the mouth opens and the horse eats.
        m.anim.v[2] = 1.0;
        let at = m.center();
        let id = m.id;
        let sound = match m.species() {
            Species::Camel => "minecraft:entity.camel.eat",
            Species::CamelHusk => "minecraft:entity.camel_husk.eat",
            Species::Donkey => "minecraft:entity.donkey.eat",
            Species::Mule => "minecraft:entity.mule.eat",
            Species::Horse if m.is_baby() => "minecraft:entity.baby_horse.eat",
            // ZombieHorse.getEatingSound.
            Species::ZombieHorse => "minecraft:entity.zombie_horse.eat",
            _ => "minecraft:entity.horse.eat",
        };
        let pitch = (self.world.random.next_float() - self.world.random.next_float()) * 0.2 + 1.0;
        self.audio.play(sound, Some(at), 1.0, pitch);
        // Animal.setInLove: entity event 18.
        if loved {
            self.love_cause.insert(id, self.lan_acting);
            let kind = self.mobs[index].kind.id.clone();
            self.native_entity_enter_love_mode(id, &kind, self.lan_acting_uuid);
            self.mob_event_particles(id, 18);
        }
        self.consume_held();
        true
    }
    /// `LocalPlayer.aiStep`'s riding jump: the bar charges while jump is
    /// held; on release, the jump's strength (0 when there is none).
    pub(super) fn charge_mount_jump(&mut self, i: usize, keys: Keys) -> f32 {
        let saddled = self.mobs[i].extra.saddled && Self::jumps(self.mobs[i].species());
        let mut jump = 0.0;
        if saddled {
            let st = &mut self.mount;
            if st.jump_ticks < 0 {
                st.jump_ticks += 1;
                if st.jump_ticks == 0 {
                    st.jump_scale = 0.0;
                }
            }
            if st.was_jumping && !keys.jump {
                st.jump_ticks = -10;
                let amount = (st.jump_scale * 100.0).floor() as i32;
                // PlayerRideableJumping.getPlayerJumpPendingScale, the scale a
                // guest's START_RIDING_JUMP carries as `data`.
                jump = equine::jump_pending_scale(amount);
            } else if !st.was_jumping && keys.jump {
                st.jump_ticks = 0;
                st.jump_scale = 0.0;
            } else if st.was_jumping {
                st.jump_ticks += 1;
                st.jump_scale = if st.jump_ticks < 10 {
                    st.jump_ticks as f32 * 0.1
                } else {
                    0.8 + 2.0 / (st.jump_ticks - 9) as f32 * 0.1
                };
            }
            st.was_jumping = keys.jump;
        }
        jump
    }
    /// Whether the rider's eyes are in water (`Entity.updateFluidOnEyes` then `isEyeInFluid(WATER)`),
    /// and not in a bubble column, as `LivingEntity.baseTick` asks before it lets a rider stay.
    fn rider_eyes_in_water(&self) -> bool {
        let Some([x, y, z]) = self.eye_position() else {
            return false;
        };
        let (bx, by, bz) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
        if &*self.res.facts().get(self.world.block(bx, by, bz)).name == "minecraft:bubble_column" {
            return false;
        }
        self.fluid_height_of(bx, by, bz, Some(Fluid::Water))
            .is_some_and(|(height, _)| f64::from(by) + f64::from(height) > y)
    }
    /// The rider's input for this tick; returns false when not on a mount.
    pub(super) fn tick_mount_riding(&mut self, keys: Keys) -> bool {
        let Some(i) = self.mount_index() else {
            return false;
        };
        if keys.shift || !self.mobs[i].alive() {
            self.dismount_mount();
            return true;
        }
        // `LivingEntity.baseTick`: a rider whose eyes are in water gets off a mount of the
        // `dismounts_underwater` tag (a server's player too: `leave_guest` tells their client).
        if dismounts_underwater(self.mobs[i].species()) && self.rider_eyes_in_water() {
            self.dismount_mount();
            return true;
        }
        let strafe = f32::from(u8::from(keys.left)) - f32::from(u8::from(keys.right));
        let forward = f32::from(u8::from(keys.forward)) - f32::from(u8::from(keys.backward));
        // LocalPlayer.aiStep: the riding jump charges while space is held.
        let steer = steering_item(
            self.mobs[i].species(),
            self.held().map(|s| s.item.as_ref()),
            self.offhand.as_ref().map(|s| s.item.as_ref()),
        );
        let jump = self.charge_mount_jump(i, keys);
        let m = &mut self.mobs[i];
        if jump > 0.0 {
            // AbstractHorse.onPlayerJump: slide on the keys and rear (horses,
            // donkeys and mules).
            if equine::is_equine(m.species()) {
                equine::on_player_jump(m);
            }
            // The nautilus plays its dash sound itself.
            if equine::is_equine(m.species()) {
                // playJumpSound, which the server broadcasts for a guest.
                let sound = format!("minecraft:{}", equine::jump_sound(m));
                let at = m.center();
                self.audio.play(&sound, Some(at), 0.4, 1.0);
            }
        }
        let m = &mut self.mobs[i];
        let pending = m.rider.map_or(0.0, |r| r.jump);
        m.rider = Some(equine::Rider {
            strafe,
            forward,
            yaw: self.yaw,
            pitch: self.pitch,
            jump: if jump > 0.0 { jump } else { pending },
            steer,
            sprint: keys.sprint || self.keys.held(Action::Sprint),
            jumping: keys.jump,
            // A server's player (a LAN guest, single player's window) drives
            // a horse from its own client (`handleMoveVehicle`).
            client: self.lan_acting.is_some(),
        });
        true
    }
    /// `LivingEntity.baseTick` for the mobs that ride a mount of the `dismounts_underwater` tag (a baby
    /// zombie on a chicken): with their eyes in water they get off it.
    pub(super) fn dismount_underwater_mob_riders(&mut self) {
        let swimmers: Vec<u32> = self
            .mobs
            .iter()
            .filter(|m| m.alive() && m.fluids.eye_in_water)
            .filter_map(|m| {
                let mount = self.vehicle_of(Ent::Id(m.id)).and_then(Ent::id)?;
                let under = self.mobs.iter().find(|v| v.id == mount)?;
                dismounts_underwater(under.species()).then_some(m.id)
            })
            .collect();
        for rider in swimmers {
            self.dismount_mob(rider);
        }
    }
    /// `positionRider`: the player sits on the mount after it moves.
    pub(super) fn position_rider(&mut self) {
        let Some(i) = self.mount_index() else {
            return;
        };
        // The player's place among the mount's passengers: on a client, as the server sent them.
        let id = self.mobs[i].id;
        let (list, me) = match self.remote.as_ref() {
            Some(r) => (r.passengers.passengers_of(Ent::Id(id)).to_vec(), Ent::Host),
            None => (self.passengers_of(Ent::Id(id)), self.acting_ent()),
        };
        let index = list.iter().position(|&p| p == me).unwrap_or(0);
        let at = self.passenger_point(
            Vehicle::Mob(i),
            index,
            list.len().max(1),
            Seat::PLAYER,
            false,
        );
        // The tick a rider takes its seat is a teleport (`Entity.moveTo`), which
        // sets the old position too: the camera must not sweep from where the
        // player stood to the seat. The ticks after it move with the mount and
        // are interpolated as usual.
        let first = self.posed_rider != self.vehicle();
        self.posed_rider = self.vehicle();
        if let Some(p) = self.player.as_mut() {
            self.previous_position = if first { at } else { p.position };
            p.position = at;
            p.velocity = Vec3::ZERO;
            p.fall_distance = 0.0;
        }
    }
    /// `removePassenger` + `getDismountLocationForPassenger`: beside the mount.
    pub(super) fn dismount_mount(&mut self) {
        let Some(id) = self.vehicle() else {
            return;
        };
        if !self.native_may_leave(id) {
            return;
        }
        self.stop_riding(self.acting_ent());
        if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
            m.rider = None;
        }
    }
    /// `AbstractHorse.tameWithName`: the acting player owns `mob`, then gets
    /// the `tame_animal` trigger.
    fn tame_mount(&mut self, mob: u32) {
        let owner = self.acting_player_uuid();
        if let Some(m) = self.mobs.iter_mut().find(|m| m.id == mob) {
            m.extra.owner = Some(owner);
            m.persistent = true;
            // AbstractHorse.tameWithName: entity event 7, spawnTamingParticles(true).
            self.mob_event_particles(mob, 7);
        }
        if let Some(view) = self
            .mobs
            .iter()
            .find(|m| m.id == mob)
            .map(|m| self.mob_entity(m))
        {
            self.trigger_entity("minecraft:tame_animal", "entity", &view);
        }
        if let Some(kind) = self
            .mobs
            .iter()
            .find(|m| m.id == mob)
            .map(|m| m.kind.id.clone())
        {
            let owner = horizon_client::lan::playerdata::ints_uuid(owner);
            self.native_entity_tame(mob, &kind, Some(owner));
        }
    }
    pub(super) fn mount_event(&mut self, e: &MobEvent) {
        match *e {
            MobEvent::Tamed { mob } => {
                // tameWithName(player): the rider, a server's player in its turn.
                let rider = self
                    .passengers_of(Ent::Id(mob))
                    .into_iter()
                    .find_map(|e| match e {
                        Ent::Guest(c) => Some(c),
                        _ => None,
                    });
                if let Some(conn) = rider
                    && let Some(mut host) = self.lan.take()
                {
                    self.as_guest(&mut host, conn, |g| g.tame_mount(mob));
                    self.lan = Some(host);
                } else {
                    self.tame_mount(mob);
                }
            }
            MobEvent::Buck { mob } => {
                if self.vehicle() == Some(mob) {
                    self.dismount_mount();
                } else {
                    // ejectPassengers: a server's player gets off too.
                    self.eject_passengers(Ent::Id(mob));
                }
                // RunAroundLikeCrazyGoal.tick: entity event 6, smoke.
                self.mob_event_particles(mob, 6);
            }
            // `LivingEntity.stabAttack`: `target.stopRiding()`.
            MobEvent::StabDismount { target } => match target {
                Who::Player => self.dismount_player(),
                Who::Mob(id) => self.dismount_mob(id),
            },
            _ => {}
        }
    }
    /// `Entity.propagateFallToPassengers`: whoever rides the mob `mob` takes
    /// `causeFallDamage` of its fall, as the mount's own damage does. The local
    /// player, the server players and the mob on its back each get it, with
    /// their own safe distance and armour.
    pub(super) fn fall_to_passengers(&mut self, mob: u32, distance: f64, modifier: f32) {
        if self.vehicle() == Some(mob) {
            self.rider_fall_damage(distance, modifier);
        }
        let guests: Vec<u32> = self
            .passengers_of(Ent::Id(mob))
            .into_iter()
            .filter_map(|e| match e {
                Ent::Guest(c) if Some(c) != self.lan_acting => Some(c),
                _ => None,
            })
            .collect();
        if !guests.is_empty()
            && let Some(mut host) = self.lan.take()
        {
            for conn in guests {
                self.guest_hit_by(&mut host, conn, |g| g.rider_fall_damage(distance, modifier));
            }
            self.lan = Some(host);
        }
        for rider in self.passengers_of(Ent::Id(mob)) {
            if let Some(r) = self.mobs.iter_mut().find(|m| Some(m.id) == rider.id()) {
                let mut events = Vec::new();
                let ride = horizon_client::mobs::anim::Ride::of(&self.riding_table, r.id);
                r.cause_fall_damage_from(distance, modifier, ride, &mut events);
                let _ = self.handle_mob_events(events);
            }
        }
    }
    /// The player this game runs (the local one, or the server player it acts
    /// as) takes the fall of the mount under it. `cause_mount_fall` runs the
    /// landing code's own fall damage for the player: creative flight is
    /// exempt, and the statistic, the sounds and the damage records wait in the
    /// player's events for its next tick, as a landing of its own leaves them.
    fn rider_fall_damage(&mut self, distance: f64, modifier: f32) {
        let Some(player) = self.player.as_mut() else {
            return;
        };
        let _ = MotionWorld {
            entities: &[],
            player,
            scene: &mut self.world,
            assets: &self.pack.assets,
            collision_binding: self.collision,
            resources: self.motion,
            health: &self.health_resources,
        }
        .cause_mount_fall(distance, modifier);
    }
    /// `PlayerRideableJumping`: horses, camels and nautili (the dash).
    fn jumps(species: Species) -> bool {
        equine::is_equine(species) || horizon_client::mobs::nautilus::is_nautilus(species)
    }
    /// `Player.jumpableVehicle`: a saddled mount that jumps (not a pig or a
    /// strider, which a saddle only steers).
    pub(super) fn jumpable_mount(&self) -> bool {
        self.mount_index()
            .is_some_and(|i| self.mobs[i].extra.saddled && Self::jumps(self.mobs[i].species()))
    }
    /// `willPrioritizeJumpInfo`: charging a jump or recharging.
    pub(super) fn mount_jumping(&self) -> bool {
        self.mount.jump_scale > 0.0 || self.mount.jump_ticks < 0
    }
    /// `Hud.getVehicleMaxHearts` of the ridden mob (0 = none).
    pub(super) fn mount_hearts(&self) -> i32 {
        self.mount_index().map_or(0, |i| {
            horizon_client::hud_layout::vehicle_max_hearts(self.mobs[i].attrs.max_health)
        })
    }
    /// `Hud.extractVehicleHealth` (a native mod's `minecraft:vehicle_health`
    /// layer, only while riding a mount). Returns the mount's heart count
    /// (`getVehicleMaxHearts`).
    pub(super) fn draw_mount_hud(&mut self, batch: &mut HudBatch, w: i32, h: i32) -> i32 {
        let Some(i) = self.mount_index() else {
            return 0;
        };
        let m = &self.mobs[i];
        let hearts = horizon_client::hud_layout::vehicle_max_hearts(m.attrs.max_health);
        let draws = horizon_client::hud_layout::vehicle_health(w, h, hearts, m.health);
        self.gui_layer("minecraft:vehicle_health", batch, |g, batch| {
            horizon_client::hud_layout::paint(batch, &g.gui.atlas, &draws);
        });
        hearts
    }
    /// The jump bar when it is the contextual bar.
    pub(super) fn draw_jump_bar(&self, batch: &mut HudBatch, w: i32, h: i32) {
        let Some(i) = self.mount_index() else {
            return;
        };
        let m = &self.mobs[i];
        // JumpableVehicleBar: `getJumpCooldown`, which only the camel's and
        // the nautilus' dash have; a horse always shows the progress.
        let cooldown = i32::from(
            self.mount.jump_ticks < 0
                && (matches!(m.species(), Species::Camel | Species::CamelHusk)
                    || horizon_client::mobs::nautilus::is_nautilus(m.species())),
        );
        let draws = horizon_client::hud_overlays::jump_bar(w, h, cooldown, self.mount.jump_scale);
        horizon_client::hud_layout::paint(batch, &self.gui.atlas, &draws);
    }
}

/// `ServerGamePacketListenerImpl.handlePlayerCommand` START_RIDING_JUMP on the
/// vehicle `m` (`PlayerRideableJumping`): `canJump()`, a scale above zero and
/// then the vehicle's own `handleStartJump`. Returns the sound the server has
/// to play, with its volume — `AbstractHorse.playJumpSound` at 0.4, `Camel`'s
/// dash at 1.0 — or None when the mount takes no such jump or plays its own
/// dash sound (the nautilus does).
///
/// Nothing here moves the mount on its own: what a horse, camel or nautilus
/// does with the jump is its `handleStartJump`, and the travel that follows
/// (a dash, a rear) is the mount's own tick.
pub(super) fn start_riding_jump(
    m: &mut horizon_client::mobs::Mob,
    data: i32,
) -> Option<(&'static str, f32)> {
    // PlayerRideableJumping.canJump: AbstractHorse.isSaddled, and a camel that
    // is not refusing to move keeps its own gate.
    if data <= 0 || !m.extra.saddled {
        return None;
    }
    if crate::mobs::camel::is_camel(m.species()) {
        if crate::mobs::camel::refuse_to_move(m) {
            return None;
        }
        // Camel.handleStartJump: the dash sound, the entity action and DASH.
        crate::mobs::camel::set_dashing(m, true);
        Some((crate::mobs::camel::voice(m, 3), 1.0))
    } else if crate::mobs::nautilus::is_nautilus(m.species()) {
        // AbstractNautilus.handleStartJump: DASH, and the dash sound it makes
        // itself when the dash runs. The momentum is `onPlayerJump`'s pending
        // scale, which the nautilus spends on its next travel; a guest's ride
        // tick fills `Rider::jump` from the keys the host hears, so the scale
        // its own bar charged comes from this packet.
        crate::mobs::nautilus::on_player_jump(m, data);
        None
    } else if equine::is_equine(m.species()) {
        // AbstractHorse.handleStartJump, which is onPlayerJump for the slide
        // and the rear, then playJumpSound.
        equine::on_player_jump(m);
        Some((equine::jump_sound(m), 0.4))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ZombieHorse.isFood` is `#minecraft:zombie_horse_food`, which holds only
    /// the red mushroom, and `AbstractHorse.isFood` is `#minecraft:horse_food`,
    /// which holds none (#8593). A skeleton horse inherits the horse food tag.
    #[test]
    fn only_a_zombie_horse_eats_a_red_mushroom() {
        assert_eq!(
            eating(Species::ZombieHorse, "minecraft:red_mushroom"),
            Some((3.0, 0, 3, false)),
            "a zombie horse takes the red mushroom"
        );
        for item in ["minecraft:wheat", "minecraft:apple", "minecraft:hay_block"] {
            assert_eq!(eating(Species::ZombieHorse, item), None, "{item}");
        }
        for species in [
            Species::Horse,
            Species::Donkey,
            Species::Mule,
            Species::SkeletonHorse,
        ] {
            assert_eq!(
                eating(species, "minecraft:red_mushroom"),
                None,
                "{species:?}: horse food has no red mushrooms"
            );
            assert_eq!(
                eating(species, "minecraft:wheat"),
                Some((2.0, 20, 3, false)),
                "{species:?}: wheat is horse food"
            );
        }
        assert_eq!(
            eating(Species::Camel, "minecraft:cactus"),
            Some((2.0, 10, 0, true))
        );
        assert_eq!(
            eating(Species::CamelHusk, "minecraft:rabbit_foot"),
            Some((2.0, 10, 0, false))
        );
    }

    #[test]
    fn the_mounts_that_throw_a_swimmer_off_are_the_dismounts_underwater_tag() {
        // data/minecraft/tags/entity_type/dismounts_underwater.json of 26.3.
        for species in [
            Species::Camel,
            Species::Chicken,
            Species::Donkey,
            Species::HappyGhast,
            Species::Horse,
            Species::Llama,
            Species::Mule,
            Species::Pig,
            Species::Ravager,
            Species::Spider,
            Species::Strider,
            Species::TraderLlama,
            Species::ZombieHorse,
        ] {
            assert!(dismounts_underwater(species), "{species:?}");
        }
        // Not in the tag: a skeleton horse swims with its rider, a camel husk is not the camel.
        for species in [Species::SkeletonHorse, Species::CamelHusk, Species::Cow] {
            assert!(!dismounts_underwater(species), "{species:?}");
        }
    }

    /// A mob of the prepared resources' type `kind` (None without them).
    fn mob(kind: &str) -> Option<horizon_client::mobs::Mob> {
        let root = std::env::var_os("HORIZON_RESOURCES")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../.local/horizon-client")
                    .join(horizon_core::VANILLA_VERSION)
            });
        let types = horizon_client::mobs::MobTypes::load(&root).ok()?;
        let t = std::sync::Arc::new(types.types.get(kind)?.clone());
        Some(horizon_client::mobs::Mob::new(
            7,
            t,
            horizon_core::geometry::Vec3::new(1.5, 64.0, -2.5),
            0.0,
        ))
    }

    /// `ItemStack.hurtAndConvertOnBreak`: the seven points a pig's boost
    /// wears break a carrot on a stick at 25, and what it leaves is a fishing
    /// rod that kept the stick's components - not a fresh plain rod.
    #[test]
    fn a_steering_stick_that_breaks_leaves_a_fishing_rod() {
        let mut stick = ItemStack::new("minecraft:carrot_on_a_stick", 1);
        stick.data_mut().custom_name = Some(horizon_world::nbt::Tag::String("Old Nag".into()));
        // One point short of the break: the caller wears it and it holds.
        stick.damage = 17;
        assert!(stick_on_break(&stick, 7, 25).is_none());
        // 18 + 7 is the break: the rod it leaves is whole and keeps the name.
        stick.damage = 18;
        let rod = stick_on_break(&stick, 7, 25).expect("the wear breaks it");
        assert_eq!(&*rod.item, "minecraft:fishing_rod");
        assert_eq!(rod.count, 1);
        assert_eq!(rod.damage, 0, "the rod starts whole");
        assert_eq!(
            rod.custom_name().as_deref(),
            Some("Old Nag"),
            "the components carry over"
        );
        // Wear Unbreaking took away breaks nothing, and a stack with no
        // durability at all never breaks.
        assert!(stick_on_break(&stick, 0, 25).is_none());
        assert!(stick_on_break(&stick, 25, 0).is_none());
    }

    /// `handlePlayerCommand` START_RIDING_JUMP reaches the ridden vehicle's
    /// `handleStartJump`: a saddled horse rears, slides on the rider's keys and
    /// its jump sound is played at 0.4; a donkey and a mule have their own
    /// sound; a camel dashes with its dash sound at 1.0. An unsaddled mount
    /// (`canJump` false) or a scale of zero takes nothing.
    #[test]
    fn a_released_jump_bar_starts_the_mounts_own_jump() {
        let Some(mut horse) = mob("minecraft:horse") else {
            return;
        };
        // Unsaddled: AbstractHorse.canJump is isSaddled.
        assert_eq!(start_riding_jump(&mut horse, 100), None);
        assert_eq!(horse.extra.counter, 0, "no rear without a jump");
        assert!(!horse.extra.stand_sliding);
        horse.set_saddle(Some(horizon_client::items::ItemStack::new(
            "minecraft:saddle",
            1,
        )));
        // A scale of zero is not a jump (`data > 0`).
        assert_eq!(start_riding_jump(&mut horse, 0), None);
        assert_eq!(horse.extra.counter, 0);
        assert_eq!(
            start_riding_jump(&mut horse, 100),
            Some(("entity.horse.jump", 0.4))
        );
        assert_eq!(
            horse.extra.counter, 20,
            "standIfPossible rears for 20 ticks"
        );
        assert!(horse.extra.stand_sliding, "onPlayerJump slides on the keys");
        for (kind, sound) in [
            ("minecraft:donkey", "entity.donkey.jump"),
            ("minecraft:mule", "entity.mule.jump"),
        ] {
            let Some(mut m) = mob(kind) else {
                continue;
            };
            m.set_saddle(Some(horizon_client::items::ItemStack::new(
                "minecraft:saddle",
                1,
            )));
            assert_eq!(start_riding_jump(&mut m, 55), Some((sound, 0.4)), "{kind}");
        }
        // A skeleton horse in water splashes instead.
        let Some(mut skeleton) = mob("minecraft:skeleton_horse") else {
            return;
        };
        skeleton.set_saddle(Some(horizon_client::items::ItemStack::new(
            "minecraft:saddle",
            1,
        )));
        skeleton.fluids.in_water = true;
        assert_eq!(
            start_riding_jump(&mut skeleton, 100),
            Some(("entity.skeleton_horse.jump_water", 0.4))
        );
    }

    /// `Camel.handleStartJump`: the dash sound, then DASH, which starts the
    /// cooldown; a sitting camel refuses to move and cannot jump
    /// (`Camel.canJump`).
    #[test]
    fn a_released_jump_bar_starts_a_camels_dash() {
        let Some(mut camel) = mob("minecraft:camel") else {
            return;
        };
        assert_eq!(start_riding_jump(&mut camel, 100), None, "no saddle");
        camel.set_saddle(Some(horizon_client::items::ItemStack::new(
            "minecraft:saddle",
            1,
        )));
        assert_eq!(
            start_riding_jump(&mut camel, 100),
            Some(("entity.camel.dash", 1.0))
        );
        assert!(camel.extra.flag2, "Camel.setDashing(true)");
        assert_eq!(
            camel.extra.anim_ticks, 0,
            "an age-0 camel starts no cooldown"
        );
        // Sitting out the ride: refuseToMove keeps the dash from starting.
        camel.extra.camel_sitting = true;
        camel.extra.flag2 = false;
        assert_eq!(start_riding_jump(&mut camel, 100), None);
        assert!(!camel.extra.flag2);
    }

    /// `AbstractNautilus.onPlayerJump` through `handlePlayerCommand`
    /// START_RIDING_JUMP: the scale the rider's own bar charged reaches the
    /// mount, and its next travel dashes with it. A nautilus in its dash
    /// cooldown takes no jump (the pending scale is left alone), and there is
    /// no sound for the server to play: the nautilus plays its dash sound
    /// itself when the dash runs.
    #[test]
    fn a_guests_released_jump_bar_arms_the_nautilus_dash() {
        let Some(mut m) = mob("minecraft:nautilus") else {
            return;
        };
        m.set_saddle(Some(horizon_client::items::ItemStack::new(
            "minecraft:saddle",
            1,
        )));
        // The guest's ride tick fills the rider; the packet arrives after it.
        m.rider = Some(equine::Rider::default());
        assert_eq!(start_riding_jump(&mut m, 75), None);
        let jump = m.rider.expect("the rider stays on").jump;
        // 0.4 + 0.4 * 75 / 90: a three-quarter bar.
        assert!((jump - 0.733_333_3).abs() < 1.0e-6, "{jump}");
        // A full bar is a scale of 1.
        assert_eq!(start_riding_jump(&mut m, 100), None);
        assert_eq!(m.rider.expect("the rider stays on").jump, 1.0);
        // The dash clock: a dash still cooling down refuses the jump, so the
        // scale already on the rider stays.
        m.extra.anim_ticks = 40;
        assert_eq!(start_riding_jump(&mut m, 30), None);
        assert_eq!(m.rider.expect("the rider stays on").jump, 1.0);
        // An unsaddled nautilus cannot jump at all (`AbstractNautilus.canJump`).
        m.extra.saddled = false;
        m.extra.anim_ticks = 0;
        assert_eq!(start_riding_jump(&mut m, 100), None);
        assert_eq!(m.rider.expect("the rider stays on").jump, 1.0);
        // The scale is spent by the mount's next travel: `executeRidersJump`
        // pushes along the rider's look, starts the 40-tick dash clock, raises
        // DASH and queues the dash sound.
        m.extra.saddled = true;
        equine::ridden_step(&mut m);
        assert_eq!(m.rider.expect("the rider stays on").jump, 0.0);
        assert_eq!(m.extra.anim_ticks, 40, "the dash clock");
        assert!(m.extra.flag2, "DASH");
        assert_eq!(m.extra.scratch[1], 1, "the dash sound plays next tick");
        assert!(
            m.velocity.z > 0.0 && m.velocity.x.abs() < 1.0e-9,
            "the dash goes along the rider's look (+z): {:?}",
            m.velocity
        );
    }

    #[test]
    fn steering_accepts_the_correct_item_in_either_hand() {
        let carrot = "minecraft:carrot_on_a_stick";
        let fungus = "minecraft:warped_fungus_on_a_stick";
        assert!(steering_item(Species::Pig, Some(carrot), None));
        assert!(steering_item(Species::Pig, None, Some(carrot)));
        assert!(steering_item(Species::Strider, None, Some(fungus)));
        assert!(!steering_item(Species::Pig, Some(fungus), None));
        assert!(!steering_item(Species::Horse, Some(carrot), Some(fungus)));
    }
}
