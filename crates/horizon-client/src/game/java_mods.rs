//! Java mods' items: their real `Item.use` runs in the embedded JVM on a view of the acting player,
//! and what it changed lands in Horizon's own player and world.

use super::*;
use horizon_java_host::wire::{In, Out};
use horizon_world::nbt::{self, Limits, NamedTag, Tag};

const INVENTORY: usize = 36;
const ARMOR: [usize; 4] = [3, 2, 1, 0];
const OFFHAND: usize = 40;

fn write_tag(tag: Tag) -> Option<Vec<u8>> {
    let root = NamedTag {
        name: "".into(),
        tag,
    };
    nbt::write_named(&root, Limits::default()).ok()
}

fn read_tag(bytes: &[u8]) -> Option<Tag> {
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
        self.java_bind_states();
        let Some(mut request) = self.java_use_request() else {
            return;
        };
        if let Some(hit) = hit {
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
            request.extend(out.0);
        }
        let op = if hit.is_some() {
            "use_item_on"
        } else {
            "use_item"
        };
        let reply = horizon_java_host::bridge::with_active(self, || host.request(op, &request));
        let Some(reply) = reply else {
            return;
        };
        if self.java_apply_use(&reply, claimed).is_none() {
            log::warn!(
                "Horizon: a Java mod's use of {} answered in pieces",
                stack.item
            );
        }
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
        for _ in 0..reply.int()? {
            let sound = reply.text()?;
            let at = [reply.double()?, reply.double()?, reply.double()?];
            let (volume, pitch) = (reply.float()?, reply.float()?);
            self.audio.play(sound, Some(at), volume, pitch);
        }
        for _ in 0..reply.int()? {
            self.spawn_entity_from_nbt(read_tag(reply.bytes()?)?);
        }
        Some(())
    }
}
