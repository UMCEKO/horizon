//! Java mods' items: their real `Item.use` runs in the embedded JVM on a view of the acting player,
//! and what it changed lands in Horizon's own player and world.

use super::*;
use commands::Target;
use horizon_java_host::wire::{In, Out};
use horizon_world::nbt::{self, Limits, NamedTag, Tag};

const INVENTORY: usize = 36;
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
        if !host.owns_item(&stack.item) {
            return;
        }
        match hit {
            Some(hit) => self.java_use_on(&stack.item, hit, "use_item_on", claimed),
            None => self.java_call_use(&stack.item, "use_item", Vec::new(), claimed),
        }
    }

    fn java_use_on(&mut self, what: &str, hit: &Hit, op: &str, claimed: &mut bool) {
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
        self.java_call_use(what, op, out.0, claimed);
    }

    fn java_call_use(&mut self, what: &str, op: &str, tail: Vec<u8>, claimed: &mut bool) {
        let Some(host) = horizon_java_host::get() else {
            return;
        };
        self.java_bind_states();
        let Some(mut request) = self.java_use_request() else {
            return;
        };
        request.extend(tail);
        let reply = horizon_java_host::bridge::with_active(self, || host.request(op, &request));
        let Some(reply) = reply else {
            return;
        };
        if self.java_apply_use(&reply, claimed).is_none() {
            log::warn!("Horizon: a Java mod's use of {what} answered in pieces");
        }
        self.java_drain_block_events();
    }

    /// Fills the acting player's view again, for a bridge that changed something its getters read.
    pub(super) fn java_refresh_player(&mut self) {
        if let (Some(host), Some(request)) = (horizon_java_host::get(), self.java_use_request()) {
            host.request("refresh", &request);
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
        let Some(listed) = host.request("states", &[]) else {
            return;
        };
        let registry = &self.pack.assets.registry;
        let mut map = StateMap {
            to_horizon: Vec::new(),
            to_java: Vec::new(),
            java_air: 0,
            owned: Vec::new(),
        };
        let mut listed = In(&listed);
        let (Some(count), Some(air)) = (listed.int(), listed.int()) else {
            return;
        };
        map.java_air = air;
        map.to_horizon.resize(count as usize, None);
        for _ in 0..count {
            let read = (|| {
                let java = listed.int()?;
                let name = listed.text()?.to_string();
                let mut properties = Vec::new();
                for _ in 0..listed.int()? {
                    properties.push((listed.text()?.to_string(), listed.text()?.to_string()));
                }
                Some((java, name, properties))
            })();
            let Some((java, name, properties)) = read else {
                return;
            };
            let owned = !name.starts_with("minecraft:");
            let state = horizon_world::chunk::BlockState {
                name,
                properties: properties.into_iter().collect(),
            };
            let Ok(id) = registry.resolve(&state) else {
                continue;
            };
            if let Some(cell) = map.to_horizon.get_mut(java as usize) {
                *cell = Some(id);
            }
            let at = id.0 as usize;
            if map.to_java.len() <= at {
                map.to_java.resize(at + 1, air);
            }
            map.to_java[at] = java;
            if map.owned.len() <= at {
                map.owned.resize(at + 1, false);
            }
            map.owned[at] = owned;
        }
        let _ = STATES.set(map);
    }

    fn java_use_request(&mut self) -> Option<Vec<u8>> {
        let position = self.player.as_ref()?.position;
        let [a, b, c, d] = self.acting_uuid();
        let mut out = Out::default();
        out.long(((a as i64) << 32) | (b as u32 as i64));
        out.long(((c as i64) << 32) | (d as u32 as i64));
        out.text(&self.options.player_name());
        out.text(self.dim.id());
        out.long(self.world.game_time);
        out.double(position.x);
        out.double(position.y);
        out.double(position.z);
        out.float(self.yaw);
        out.float(self.pitch);
        out.bool(self.keys.contains(&KeyCode::ShiftLeft));
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

    fn java_apply_use(&mut self, reply: &[u8], claimed: &mut bool) -> Option<()> {
        let mut reply = In(reply);
        *claimed = reply.bool()?;
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
        self.java_apply_effects(&mut reply)
    }

    /// What Java did to the world on a call: sounds, entities, vanilla block entities it changed.
    fn java_apply_effects(&mut self, reply: &mut In<'_>) -> Option<()> {
        for _ in 0..reply.int()? {
            let sound = reply.text()?;
            let at = [reply.double()?, reply.double()?, reply.double()?];
            let (volume, pitch) = (reply.float()?, reply.float()?);
            self.audio.play(sound, Some(at), volume, pitch);
        }
        for _ in 0..reply.int()? {
            self.spawn_entity_from_nbt(read_tag(reply.bytes()?)?);
        }
        for _ in 0..reply.int()? {
            let tag = read_tag(reply.bytes()?)?;
            if let Some((key, entity)) = horizon_client::save::decode_block_entity(&tag) {
                self.world.block_entities.insert(key, entity);
            }
        }
        for _ in 0..reply.int()? {
            let id = reply.int()? as u32;
            let tag = read_tag(reply.bytes()?)?;
            if let Some(target) = self.java_target(id) {
                let _ = self.apply_entity_nbt(target, tag);
            }
        }
        for _ in 0..reply.int()? {
            let id = reply.int()? as u32;
            if let Some(target) = self.java_target(id) {
                self.kill_target(target);
            }
        }
        Some(())
    }

    /// Calls `GameBridge`'s `op` for this level, with what it does to the world applied after.
    fn java_level_call(&mut self, op: &str, body: impl FnOnce(&mut Out)) {
        let Some(host) = horizon_java_host::get() else {
            return;
        };
        self.java_bind_states();
        let mut out = Out::default();
        out.text(self.dim.id());
        out.long(self.world.game_time);
        body(&mut out);
        let reply = horizon_java_host::bridge::with_active(self, || host.request(op, &out.0));
        if let Some(reply) = reply {
            self.java_apply_effects(&mut In(&reply));
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
            | Target::Dragon(id) => Some(id),
        }
    }

    fn java_target(&mut self, id: u32) -> Option<Target> {
        self.level_candidates()
            .into_iter()
            .find(|t| Self::java_target_id(*t) == Some(id))
    }

    fn java_write_entity(&mut self, target: Target, out: &mut Out) -> bool {
        let Some(id) = Self::java_target_id(target) else {
            return false;
        };
        let kind = self.target_type(target);
        let Some(mut tag) = self.entity_nbt(target) else {
            return false;
        };
        if let Tag::Compound(map) = &mut tag {
            map.insert("id".into(), Tag::String(kind.as_str().into()));
        }
        let Some(bytes) = write_tag(tag) else {
            return false;
        };
        out.int(id as i32);
        out.bytes(&bytes);
        true
    }

    /// `EntityViews.query`: the entities with positions in the box, in their saved form.
    pub(in crate::game) fn java_entities_in(&mut self, low: [f64; 3], high: [f64; 3]) -> Vec<u8> {
        let mut out = Out::default();
        let mut count = 0;
        for target in self.level_candidates() {
            let inside = self
                .target_position(target)
                .is_some_and(|p| (0..3).all(|axis| p[axis] >= low[axis] && p[axis] <= high[axis]));
            if inside && self.java_write_entity(target, &mut out) {
                count += 1;
            }
        }
        let mut all = Out::default();
        all.int(count);
        all.0.extend(out.0);
        all.0
    }

    /// `EntityViews.queryId`: one entity the same way.
    pub(in crate::game) fn java_entity_by_id(&mut self, id: u32) -> Vec<u8> {
        let mut out = Out::default();
        let found = self
            .java_target(id)
            .is_some_and(|t| self.java_write_entity(t, &mut out));
        let mut all = Out::default();
        all.int(found as i32);
        all.0.extend(out.0);
        all.0
    }

    /// `level.getBlockEntity` found nothing in Java: the saved form Horizon holds, if any, goes there now.
    pub(in crate::game) fn java_send_block_entity(&mut self, pos: BlockPos) {
        let key = (pos.x, pos.y, pos.z);
        let Some(entity) = self.world.block_entities.get(&key) else {
            return;
        };
        if matches!(entity, BlockEntity::Mod(m) if m.id.is_empty()) {
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
        self.java_level_call("block_entity_load", |out| {
            out.long(pos.pack());
            out.bytes(&bytes);
        });
    }

    /// Java made a block entity: its place in the chunk, which saves it and gives it its turn to tick.
    pub(in crate::game) fn java_adopt_block_entity(&mut self, pos: BlockPos, id: String) {
        let entity = BlockEntity::Mod(Box::new(horizon_client::containers::ModBlockEntity {
            id,
            ..Default::default()
        }));
        self.world
            .block_entities
            .insert((pos.x, pos.y, pos.z), entity);
    }

    /// `BlockEntityTicker.tick` of a block entity Java holds; it's out of the world during its tick, so its saved form comes along when Java asks.
    pub(super) fn java_block_entity_tick(
        &mut self,
        pos: [i32; 3],
        entity: &horizon_client::containers::ModBlockEntity,
    ) {
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        if !self.java_tick_once(at) {
            let key = (pos[0], pos[1], pos[2]);
            let name = self
                .res
                .facts()
                .get(self.world.block(pos[0], pos[1], pos[2]))
                .name
                .clone();
            let whole = BlockEntity::Mod(Box::new(entity.clone()));
            let tag = horizon_client::save::encode_block_entity(key, &whole, &name);
            if let Some(bytes) = write_tag(tag) {
                self.java_level_call("block_entity_load", |out| {
                    out.long(at.pack());
                    out.bytes(&bytes);
                });
                self.java_tick_once(at);
            }
        }
        self.java_drain_block_events();
    }

    /// False when Java doesn't hold this block entity yet.
    fn java_tick_once(&mut self, at: BlockPos) -> bool {
        let Some(host) = horizon_java_host::get() else {
            return true;
        };
        let mut out = Out::default();
        out.text(self.dim.id());
        out.long(self.world.game_time);
        out.long(at.pack());
        let reply = horizon_java_host::bridge::with_active(self, || {
            host.request("block_entity_tick", &out.0)
        });
        let Some(reply) = reply else {
            return true;
        };
        let mut reply = In(&reply);
        let held = reply.bool().unwrap_or(true);
        self.java_apply_effects(&mut reply);
        held
    }

    /// `ServerPlayerGameMode.useItemOn`, for a click on a mod's block: true when the block took it.
    pub(super) fn java_click_block(&mut self, hit: &Hit) -> bool {
        let state = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
        if !horizon_java_host::bridge::java_block(state) {
            return false;
        }
        let mut claimed = false;
        self.java_use_on("a block", hit, "click_block", &mut claimed);
        claimed
    }

    /// Before a save: what each block entity Java holds would write, into the chunk's own record of it.
    pub(super) fn java_flush_block_entities(&mut self) {
        let Some(host) = horizon_java_host::get() else {
            return;
        };
        let mut out = Out::default();
        out.text(self.dim.id());
        let reply = horizon_java_host::bridge::with_active(self, || {
            host.request("block_entities_save", &out.0)
        });
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
}

thread_local! {
    static BLOCK_EVENTS: std::cell::RefCell<Vec<BlockEvent>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn java_id(state: BlockStateId) -> i32 {
    use horizon_java_host::bridge::{Ret, STATES};
    let _ = STATES.get();
    state.into_raw()
}

/// A level's hook, inside `with_level`: Java can't run yet, as it would reenter the level.
pub(super) fn note_block_changed(pos: BlockPos, old: BlockStateId, new: BlockStateId) {
    use horizon_java_host::bridge::java_block;
    if java_block(old) || java_block(new) {
        BLOCK_EVENTS.with_borrow_mut(|e| {
            e.push(BlockEvent {
                kind: 0,
                pos,
                a: java_id(old),
                b: java_id(new),
            });
        });
    }
}

pub(super) fn note_neighbor_changed(pos: BlockPos, state: BlockStateId, moved: bool) {
    if horizon_java_host::bridge::java_block(state) {
        BLOCK_EVENTS.with_borrow_mut(|e| {
            e.push(BlockEvent {
                kind: 1,
                pos,
                a: java_id(state),
                b: moved as i32,
            });
        });
    }
}

pub(super) fn note_block_tick(pos: BlockPos, state: BlockStateId, random: bool) {
    if horizon_java_host::bridge::java_block(state) {
        BLOCK_EVENTS.with_borrow_mut(|e| {
            e.push(BlockEvent {
                kind: if random { 3 } else { 2 },
                pos,
                a: java_id(state),
                b: 0,
            });
        });
    }
}
