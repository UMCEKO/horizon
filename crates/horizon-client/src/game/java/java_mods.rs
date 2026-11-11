// game-module-cfg: feature java-mods
//! Java mods' items: their real `Item.use` runs in the embedded JVM on a view of the acting player,
//! and what it changed lands in Horizon's own player and world.
#![allow(dead_code)]

use super::*;
use commands::Target;
use horizon_core::nbt::{self, Limits, NamedTag, Tag};
use horizon_java_host::wire::{In, Out};

const INVENTORY: usize = 36;
const UNMAPPED: i32 = horizon_java_host::bridge::UNMAPPED;
const ARMOR: [usize; 4] = [3, 2, 1, 0];
const OFFHAND: usize = 40;

pub(super) fn write_tag(tag: Tag) -> Option<Vec<u8>> {
    let root = NamedTag {
        name: "".into(),
        tag,
    };
    nbt::write_named(&root, Limits::default()).ok()
}

pub(super) fn read_tag(bytes: &[u8]) -> Option<Tag> {
    nbt::read_named(bytes, Limits::default())
        .ok()
        .map(|(root, _)| root.tag)
}

impl Game<'_> {
    /// The slots of `Inventory.getItem`: main, boots to helmet, off hand.
    fn java_slot(&mut self, slot: usize) -> Option<&mut Option<ItemStack>> {
        match slot {
            0..INVENTORY => self.inventory.get_mut(slot),
            OFFHAND => Some(&mut self.offhand),
            _ => self
                .armor
                .get_mut(*ARMOR.get(slot.checked_sub(INVENTORY)?)?),
        }
    }

    /// `ServerPlayerGameMode.useItem` of an item a Java mod registered.
    pub(super) fn java_use_item(&mut self, stack: &ItemStack, claimed: &mut bool) {
        self.java_use(stack, None, claimed);
    }

    /// `ItemStack.useOn`, the same way, once the block let the click through.
    pub(super) fn java_use_item_on(&mut self, stack: &ItemStack, hit: &Hit, claimed: &mut bool) {
        self.java_use(stack, Some(hit), claimed);
    }

    fn java_use(&mut self, stack: &ItemStack, hit: Option<&Hit>, claimed: &mut bool) {
        let Some(host) = horizon_java_host::get() else {
            return;
        };
        if !host.item_code_runs(&stack.item, hit.is_some()) {
            // Vanilla code's use is Horizon's, from the item's components, but mods hear it first and may cancel it.
            if hit.is_none() {
                self.java_call_use(&stack.item, "event_use_item", Vec::new(), claimed);
            }
            return;
        }
        let using = match hit {
            Some(hit) => self.java_use_on(&stack.item, hit, "use_item_on", claimed),
            None => self.java_call_use(&stack.item, "use_item", Vec::new(), claimed),
        };
        // `startUsingItem` in Java: Horizon's player holds the use, and each tick goes on with it there.
        if let Some(ticks) = using {
            self.using = Some((self.held_slot(), ticks));
        }
    }

    /// `LivingEntity.updatingUsingItem` for an item whose use is its mod's code; false leaves the tick to Horizon.
    pub(super) fn java_tick_using(&mut self, slot: usize, ticks: u32, stack: &ItemStack) -> bool {
        if !horizon_java_host::get().is_some_and(|h| h.item_code_runs(&stack.item, false)) {
            return false;
        }
        let using = self.java_call_using(stack, ticks, "use_tick");
        self.using = using.map(|ticks| (slot, ticks));
        true
    }

    /// `LivingEntity.releaseUsingItem` on the acting player: the item's release, its release tick, then `stopUsingItem`.
    pub(super) fn java_release_using_item(&mut self) {
        let Some((slot, ticks)) = self.using else {
            return;
        };
        let Some(hand) = self.used_hand() else {
            return;
        };
        let stack = self.stack_in(slot).cloned();
        self.with_hand(hand, |g| match &stack {
            // `release_used_item` ends the use and sends its vibration, as `stopUsingItem` does.
            Some(stack) => {
                g.release_used_item(stack, ticks, false);
                if &*stack.item == "minecraft:crossbow" {
                    g.tick_crossbow(slot, ticks, stack);
                }
            }
            None => g.using = None,
        });
        self.java_clear_use();
    }

    /// The Java view forgets its held use once native has ended it.
    fn java_clear_use(&mut self) {
        let [a, b, c, d] = self.acting_uuid();
        let mut out = Out::default();
        out.long(((a as i64) << 32) | (b as u32 as i64));
        out.long(((c as i64) << 32) | (d as u32 as i64));
        if let Some(host) = horizon_java_host::get() {
            horizon_java_host::bridge::with_active(self, || host.request("clear_use", &out.0));
        }
    }

    /// `releaseUsingItem` (or `stopUsingItem` when the hand changed) for such an item; false leaves it to Horizon.
    pub(super) fn java_release_using(
        &mut self,
        stack: &ItemStack,
        ticks: u32,
        hand_changed: bool,
    ) -> bool {
        if !horizon_java_host::get().is_some_and(|h| h.item_code_runs(&stack.item, false)) {
            return false;
        }
        let op = if hand_changed {
            "stop_using"
        } else {
            "release_using"
        };
        self.java_call_using(stack, ticks, op);
        true
    }

    fn java_call_using(&mut self, stack: &ItemStack, ticks: u32, op: &str) -> Option<u32> {
        let mut out = Out::default();
        out.int(ticks as i32);
        let mut claimed = false;
        self.java_call_use(&stack.item, op, out.0, &mut claimed)
    }

    /// `ItemStack.interactLivingEntity` of an item whose own code answers it, once the mob aimed at passed (`Player.interactOn`).
    pub(super) fn java_interact_living(&mut self) -> bool {
        let Some(host) = horizon_java_host::get() else {
            return false;
        };
        let Some(stack) = self.held().cloned() else {
            return false;
        };
        if !host.item_entity_code(&stack.item) {
            return false;
        }
        let Some((index, ..)) = self.mob_hit() else {
            return false;
        };
        let mut out = Out::default();
        out.int(self.mobs[index].id as i32);
        let mut claimed = false;
        self.java_call_use(&stack.item, "interact_entity", out.0, &mut claimed);
        if claimed {
            self.swing_arm();
        }
        claimed
    }

    fn java_use_on(&mut self, what: &str, hit: &Hit, op: &str, claimed: &mut bool) -> Option<u32> {
        let mut out = Out::default();
        out.long(BlockPos::new(hit.pos[0], hit.pos[1], hit.pos[2]).pack());
        out.int(match hit.face {
            [0, -1, 0] => 0,
            [0, 1, 0] => 1,
            [0, 0, -1] => 2,
            [0, 0, 1] => 3,
            [-1, 0, 0] => 4,
            _ => 5,
        });
        out.double(hit.point[0]);
        out.double(hit.point[1]);
        out.double(hit.point[2]);
        self.java_call_use(what, op, out.0, claimed)
    }

    /// One use call; the ticks the player has been using an item when the call leaves it using one.
    fn java_call_use(
        &mut self,
        what: &str,
        op: &str,
        tail: Vec<u8>,
        claimed: &mut bool,
    ) -> Option<u32> {
        self.java_bind_states();
        let mut request = self.java_use_request()?;
        request.extend(tail);
        let reply = self.java_request(op, &request)?;
        let using = self.java_apply_use(&reply, claimed);
        if using.is_none() {
            log::warn!("Horizon: a Java mod's use of {what} answered in pieces");
        }
        self.java_drain_block_events();
        using.flatten()
    }

    /// Fills the acting player's view again, for a bridge that changed something its getters read.
    pub(super) fn java_refresh_player(&mut self) {
        if let (Some(host), Some(request)) = (horizon_java_host::get(), self.java_use_request()) {
            horizon_java_host::bridge::with_active(self, || host.request("refresh", &request));
        }
    }

    /// Java's block state ids against this game's, once.
    pub(super) fn java_bind_states(&mut self) {
        use horizon_java_host::bridge::{STATES, StateMap};
        let Some(host) = horizon_java_host::get() else {
            return;
        };
        if STATES.get().is_some() {
            return;
        }
        let Some(listed) =
            horizon_java_host::bridge::with_active(self, || host.request("states", &[]))
        else {
            return;
        };
        let registry = &self.pack.assets.registry;
        let mut map = StateMap {
            to_horizon: Vec::new(),
            to_java: Vec::new(),
            java_air: 0,
            owned: Vec::new(),
            entity: Vec::new(),
            animate: Vec::new(),
            animate_whole: false,
            animate_any: false,
        };
        let mut listed = In(&listed);
        let (Some(count), Some(air), Some(whole)) = (listed.int(), listed.int(), listed.bool())
        else {
            return;
        };
        map.java_air = air;
        map.animate_whole = whole;
        map.to_horizon.resize(count as usize, None);
        map.to_java.resize(registry.len(), UNMAPPED);
        map.owned.resize(registry.len(), false);
        map.entity.resize(registry.len(), false);
        map.animate.resize(registry.len(), 0);
        let mut missing: std::collections::BTreeMap<String, usize> = Default::default();
        for _ in 0..count {
            let read = (|| {
                let java = listed.int()?;
                let name = listed.text()?.to_string();
                let mut properties = Vec::new();
                for _ in 0..listed.int()? {
                    properties.push((listed.text()?.to_string(), listed.text()?.to_string()));
                }
                let entity = listed.bool()?;
                let owned = listed.bool()?;
                let animate = listed.byte()?;
                Some((java, name, properties, entity, owned, animate))
            })();
            let Some((java, name, properties, entity, owned, animate)) = read else {
                log::error!(
                    "Horizon: Java's block state list ended early; Java mods' blocks are unbound"
                );
                return;
            };
            let state = horizon_world::chunk::BlockState {
                name,
                properties: properties.into_iter().collect(),
            };
            let Ok(id) = registry.resolve(&state) else {
                *missing.entry(state.name).or_default() += 1;
                continue;
            };
            if let Some(cell) = map.to_horizon.get_mut(java as usize) {
                *cell = Some(id);
            }
            let at = id.0 as usize;
            if map.to_java.len() <= at {
                map.to_java.resize(at + 1, UNMAPPED);
                map.owned.resize(at + 1, false);
                map.entity.resize(at + 1, false);
                map.animate.resize(at + 1, 0);
            }
            map.to_java[at] = java;
            map.owned[at] = owned;
            map.entity[at] = entity;
            map.animate[at] = animate;
        }
        let animated = map.animate.iter().filter(|a| **a != 0).count();
        map.animate_any = animated > 0 || map.animate_whole;
        if animated > 0 {
            log::info!("Horizon: {animated} block states run animateTick in Java");
        }
        if !missing.is_empty() {
            let blocks: Vec<String> = missing.iter().map(|(b, n)| format!("{b} ({n})")).collect();
            log::error!(
                "Horizon: {} Java block states have no state in Horizon's registry, so they never cross between the two; carry their mods with oracle facts: {}",
                missing.values().sum::<usize>(),
                blocks.join(", ")
            );
        }
        let mut unmatched: std::collections::BTreeMap<&str, usize> = Default::default();
        for (i, java) in map.to_java.iter().enumerate() {
            if *java == UNMAPPED
                && let Some(state) = registry.state(BlockStateId(i as u32))
            {
                *unmatched.entry(state.name.as_str()).or_default() += 1;
            }
        }
        if !unmatched.is_empty() {
            let blocks: Vec<String> = unmatched
                .iter()
                .map(|(b, n)| format!("{b} ({n})"))
                .collect();
            log::error!(
                "Horizon: {} of Horizon's block states have no Java state; a Java mod reading one fails: {}",
                unmatched.values().sum::<usize>(),
                blocks.join(", ")
            );
        }
        let _ = STATES.set(map);
    }

    pub(in crate::game) fn java_use_request(&mut self) -> Option<Vec<u8>> {
        let position = self.player.as_ref()?.position;
        let [a, b, c, d] = self.acting_uuid();
        let mut out = Out::default();
        out.long(((a as i64) << 32) | (b as u32 as i64));
        out.long(((c as i64) << 32) | (d as u32 as i64));
        out.text(
            &self
                .lan_acting_name
                .clone()
                .unwrap_or_else(|| self.options.player_name()),
        );
        out.text(self.dim.id());
        out.long(self.world.game_time);
        out.text(self.player.as_ref()?.pose.name());
        out.double(position.x);
        out.double(position.y);
        out.double(position.z);
        out.float(self.yaw);
        out.float(self.pitch);
        out.bool(self.keys.held(horizon_client::actions::Action::Sneak));
        out.int(self.mode as i32);
        out.int(matches!(self.hand, Hand::Off) as i32);
        out.int(self.selected as i32);
        out.int(self.experience.level);
        out.float(self.experience.progress);
        out.int(self.experience.total);
        let player = self.player.as_ref()?;
        out.float(player.health.health());
        out.int(player.health.food.food);
        out.float(player.health.food.saturation);
        out.float(player.health.food.exhaustion);
        out.float(player.health.absorption());
        let effects = &self.potions.effects.list;
        out.int(effects.len() as i32);
        for effect in effects {
            out.text(&format!("minecraft:{}", effect.id));
            out.int(effect.duration);
            out.int(effect.amplifier);
            out.bool(effect.ambient);
            out.bool(effect.visible);
        }
        out.double(player.velocity.x);
        out.double(player.velocity.y);
        out.double(player.velocity.z);
        out.bool(player.ground.on_ground);
        let mut stacks = Vec::new();
        for slot in 0..=OFFHAND {
            if let Some(Some(stack)) = self.java_slot(slot).map(|s| s.clone()) {
                let tag = horizon_client::save::item_to_nbt(&stack);
                stacks.push((slot, write_tag(tag)?));
            }
        }
        out.int(stacks.len() as i32);
        for (slot, bytes) in stacks {
            out.int(slot as i32);
            out.bytes(&bytes);
        }
        Some(out.0)
    }

    fn java_apply_use(&mut self, reply: &[u8], claimed: &mut bool) -> Option<Option<u32>> {
        let mut reply = In(reply);
        *claimed = reply.bool()?;
        let using = reply.bool()?;
        let ticks = reply.int()?;
        self.experience = horizon_client::entities::Experience {
            level: reply.int()?,
            progress: reply.float()?,
            total: reply.int()?,
        };
        for _ in 0..reply.int()? {
            let slot = reply.int()? as usize;
            let bytes = reply.bytes()?;
            let stack = if bytes.is_empty() {
                None
            } else {
                horizon_client::save::item_from_nbt(&read_tag(bytes)?)
            };
            *self.java_slot(slot)? = stack;
            self.hand_stack_replaced(if slot == OFFHAND {
                shield::OFFHAND_SLOT
            } else {
                slot
            });
        }
        self.java_apply_effects(&mut reply)?;
        Some(using.then_some(ticks.max(0) as u32))
    }

    /// What Java did to the world on a call: the sounds it played.
    pub(super) fn java_apply_effects(&mut self, reply: &mut In<'_>) -> Option<()> {
        for _ in 0..reply.int()? {
            let sound = reply.text()?;
            let at = [reply.double()?, reply.double()?, reply.double()?];
            let (volume, pitch) = (reply.float()?, reply.float()?);
            self.audio.play(sound, Some(at), volume, pitch);
        }
        Some(())
    }

    /// Calls `GameBridge`'s `op` for this level, with what it does to the world applied after.
    fn java_level_call(&mut self, op: &str, body: impl FnOnce(&mut Out)) {
        let Some(host) = horizon_java_host::get() else {
            return;
        };
        self.java_bind_states();
        self.java_push_stale();
        let (dim, time) = (self.dim.id(), self.world.game_time);
        let effects = horizon_java_host::bridge::with_active(self, || {
            host.call_with(
                op,
                |out| {
                    out.text(dim);
                    out.long(time);
                    body(out);
                },
                // Nothing changed is the common reply: no sounds.
                |reply| Some((reply.0 != [0u8; 4]).then(|| reply.0.to_vec())),
            )
        });
        if let Some(Some(reply)) = effects {
            self.java_apply_effects(&mut In(&reply));
        }
    }

    /// One request into Java's server side, after the block entities Horizon changed since Java last heard reached it.
    pub(super) fn java_request(&mut self, op: &str, input: &[u8]) -> Option<Vec<u8>> {
        let host = horizon_java_host::get()?;
        self.java_push_stale();
        horizon_java_host::bridge::with_active(self, || host.request(op, input))
    }

    /// What Java holds of a block entity Horizon changed on its own: the new saved form, or its removal.
    pub(in crate::game) fn java_push_stale(&mut self) {
        for key in self.world.block_entities.take_java_stale() {
            let pos = BlockPos::new(key.0, key.1, key.2);
            match self.world.block_entities.get(&key) {
                Some(BlockEntity::Mod(_)) => self.world.block_entities.release_for_java(&key),
                Some(_) => self.java_send_block_entity(pos),
                None => {
                    self.world.block_entities.release_for_java(&key);
                    self.java_level_call("block_entity_drop", |out| out.long(pos.pack()));
                }
            }
        }
    }

    /// Mods' blocks changed, were ticked or heard a neighbour since the last call: their Java runs now.
    pub(super) fn java_drain_block_events(&mut self) {
        let events = BLOCK_EVENTS.take();
        if events.is_empty() {
            return;
        }
        self.java_level_call("block_events", |out| {
            out.int(events.len() as i32);
            for event in &events {
                out.int(event.kind as i32);
                out.long(event.pos.pack());
                out.int(event.a);
                out.int(event.b);
                out.int(event.c);
            }
        });
    }

    /// A target's id among this level's entities, which `Target`s have all but for players.
    fn java_target_id(target: Target) -> Option<u32> {
        match target {
            Target::Player | Target::Guest(_) => None,
            Target::Mob(id)
            | Target::Item(id)
            | Target::Orb(id)
            | Target::Projectile(id)
            | Target::Stand(id)
            | Target::Frame(id)
            | Target::Painting(id)
            | Target::Boat(id)
            | Target::Cart(id)
            | Target::Other(id)
            | Target::Crystal(id)
            | Target::Dragon(id)
            | Target::Fangs(id)
            | Target::Tnt(id)
            | Target::Bolt(id)
            | Target::Falling(id)
            | Target::Rocket(id)
            | Target::Trident(id)
            | Target::Spawner(id)
            | Target::Cloud(id)
            | Target::DragonPart(id)
            | Target::Cushion(id)
            | Target::Hook(id)
            | Target::Knot(id) => Some(id),
        }
    }

    pub(in crate::game) fn java_target(&mut self, id: u32) -> Option<Target> {
        self.level_candidates()
            .into_iter()
            .find(|t| Self::java_target_id(*t) == Some(id))
    }

    /// `level.getBlockEntity` found nothing in Java: the saved form Horizon holds, if any, goes there now.
    pub(in crate::game) fn java_send_block_entity(&mut self, pos: BlockPos) {
        let key = (pos.x, pos.y, pos.z);
        let Some(entity) = self.world.block_entities.get(&key) else {
            return;
        };
        if matches!(entity, BlockEntity::Mod(m) if m.id().is_empty()) {
            return;
        }
        let name = self
            .res
            .facts()
            .get(self.world.block(pos.x, pos.y, pos.z))
            .name
            .clone();
        let tag = horizon_client::save::encode_block_entity(key, entity, &name);
        let Some(bytes) = write_tag(tag) else {
            return;
        };
        self.world.block_entities.hold_for_java(key);
        self.java_level_call("block_entity_load", |out| {
            out.long(pos.pack());
            out.bytes(&bytes);
        });
    }

    /// `BlockEntities.store`: a vanilla block entity Java changed, written through to Horizon's chunk.
    pub(in crate::game) fn java_store_block_entity(&mut self, pos: BlockPos, tag: &[u8]) {
        let Some((key, entity)) =
            read_tag(tag).and_then(|t| horizon_client::save::decode_block_entity(&t))
        else {
            log::warn!("Horizon: a Java mod's block entity at {pos:?} didn't decode");
            return;
        };
        self.world.block_entities.insert_from_java(key, entity);
        self.world.modified.insert((key.0 >> 4, key.2 >> 4));
    }

    /// `data get block` of a block entity Java holds: its live saved form replaces the chunk's copy first.
    pub(in crate::game) fn java_refresh_block_entity(&mut self, pos: BlockPos) {
        self.java_bind_states();
        let mut out = Out::default();
        out.text(self.dim.id());
        out.long(self.world.game_time);
        out.long(pos.pack());
        let reply = self.java_request("block_entity_data", &out.0);
        let Some(tag) = reply
            .filter(|r| !r.is_empty())
            .and_then(|r| In(&r).bytes().and_then(read_tag))
        else {
            return;
        };
        if let Some((key, entity)) = horizon_client::save::decode_block_entity(&tag) {
            self.world.block_entities.insert(key, entity);
        }
    }

    /// `data merge`/`modify` of a block entity Java holds: it reads the new form, as vanilla's does.
    pub(in crate::game) fn java_set_block_entity(
        &mut self,
        pos: BlockPos,
        tag: horizon_core::nbt::Tag,
    ) {
        let Some(bytes) = write_tag(tag) else {
            return;
        };
        self.java_level_call("block_entity_set", |out| {
            out.long(pos.pack());
            out.bytes(&bytes);
        });
    }

    /// NeoForge's `VanillaInventoryCodeHooks` for a hopper and the item handler of the block at `target`, if it has one.
    pub(in crate::game) fn java_hopper(
        &mut self,
        hopper: (i32, i32, i32),
        target: (i32, i32, i32),
        side: horizon_core::geometry::Direction,
        insert: bool,
    ) -> Option<bool> {
        horizon_java_host::get()?;
        self.java_bind_states();
        if !horizon_java_host::bridge::java_block(self.world.block(target.0, target.1, target.2)) {
            return None;
        }
        let pos = |p: (i32, i32, i32)| BlockPos::new(p.0, p.1, p.2).pack();
        let mut out = Out::default();
        out.text(self.dim.id());
        out.long(self.world.game_time);
        out.long(pos(hopper));
        out.long(pos(target));
        out.int(side as i32);
        out.bool(insert);
        let reply = self.java_request("hopper_transfer", &out.0)?;
        let mut reply = In(&reply);
        let (found, moved) = (reply.bool()?, reply.bool()?);
        self.java_apply_effects(&mut reply);
        found.then_some(moved)
    }

    /// Java made a block entity: its place in the chunk, which saves it and gives it its turn to tick.
    pub(in crate::game) fn java_adopt_block_entity(&mut self, pos: BlockPos, id: String) {
        let entity = BlockEntity::Mod(Box::new(horizon_client::containers::ModBlockEntity::new(
            id,
            horizon_client::containers::ModBlockEntityFields::default(),
        )));
        let key = (pos.x, pos.y, pos.z);
        self.world.block_entities.release_for_java(&key);
        self.world.block_entities.insert(key, entity);
    }

    /// `BlockEntityTicker.tick` of a block entity Java holds, which it took when its chunk loaded.
    pub(super) fn java_block_entity_tick(&mut self, pos: [i32; 3]) {
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        if !self.java_tick_once(at) {
            thread_local! {
                static WARNED: std::cell::RefCell<std::collections::HashSet<BlockPos>> = Default::default();
            }
            if WARNED.with_borrow_mut(|w| w.insert(at)) {
                log::warn!(
                    "Horizon: no Java block entity at {pos:?} for Horizon's record of one, so it doesn't tick"
                );
            }
        }
        self.java_drain_block_events();
    }

    /// Columns that joined and left this level: Java takes the joined ones' block entities as `LevelChunk.postProcessGeneration` promotes them, and lets the others' go as `ServerLevel.unload` does, handing back what they hadn't saved.
    pub(super) fn java_chunks_changed(
        &mut self,
        loaded: &[(i32, i32)],
        unloaded: &[Arc<horizon_client::chunk::Chunk>],
    ) {
        // Servers' configs load with `ServerLifecycleHooks`, so vanilla's chunks only load after it; this level's earlier ones catch up here.
        let gone: horizon_client::fast_hash::FxHashSet<(i32, i32)> =
            unloaded.iter().map(|c| (c.x, c.z)).collect();
        self.world.block_entities.release_columns_for_java(&gone);
        if self.remote.is_some() || !JAVA_SERVER.with_borrow(|s| s.started) {
            return;
        }
        let behind = JAVA_SERVER.with_borrow_mut(|s| s.levels.insert(self.dim.id().to_owned()));
        if loaded.is_empty() && unloaded.is_empty() && !behind {
            return;
        }
        if horizon_java_host::get().is_none() {
            return;
        }
        self.java_bind_states();
        // ChunkMap.scheduleUnload: a leaving column is out of the visible map before its save and unload.
        UNLOADING.set(gone);
        let joined: horizon_client::fast_hash::FxHashSet<(i32, i32)> = if behind {
            self.world.chunks.keys().copied().collect()
        } else {
            loaded.iter().copied().collect()
        };
        self.java_chunks_leaving(unloaded);
        self.java_chunks_read(&joined);
        let mut entities: Vec<(BlockPos, Vec<u8>)> = Vec::new();
        if !joined.is_empty() {
            for (&key, entity) in self.world.block_entities.iter() {
                let BlockEntity::Mod(m) = entity else {
                    continue;
                };
                let state = self.world.block(key.0, key.1, key.2);
                if m.id().is_empty()
                    || !joined.contains(&(key.0 >> 4, key.2 >> 4))
                    || !horizon_java_host::bridge::java_block(state)
                {
                    continue;
                }
                let name = &self.res.facts().get(state).name;
                let tag = horizon_client::save::encode_block_entity(key, entity, name);
                if let Some(bytes) = write_tag(tag) {
                    entities.push((BlockPos::new(key.0, key.1, key.2), bytes));
                }
            }
        }
        let mut out = Out::default();
        out.text(self.dim.id());
        out.long(self.world.game_time);
        out.int(entities.len() as i32);
        for (pos, bytes) in &entities {
            out.long(pos.pack());
            out.bytes(bytes);
        }
        out.int(unloaded.len() as i32);
        for chunk in unloaded {
            out.int(chunk.x);
            out.int(chunk.z);
        }
        let reply = self.java_request("chunks_changed", &out.0);
        UNLOADING.take();
        let Some(reply) = reply else {
            return;
        };
        let mut reply = In(&reply);
        for _ in 0..reply.int().unwrap_or(0) {
            let Some(tag) = reply.bytes().and_then(read_tag) else {
                return;
            };
            if let Some((key, entity)) = horizon_client::save::decode_block_entity(&tag) {
                self.world.modified.insert((key.0 >> 4, key.2 >> 4));
                self.world.block_entities.insert(key, entity);
            }
        }
        self.java_apply_effects(&mut reply);
        self.java_drain_block_events();
        self.java_chunks_full(&joined);
    }

    /// `MinecraftServer.stopServer`'s first step: `ServerStopping`, before the players leave.
    pub(super) fn java_server_stopping(&mut self) {
        if self.remote.is_none() && JAVA_SERVER.with_borrow(|s| s.started) {
            self.java_level_call("server_stopping", |_| {});
        }
    }

    /// `PlayerList.removeAll`: the players still in Java's server log out.
    pub(super) fn java_players_removed(&mut self) {
        if self.remote.is_none() && JAVA_SERVER.with_borrow(|s| s.started) {
            self.java_level_call("players_removed", |_| {});
        }
    }

    /// `ServerLevel.unload` of every column of the level in place, as `MinecraftServer.stopServer` does before `saveAllChunks`.
    pub(super) fn java_level_closing(&mut self) {
        if !self.java_level_running() {
            return;
        }
        let all: Vec<Arc<horizon_client::chunk::Chunk>> =
            self.world.chunks.values().cloned().collect();
        self.java_chunks_changed(&[], &all);
    }

    /// Whether this level's chunks are the Java server's: it started, and the level caught up on its chunks.
    pub(super) fn java_level_running(&self) -> bool {
        self.remote.is_none()
            && JAVA_SERVER.with_borrow(|s| s.started && s.levels.contains(self.dim.id()))
    }

    /// What changed in the players' views, chunks and pairings, for the `ChunkMap` in Java: views and resets, then the chunks watched and unwatched, then pairings and departures.
    pub(super) fn java_tracking(
        &mut self,
        events: Vec<horizon_client::lan::hub::Tracking>,
        watches: &[horizon_client::lan::chunk_watch::LevelWatches],
    ) {
        let records = tracking_records(events, watches);
        if records.is_empty() {
            return;
        }
        self.java_level_call("tracking", |out| {
            out.int(records.len() as i32);
            for record in &records {
                record.write(out);
            }
        });
    }

    /// The hub's chunk watches run for the native mods first, then Java hears them with the views and pairings; the pairings stay for `run_native_tracking`.
    pub(super) fn java_lan_tracking(&mut self) {
        if !horizon_client::lan::chunk_watch::java_side() {
            return;
        }
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let watches = host.hub.chunk_watches.take();
        host.hub.chunk_watches =
            horizon_client::lan::chunk_watch::ChunkWatchLog::new(watches.clone());
        self.run_chunk_watches();
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let events = host.hub.take_tracking();
        self.java_tracking(events, &watches);
        self.java_wire_flush_lan();
    }

    /// `ServerChunkCache.getChunk` with `load`: a ticket on the chunk and the server thread waiting for it, as `managedBlock` does; streaming's unloads wait for the next `update_world`.
    pub(in crate::game) fn java_load_chunk(&mut self, x: i32, z: i32) -> bool {
        let key = (x, z);
        if self.world.chunks.contains_key(&key) {
            return true;
        }
        if self.streaming.paused {
            log::error!(
                "Horizon: a Java mod asked for chunk ({x}, {z}) while chunk streaming is paused"
            );
            return false;
        }
        let areas = self.streaming.extra.len();
        self.streaming.extra.push((key, 0));
        let began = std::time::Instant::now();
        while !self.world.chunks.contains_key(&key) && began.elapsed().as_secs() < 60 {
            let (uploads, unloaded) = self.streaming.update(&mut self.world);
            self.streaming.free(uploads);
            PUMPED_UNLOADS.with_borrow_mut(|pumped| pumped.extend(unloaded));
            if !self.world.chunks.contains_key(&key) {
                self.streaming.wait(std::time::Duration::from_millis(5));
            }
        }
        self.streaming.extra.truncate(areas);
        // Vanilla's ticket lasts a tick; the next update keeps it too.
        self.spawn_wait.push(key);
        let loaded = self.world.chunks.contains_key(&key);
        if !loaded {
            log::error!(
                "Horizon: a Java mod's getChunk({x}, {z}) gave up after a minute of waiting for it"
            );
        }
        loaded
    }

    /// False when Java doesn't hold this block entity yet.
    fn java_tick_once(&mut self, at: BlockPos) -> bool {
        if horizon_java_host::get().is_none() {
            return true;
        }
        let mut out = Out::default();
        out.text(self.dim.id());
        out.long(self.world.game_time);
        out.long(at.pack());
        let reply = self.java_request("block_entity_tick", &out.0);
        let Some(reply) = reply else {
            return true;
        };
        let mut reply = In(&reply);
        let held = reply.bool().unwrap_or(true);
        self.java_apply_effects(&mut reply);
        held
    }

    /// `ServerPlayerGameMode.useItemOn` for a mod's block, `RightClickBlock` for a vanilla one: Some(true) when the block took the click, Some(false) when a listener canceled it.
    pub(super) fn java_click_block(&mut self, hit: &Hit) -> Option<bool> {
        horizon_java_host::get()?;
        let state = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
        let mut claimed = false;
        if horizon_java_host::bridge::java_block(state) {
            self.java_use_on("a block", hit, "click_block", &mut claimed);
            return claimed.then_some(true);
        }
        self.java_use_on("a block", hit, "event_click_block", &mut claimed);
        claimed.then_some(false)
    }

    /// `BreakBlockEvent`, where a player's break starts.
    pub(super) fn java_break_event(&mut self, pos: [i32; 3], canceled: &mut bool) {
        if horizon_java_host::get().is_some() {
            let mut out = Out::default();
            out.long(BlockPos::new(pos[0], pos[1], pos[2]).pack());
            self.java_call_use("a break", "event_break", out.0, canceled);
        }
    }

    /// `BlockState.getDestroyProgress` on the player's view, so its tool's and mods' break speeds run; the window asks its Java client.
    pub(super) fn java_destroy_progress(&mut self, pos: [i32; 3]) -> Option<f32> {
        if self.remote.is_some() {
            return self.java_client_destroy_progress(pos);
        }
        horizon_java_host::get()?;
        self.java_bind_states();
        let mut request = self.java_use_request()?;
        let mut out = Out(std::mem::take(&mut request));
        out.long(BlockPos::new(pos[0], pos[1], pos[2]).pack());
        let reply = self.java_request("destroy_progress", &out.0)?;
        In(&reply).float()
    }

    /// `Player.hasCorrectToolForDrops(state, level, pos)` on the player's view, with NeoForge's harvest check.
    pub(super) fn java_correct_tool(&mut self, pos: [i32; 3]) -> Option<bool> {
        if self.remote.is_some() {
            return None;
        }
        horizon_java_host::get()?;
        self.java_bind_states();
        let mut out = Out(self.java_use_request()?);
        out.long(BlockPos::new(pos[0], pos[1], pos[2]).pack());
        let reply = self.java_request("correct_tool", &out.0)?;
        In(&reply).byte().map(|b| b != 0)
    }

    /// `ServerPlayerGameMode.destroyBlock` after the break event, for a mod's block: Java removes it, spends the tool and drops; false leaves it to Horizon.
    pub(super) fn java_destroy_block(&mut self, pos: [i32; 3], state: BlockStateId) -> bool {
        let Some(host) = horizon_java_host::get() else {
            return false;
        };
        if !horizon_java_host::bridge::java_block(state) {
            return false;
        }
        let own_mine = self
            .held()
            .is_some_and(|stack| host.item_mine_code(&stack.item));
        let mut out = Out::default();
        out.long(BlockPos::new(pos[0], pos[1], pos[2]).pack());
        out.bool(own_mine);
        let mut claimed = false;
        self.java_call_use("a broken block", "destroy_block", out.0, &mut claimed);
        if !own_mine && !self.creative && self.world.block(pos[0], pos[1], pos[2]) != state {
            let facts = self.res.facts().get(state).clone();
            self.mine_block_with_held(&facts.name, facts.hardness);
        }
        true
    }

    /// `ItemStack.mineBlock` for a held item whose class has its own, after the block was removed; false leaves it to Horizon.
    pub(super) fn java_mine_block(&mut self, pos: [i32; 3], state: BlockStateId) -> bool {
        let Some(host) = horizon_java_host::get() else {
            return false;
        };
        let Some(stack) = self.held().cloned() else {
            return false;
        };
        if !host.item_mine_code(&stack.item) {
            return false;
        }
        self.java_bind_states();
        let Some(java) = horizon_java_host::bridge::STATES
            .get()
            .and_then(|map| map.to_java.get(state.0 as usize).copied())
        else {
            return false;
        };
        let mut out = Out::default();
        out.int(java);
        out.long(BlockPos::new(pos[0], pos[1], pos[2]).pack());
        let mut claimed = false;
        self.java_call_use(&stack.item, "mine_block", out.0, &mut claimed);
        true
    }

    /// `AttackEntityEvent`, where a player's attack starts.
    pub(super) fn java_attack_event(&mut self, entity: i32, canceled: &mut bool) {
        if horizon_java_host::get().is_some() {
            let mut out = Out::default();
            out.int(entity);
            self.java_call_use("an attack", "event_attack", out.0, canceled);
        }
    }

    /// The server's start and end, with NeoForge's real hooks.
    pub(super) fn java_world_phase(&mut self, open: bool) {
        let root = self.save.root.clone();
        let world = root
            .ancestors()
            .find(|dir| dir.join("level.dat").exists())
            .unwrap_or(&root)
            .to_string_lossy()
            .into_owned();
        self.java_level_call(if open { "server_start" } else { "server_stop" }, |out| {
            out.text(&world);
        });
        JAVA_SERVER.with_borrow_mut(|s| {
            s.started = open;
            s.levels.clear();
        });
        horizon_client::lan::chunk_watch::set_java_side(
            open && horizon_java_host::get().is_some(),
        );
        // Building every entity type reads the level through bridged natives (`getDifficulty`), so the game must be active.
        if open {
            horizon_java_host::bridge::with_active(self, super::java_content::entity_types);
        }
    }

    /// `ServerTickEvent`, and the server's queue of tasks mods gave it.
    pub(super) fn java_server_tick(&mut self, post: bool) {
        self.java_level_call("server_tick", |out| out.bool(post));
        self.java_drain_block_events();
        self.java_wire_flush_lan();
    }

    /// `LevelTickEvent`, with this level in place.
    pub(super) fn java_level_tick(&mut self, post: bool) {
        self.java_level_call("level_tick", |out| out.bool(post));
        self.java_drain_block_events();
        self.java_wire_flush_lan();
    }

    /// `PlayerLoggedOutEvent`, and that player's view gone.
    pub(super) fn java_player_left(&mut self, uuid: u128) {
        if horizon_java_host::get().is_none() {
            return;
        }
        let mut out = Out::default();
        out.long((uuid >> 64) as i64);
        out.long(uuid as i64);
        if let Some(reply) = self.java_request("player_left", &out.0) {
            self.java_apply_effects(&mut In(&reply));
        }
    }

    /// `PlayerTickEvent` and `Item.inventoryTick` of mods' stacks, for the player in place.
    pub(super) fn java_player_tick(&mut self) {
        if horizon_java_host::get().is_some() {
            let mut claimed = false;
            self.java_call_use("a player tick", "player_tick", Vec::new(), &mut claimed);
        }
    }

    /// Before a save: what each block entity Java holds would write, into the chunk's own record of it.
    pub(super) fn java_flush_block_entities(&mut self) {
        if horizon_java_host::get().is_none() {
            return;
        }
        let mut out = Out::default();
        out.text(self.dim.id());
        let reply = self.java_request("block_entities_save", &out.0);
        let Some(reply) = reply else {
            return;
        };
        let mut reply = In(&reply);
        for _ in 0..reply.int().unwrap_or(0) {
            let Some(tag) = reply.bytes().and_then(read_tag) else {
                return;
            };
            if let Some((key, entity)) = horizon_client::save::decode_block_entity(&tag) {
                self.world.block_entities.insert(key, entity);
            }
        }
    }
}

/// `kind`: what `GameBridge.blockEvents` runs for it.
pub(super) struct BlockEvent {
    kind: u8,
    pos: BlockPos,
    a: i32,
    b: i32,
    c: i32,
}

/// Whether Java's server started, and the levels whose loaded chunks Java has taken since.
#[derive(Default)]
struct JavaServer {
    started: bool,
    levels: std::collections::HashSet<String>,
}

thread_local! {
    static JAVA_SERVER: std::cell::RefCell<JavaServer> = Default::default();
    static BLOCK_EVENTS: std::cell::RefCell<Vec<BlockEvent>> = const { std::cell::RefCell::new(Vec::new()) };
    static WIRE_OUT: std::cell::RefCell<Vec<(u128, Vec<u8>)>> = const { std::cell::RefCell::new(Vec::new()) };
    static PUMPED_UNLOADS: std::cell::RefCell<Vec<Arc<horizon_client::chunk::Chunk>>> = const { std::cell::RefCell::new(Vec::new()) };
    static UNLOADING: std::cell::RefCell<horizon_client::fast_hash::FxHashSet<(i32, i32)>> = std::cell::RefCell::default();
}

impl Game<'_> {
    /// Whether Java sees this column loaded: Horizon holds it and it isn't being unloaded.
    pub(in crate::game) fn java_column_loaded(&self, x: i32, z: i32) -> bool {
        self.world.chunks.contains_key(&(x, z)) && !UNLOADING.with_borrow(|u| u.contains(&(x, z)))
    }
}

/// Columns a Java mod's `getChunk` unloaded while it waited, for `update_world` to save and drop.
pub(super) fn take_pumped_unloads() -> Vec<Arc<horizon_client::chunk::Chunk>> {
    PUMPED_UNLOADS.take()
}

fn java_id(state: BlockStateId) -> i32 {
    use horizon_java_host::bridge::Ret;
    state.into_raw()
}

fn push_event(kind: u8, pos: BlockPos, a: i32, b: i32, c: i32) {
    BLOCK_EVENTS.with_borrow_mut(|e| e.push(BlockEvent { kind, pos, a, b, c }));
}

/// A level's hook, inside `with_level`: `GameLevel` holds the world, so Java's calls back into it would alias it.
pub(super) fn note_block_changed(
    pos: BlockPos,
    old: BlockStateId,
    new: BlockStateId,
    flags: i32,
    block_changed: bool,
) {
    use horizon_java_host::bridge::{java_block, java_has_block_entity};
    if java_block(old)
        || java_block(new)
        || (block_changed && (java_has_block_entity(old) || java_has_block_entity(new)))
    {
        push_event(0, pos, java_id(old), java_id(new), flags);
    }
}

/// `BlockState.updateShape` of a mod's block: `direction` is Java's ordinal, the neighbour is at `pos` beside it.
pub(super) fn note_shape_update(
    pos: BlockPos,
    state: BlockStateId,
    direction: horizon_core::geometry::Direction,
    neighbour: BlockStateId,
) {
    use horizon_core::geometry::Direction::{Down, East, North, South, Up, West};
    if horizon_java_host::bridge::java_block(state) {
        let ordinal = match direction {
            Down => 0,
            Up => 1,
            North => 2,
            South => 3,
            West => 4,
            East => 5,
        };
        push_event(4, pos, java_id(state), ordinal, java_id(neighbour));
    }
}

/// `changed`: the default state of the block that changed beside, as Java's `neighborChanged` takes its block.
pub(super) fn note_neighbor_changed(
    pos: BlockPos,
    state: BlockStateId,
    changed: BlockStateId,
    moved: bool,
) {
    if horizon_java_host::bridge::java_block(state) {
        push_event(1, pos, java_id(state), moved as i32, java_id(changed));
    }
}

pub(super) fn note_block_tick(pos: BlockPos, state: BlockStateId, random: bool) {
    if horizon_java_host::bridge::java_block(state) {
        push_event(if random { 3 } else { 2 }, pos, java_id(state), 0, 0);
    }
}

/// A frame of the Java side for the Java client of a player in another process; the hub carries it at the next flush.
pub(super) fn java_wire_push(player: &str, frame: Vec<u8>) {
    if let Ok(uuid) = u128::from_str_radix(&player.replace('-', ""), 16) {
        WIRE_OUT.with_borrow_mut(|out| out.push((uuid, frame)));
    }
}

/// Sends what the Java side wrote for remote Java clients since the last flush, in the order it wrote it.
pub(super) fn java_wire_flush(hub: &horizon_client::lan::hub::Hub) {
    for (uuid, frame) in WIRE_OUT.take() {
        hub.send_java(uuid, frame);
    }
}

impl Game<'_> {
    /// A guest connected with Java mods of its own: its Java client hears of its login over its connection.
    pub(super) fn java_remote_join(&mut self, uuid: u128) {
        let Some(host) = horizon_java_host::get() else {
            return;
        };
        let mut out = Out::default();
        out.long((uuid >> 64) as i64);
        out.long(uuid as i64);
        horizon_java_host::bridge::with_active(self, || host.request("remote_join", &out.0));
    }

    /// A frame from a guest's Java client: the packet it wrote, for the player's listener at its next turn.
    pub(super) fn java_remote_in(&mut self, uuid: u128, frame: &[u8]) {
        let Some(host) = horizon_java_host::get() else {
            return;
        };
        let mut out = Out::default();
        out.long((uuid >> 64) as i64);
        out.long(uuid as i64);
        out.bytes(frame);
        horizon_java_host::bridge::with_active(self, || host.request("remote_in", &out.0));
    }

    /// The frames the Java side wrote while the hub was in use elsewhere, now that it is free.
    pub(super) fn java_wire_flush_lan(&self) {
        if let Some(host) = &self.lan {
            java_wire_flush(&host.hub);
        }
    }
}

#[cfg(test)]
mod block_event_tests {
    use super::*;
    use horizon_core::geometry::Direction;
    use horizon_java_host::bridge::{STATES, StateMap};

    fn kinds() -> Vec<u8> {
        BLOCK_EVENTS.take().iter().map(|e| e.kind).collect()
    }

    #[test]
    fn block_entity_changes_and_mod_shapes_reach_java() {
        let (air, stone, chest, ours) = (
            BlockStateId(0),
            BlockStateId(1),
            BlockStateId(2),
            BlockStateId(3),
        );
        let _ = STATES.set(StateMap {
            to_horizon: vec![Some(air), Some(stone), Some(chest), Some(ours)],
            to_java: vec![0, 1, 2, 3],
            java_air: 0,
            owned: vec![false, false, false, true],
            entity: vec![false, false, true, false],
            animate: vec![0; 4],
            animate_whole: false,
            animate_any: false,
        });
        let at = BlockPos::new(0, 0, 0);
        BLOCK_EVENTS.take();
        note_block_changed(at, air, chest, 3, true);
        note_block_changed(at, chest, air, 3, true);
        assert_eq!(kinds(), [0, 0]);
        note_block_changed(at, air, stone, 3, true);
        note_block_changed(at, chest, chest, 3, false);
        assert!(kinds().is_empty());
        note_shape_update(at, stone, Direction::Up, air);
        assert!(kinds().is_empty());
        note_shape_update(at, ours, Direction::East, air);
        let events = BLOCK_EVENTS.take();
        assert_eq!(events.len(), 1);
        assert_eq!((events[0].kind, events[0].b), (4, 5));
    }
}

/// One change Java's `ChunkMap` hears, as its wire numbers it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum TrackRecord {
    Tracking(horizon_client::lan::hub::Tracking),
    Chunk {
        uuid: u128,
        chunk: [i32; 2],
        watched: bool,
    },
}

impl TrackRecord {
    fn write(&self, out: &mut Out) {
        use horizon_client::lan::hub::Tracking;
        let head = |out: &mut Out, kind: i32, uuid: u128| {
            out.int(kind);
            out.long((uuid >> 64) as i64);
            out.long(uuid as i64);
        };
        match self {
            Self::Tracking(Tracking::View {
                uuid,
                entity_id,
                dimension,
                center,
                radius,
            }) => {
                head(out, 0, *uuid);
                out.int(*entity_id);
                out.text(dimension);
                out.int(center.0);
                out.int(center.1);
                out.int(*radius);
            }
            Self::Chunk {
                uuid,
                chunk,
                watched,
            } => {
                head(out, if *watched { 1 } else { 2 }, *uuid);
                out.int(chunk[0]);
                out.int(chunk[1]);
            }
            Self::Tracking(Tracking::Reset { uuid }) => head(out, 3, *uuid),
            Self::Tracking(Tracking::Pairing {
                uuid,
                entity,
                paired,
            }) => {
                head(out, 4, *uuid);
                out.int(*entity);
                out.bool(*paired);
            }
            Self::Tracking(Tracking::Left { uuid }) => head(out, 5, *uuid),
        }
    }
}

/// The hub's changes in the order Java applies them: a view before its chunks, a departure after them; `Sent` is native's only.
fn tracking_records(
    events: Vec<horizon_client::lan::hub::Tracking>,
    watches: &[horizon_client::lan::chunk_watch::LevelWatches],
) -> Vec<TrackRecord> {
    use horizon_client::lan::chunk_watch::ChunkWatchKind;
    use horizon_client::lan::hub::Tracking;
    let (views, after): (Vec<_>, Vec<_>) = events
        .into_iter()
        .partition(|e| matches!(e, Tracking::View { .. } | Tracking::Reset { .. }));
    let chunks = watches.iter().flat_map(|run| &run.watches).filter_map(|w| {
        let watched = match w.kind {
            ChunkWatchKind::Watch => true,
            ChunkWatchKind::Unwatch => false,
            ChunkWatchKind::Sent => return None,
        };
        Some(TrackRecord::Chunk {
            uuid: w.uuid,
            chunk: w.chunk,
            watched,
        })
    });
    views
        .into_iter()
        .map(TrackRecord::Tracking)
        .chain(chunks)
        .chain(after.into_iter().map(TrackRecord::Tracking))
        .collect()
}

#[cfg(test)]
mod tracking_tests {
    use super::*;
    use horizon_client::lan::chunk_watch::{ChunkWatch, ChunkWatchKind, LevelWatches};
    use horizon_client::lan::hub::Tracking;

    fn watch(kind: ChunkWatchKind, chunk: [i32; 2]) -> ChunkWatch {
        ChunkWatch {
            kind,
            uuid: 7,
            entity_id: 1,
            chunk,
        }
    }

    #[test]
    fn a_view_comes_before_its_chunks_and_a_departure_after_them() {
        let events = vec![
            Tracking::Left { uuid: 7 },
            Tracking::Pairing {
                uuid: 7,
                entity: 3,
                paired: true,
            },
            Tracking::View {
                uuid: 7,
                entity_id: 1,
                dimension: "minecraft:overworld".into(),
                center: (0, 0),
                radius: 2,
            },
        ];
        let watches = [LevelWatches {
            dimension: "minecraft:overworld".into(),
            watches: vec![
                watch(ChunkWatchKind::Watch, [1, 2]),
                watch(ChunkWatchKind::Sent, [1, 2]),
                watch(ChunkWatchKind::Unwatch, [3, 4]),
            ],
        }];
        let kinds: Vec<i32> = tracking_records(events, &watches)
            .iter()
            .map(|r| {
                let mut out = Out::default();
                r.write(&mut out);
                i32::from_be_bytes(out.0[..4].try_into().unwrap())
            })
            .collect();
        assert_eq!(kinds, [0, 1, 2, 5, 4]);
    }
}
