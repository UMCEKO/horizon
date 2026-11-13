// game-module-cfg: feature java-mods
//! The Java client's entities, fed from what the window applied of the server's entity packets: Java never ticks, moves or draws them.

use super::java_screens::live::{java_client_remote, java_seen, java_seen_now};
use super::*;
use horizon_java_host::wire::Out;
use horizon_protocol::buf::{Codec, Writer, unpack_degrees};
use horizon_protocol::packets::play::{self, Clientbound as C};
use std::cell::RefCell;
use std::collections::HashMap;

/// A feed entry's kind; wire values shared with `ClientEntities.java`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
enum Entry {
    Add,
    Data,
    Equip,
    Passengers,
    Move,
    Event,
    Remove,
    Tick,
}

/// The `EntityJoinLevelEvent` and `EntityLeaveLevelEvent` bits of an ADD's last byte: which of them the client's listeners hear for the type.
const JOIN_BIT: u8 = 1;
const LEAVE_BIT: u8 = 2;
const JOIN_EVENT: &str = "net/neoforged/neoforge/event/entity/EntityJoinLevelEvent";
const LEAVE_EVENT: &str = "net/neoforged/neoforge/event/entity/EntityLeaveLevelEvent";
/// The `EntityTickEvent` bits of a TICK's last byte.
const PRE_BIT: u8 = 1;
const POST_BIT: u8 = 2;

/// An entity's place, look, motion and footing as the window last applied them.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
struct Pose {
    position: [f64; 3],
    yaw: f32,
    pitch: f32,
    head: f32,
    motion: [f64; 3],
    on_ground: bool,
}

/// What the window's own state says of the entities a packet names, after it applied the packet.
trait Applied {
    fn type_name(&self, kind: i32) -> Option<String>;
    fn position(&self, id: i32) -> Option<[f64; 3]>;
    /// The vehicle `SetPassengers` seated `passenger` in.
    fn vehicle(&self, passenger: i32) -> Option<i32>;
    /// The join and leave bits for entities of this type on the client.
    fn joins_and_leaves(&self, type_id: &str) -> u8;
}

impl Applied for remote::Remote {
    fn type_name(&self, kind: i32) -> Option<String> {
        self.builtin
            .get("minecraft:entity_type")?
            .name(kind)
            .map(str::to_owned)
    }
    fn position(&self, id: i32) -> Option<[f64; 3]> {
        self.tracked_position(id)
    }
    fn vehicle(&self, passenger: i32) -> Option<i32> {
        self.passenger_vehicles
            .get(&(passenger as u32))
            .map(|&v| v as i32)
    }
    fn joins_and_leaves(&self, type_id: &str) -> u8 {
        let heard = |event: &str| {
            // Without an analysis every listener hears every type, as on the server.
            horizon_java_host::entity_facts::get()
                .is_none_or(|f| f.event(event).is_some_and(|e| f.admits(e, type_id, true)))
        };
        let bit = |event: &str, bit: u8| if heard(event) { bit } else { 0 };
        bit(JOIN_EVENT, JOIN_BIT) | bit(LEAVE_EVENT, LEAVE_BIT)
    }
}

thread_local! {
    static POSES: RefCell<HashMap<i32, Pose>> = RefCell::default();
}

impl Game<'_> {
    /// Whether the Java client hears of this packet once the window applied it: from the connection's start to a Java client that connects with it, the window's own server's or one over the connection.
    pub(in crate::game) fn java_feeds(&self, packet: &C) -> bool {
        self.java_feeding()
            && matches!(
                packet,
                C::AddEntity(_)
                    | C::SetEntityData(_)
                    | C::SetEquipment(_)
                    | C::SetPassengers(_)
                    | C::MoveEntityPos(_)
                    | C::MoveEntityPosRot(_)
                    | C::MoveEntityRot(_)
                    | C::EntityPositionSync(_)
                    | C::TeleportEntity(_)
                    | C::RotateHead(_)
                    | C::SetEntityMotion(_)
                    | C::EntityEvent(_)
                    | C::RemoveEntities(_)
            )
    }

    fn java_feeding(&self) -> bool {
        horizon_java_host::get().is_some() && (self.integrated.is_some() || java_client_remote())
    }

    /// The window ticked the mob at `index` of its client entities: the Java client posts the tick events its listeners hear for the type, in the order the window ticked them.
    pub(in crate::game) fn java_entity_ticked(&mut self, index: usize) {
        if !self.java_feeding() {
            return;
        }
        let Some(m) = self.mobs.get(index) else {
            return;
        };
        let id = m.id as i32;
        let mask = tick_mask(&m.kind.id);
        if mask == 0 || !POSES.with_borrow(|poses| poses.contains_key(&id)) {
            return;
        }
        let mut out = Out::default();
        out.int(id);
        out.0.push(mask);
        let mut entry = vec![Entry::Tick as u8];
        entry.extend(out.0);
        self.java_entity_entry(java_seen_now(), &entry);
    }

    /// The Java client's listeners refused these entities' joins: its level never had them, so neither does the window.
    pub(in crate::game) fn java_entities_refused(&mut self, ids: &[i32]) {
        POSES.with_borrow_mut(|poses| ids.iter().for_each(|id| _ = poses.remove(id)));
        self.remove_entities(ids);
    }

    /// The window applied the play packet at `seq`; `packet` is it when the Java client hears of it.
    pub(in crate::game) fn java_feed(&mut self, packet: Option<&C>, seq: u64) {
        let entry = packet.and_then(|packet| {
            let r = self.remote.as_ref()?;
            POSES.with_borrow_mut(|poses| entry(packet, r.entity_id, r, poses))
        });
        if let Some(entry) = entry {
            self.java_entity_entry(seq, &entry);
        }
        java_seen(seq);
    }
}

/// The Java client's world is gone: what it was told of entities with it.
pub(in crate::game) fn java_client_entities_left() {
    POSES.with_borrow_mut(HashMap::clear);
}

/// The `EntityTickEvent` bits the client's listeners hear for entities of this type; both without an analysis, as for the joins.
fn tick_mask(type_id: &str) -> u8 {
    use horizon_java_host::entity_facts::Hook;
    let Some(f) = horizon_java_host::entity_facts::get() else {
        return PRE_BIT | POST_BIT;
    };
    let bit = |hook: Hook, bit: u8| if f.hook_admits(hook, type_id, true) { bit } else { 0 };
    bit(Hook::EntityTickPre, PRE_BIT) | bit(Hook::EntityTickPost, POST_BIT)
}

fn body<T: Codec<T>>(packet: &T) -> Vec<u8> {
    let mut w = Writer::new();
    T::write(packet, &mut w);
    w.data
}

/// The entry for `packet`, applied already; the local player `me` is the Java client's own and isn't fed.
fn entry(
    packet: &C,
    me: i32,
    applied: &impl Applied,
    poses: &mut HashMap<i32, Pose>,
) -> Option<Vec<u8>> {
    let mut out = Out::default();
    let kind = match packet {
        C::AddEntity(p) if p.id != me => {
            let pose = Pose {
                position: p.position,
                yaw: unpack_degrees(p.yaw),
                pitch: unpack_degrees(p.pitch),
                head: unpack_degrees(p.head_yaw),
                motion: p.velocity,
                on_ground: false,
            };
            poses.insert(p.id, pose);
            out.int(p.id);
            out.long((p.uuid >> 64) as i64);
            out.long(p.uuid as i64);
            let type_id = applied.type_name(p.kind)?;
            out.text(&type_id);
            pose.position.iter().for_each(|v| out.double(*v));
            out.float(pose.pitch);
            out.float(pose.yaw);
            out.float(pose.head);
            pose.motion.iter().for_each(|v| out.double(*v));
            out.int(p.data);
            out.0.push(applied.joins_and_leaves(&type_id));
            Entry::Add
        }
        C::SetEntityData(p) if p.id != me => {
            out.int(p.id);
            out.bytes(&body(p));
            Entry::Data
        }
        C::SetEquipment(p) if p.id != me => {
            out.int(p.id);
            out.bytes(&body(p));
            Entry::Equip
        }
        C::SetPassengers(p) if p.vehicle != me => {
            let seated: Vec<i32> = p
                .passengers
                .iter()
                .copied()
                .filter(|&id| id != me && applied.vehicle(id) == Some(p.vehicle))
                .collect();
            out.int(p.vehicle);
            out.int(seated.len() as i32);
            seated.iter().for_each(|id| out.int(*id));
            Entry::Passengers
        }
        C::EntityEvent(p) if p.id != me => {
            out.int(p.id);
            out.0.push(p.event as u8);
            Entry::Event
        }
        C::RemoveEntities(p) => {
            let gone: Vec<i32> = p.ids.iter().copied().filter(|&id| id != me).collect();
            if gone.is_empty() {
                return None;
            }
            out.int(gone.len() as i32);
            for id in &gone {
                poses.remove(id);
                out.int(*id);
            }
            Entry::Remove
        }
        _ => {
            let (id, pose) = moved(packet, me, applied, poses)?;
            poses.insert(id, pose);
            out.int(id);
            pose.position.iter().for_each(|v| out.double(*v));
            out.float(pose.yaw);
            out.float(pose.pitch);
            out.float(pose.head);
            pose.motion.iter().for_each(|v| out.double(*v));
            out.bool(pose.on_ground);
            Entry::Move
        }
    };
    let mut entry = vec![kind as u8];
    entry.extend(out.0);
    Some(entry)
}

/// A move packet's entity and its pose after the window applied the packet.
fn moved(
    packet: &C,
    me: i32,
    applied: &impl Applied,
    poses: &HashMap<i32, Pose>,
) -> Option<(i32, Pose)> {
    let id = match packet {
        C::MoveEntityPos(play::MoveEntityPos(m))
        | C::MoveEntityPosRot(play::MoveEntityPosRot(m))
        | C::MoveEntityRot(play::MoveEntityRot(m)) => m.id,
        C::EntityPositionSync(p) => p.id,
        C::TeleportEntity(p) => p.id,
        C::RotateHead(p) => p.id,
        C::SetEntityMotion(p) => p.id,
        _ => return None,
    };
    let old = *poses.get(&id).filter(|_| id != me)?;
    let mut pose = old;
    pose.position = applied.position(id).unwrap_or(old.position);
    match packet {
        C::MoveEntityPos(play::MoveEntityPos(m))
        | C::MoveEntityPosRot(play::MoveEntityPosRot(m))
        | C::MoveEntityRot(play::MoveEntityRot(m)) => {
            if let Some((yaw, pitch)) = m.rotation {
                (pose.yaw, pose.pitch) = (unpack_degrees(yaw), unpack_degrees(pitch));
            }
            pose.on_ground = m.on_ground;
        }
        C::EntityPositionSync(p) => {
            (pose.yaw, pose.pitch, pose.on_ground) = (p.yaw, p.pitch, p.on_ground);
        }
        C::TeleportEntity(p) => {
            // `PositionMoveRotation.calculateAbsolute`.
            let relative = |bit: i32, base: f32| if p.relatives & bit != 0 { base } else { 0.0 };
            pose.yaw = relative(play::relative::Y_ROT, old.yaw) + p.yaw;
            pose.pitch = (relative(play::relative::X_ROT, old.pitch) + p.pitch).clamp(-90.0, 90.0);
            let motion = horizon_client::player_sync::teleport_velocity(
                Vec3::new(old.motion[0], old.motion[1], old.motion[2]),
                (old.yaw, old.pitch),
                (pose.yaw, pose.pitch),
                &play::PlayerPosition {
                    teleport: 0,
                    position: p.position,
                    delta: p.delta,
                    yaw: p.yaw,
                    pitch: p.pitch,
                    relatives: p.relatives,
                },
            );
            pose.motion = [motion.x, motion.y, motion.z];
            pose.on_ground = p.on_ground;
        }
        C::RotateHead(p) => pose.head = unpack_degrees(p.head_yaw),
        C::SetEntityMotion(p) => pose.motion = p.velocity,
        _ => return None,
    }
    Some((id, pose))
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_java_host::wire::In;

    struct World {
        positions: HashMap<i32, [f64; 3]>,
        vehicles: HashMap<i32, i32>,
    }

    impl Applied for World {
        fn type_name(&self, kind: i32) -> Option<String> {
            (kind == 95).then(|| "minecraft:pig".to_owned())
        }
        fn position(&self, id: i32) -> Option<[f64; 3]> {
            self.positions.get(&id).copied()
        }
        fn vehicle(&self, passenger: i32) -> Option<i32> {
            self.vehicles.get(&passenger).copied()
        }
        fn joins_and_leaves(&self, type_id: &str) -> u8 {
            if type_id == "minecraft:pig" { JOIN_BIT | LEAVE_BIT } else { 0 }
        }
    }

    fn world() -> World {
        World {
            positions: HashMap::from([(7, [1.5, 64.0, -2.5])]),
            vehicles: HashMap::new(),
        }
    }

    fn add(id: i32) -> C {
        C::AddEntity(play::AddEntity {
            id,
            uuid: 0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10,
            kind: 95,
            position: [1.5, 64.0, -2.5],
            velocity: [0.0, -0.08, 0.0],
            pitch: 0,
            yaw: 64,
            head_yaw: -128,
            data: 3,
        })
    }

    /// `ClientEntities.read`'s order for an ADD.
    fn read_add(entry: &[u8]) -> (i32, u128, String, [f64; 3], [f32; 3], [f64; 3], i32, u8) {
        assert_eq!(entry[0], Entry::Add as u8);
        let mut r = In(&entry[1..]);
        let id = r.int().unwrap();
        let uuid = ((r.long().unwrap() as u64 as u128) << 64) | r.long().unwrap() as u64 as u128;
        let kind = r.text().unwrap().to_owned();
        let position = [
            r.double().unwrap(),
            r.double().unwrap(),
            r.double().unwrap(),
        ];
        let look = [r.float().unwrap(), r.float().unwrap(), r.float().unwrap()];
        let motion = [
            r.double().unwrap(),
            r.double().unwrap(),
            r.double().unwrap(),
        ];
        let data = r.int().unwrap();
        let events = r.byte().unwrap();
        assert!(r.0.is_empty());
        (id, uuid, kind, position, look, motion, data, events)
    }

    #[test]
    fn add_entry_carries_the_packet_with_its_angles_in_degrees() {
        let mut poses = HashMap::new();
        let entry = entry(&add(7), 1, &world(), &mut poses).unwrap();
        let (id, uuid, kind, position, look, motion, data, events) = read_add(&entry);
        assert_eq!((id, kind.as_str(), data), (7, "minecraft:pig", 3));
        assert_eq!(events, JOIN_BIT | LEAVE_BIT);
        assert_eq!(uuid, 0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10);
        assert_eq!(position, [1.5, 64.0, -2.5]);
        // Pitch, yaw, head yaw: 64/256 of a turn is 90 degrees, -128 is -180.
        assert_eq!(look, [0.0, 90.0, -180.0]);
        assert_eq!(motion, [0.0, -0.08, 0.0]);
    }

    #[test]
    fn the_local_player_is_not_fed() {
        let mut poses = HashMap::new();
        assert!(entry(&add(1), 1, &world(), &mut poses).is_none());
        let removed = C::RemoveEntities(play::RemoveEntities { ids: vec![1] });
        assert!(entry(&removed, 1, &world(), &mut poses).is_none());
    }

    #[test]
    fn move_entry_is_the_pose_after_the_window_applied_it() {
        let mut poses = HashMap::new();
        let mut world = world();
        entry(&add(7), 1, &world, &mut poses).unwrap();
        world.positions.insert(7, [3.0, 65.0, -2.5]);
        let moved = C::MoveEntityPosRot(play::MoveEntityPosRot(play::MoveEntity {
            id: 7,
            delta: None,
            rotation: Some((-64, 32)),
            on_ground: true,
        }));
        let entry = entry(&moved, 1, &world, &mut poses).unwrap();
        assert_eq!(entry[0], Entry::Move as u8);
        let mut r = In(&entry[1..]);
        assert_eq!(r.int(), Some(7));
        let position = [
            r.double().unwrap(),
            r.double().unwrap(),
            r.double().unwrap(),
        ];
        assert_eq!(position, [3.0, 65.0, -2.5]);
        assert_eq!(
            [r.float().unwrap(), r.float().unwrap(), r.float().unwrap()],
            [-90.0, 45.0, -180.0]
        );
        let motion = [
            r.double().unwrap(),
            r.double().unwrap(),
            r.double().unwrap(),
        ];
        assert_eq!(motion, [0.0, -0.08, 0.0]);
        assert_eq!(r.bool(), Some(true));
        assert!(r.0.is_empty());
    }

    #[test]
    fn passengers_are_those_the_window_seated() {
        let mut poses = HashMap::new();
        let mut world = world();
        world.vehicles.extend([(8, 7), (1, 7)]);
        let packet = C::SetPassengers(play::SetPassengers {
            vehicle: 7,
            passengers: vec![1, 9, 8],
        });
        let entry = entry(&packet, 1, &world, &mut poses).unwrap();
        let mut r = In(&entry[1..]);
        assert_eq!((r.int(), r.int(), r.int()), (Some(7), Some(1), Some(8)));
        assert!(r.0.is_empty());
    }

    #[test]
    fn feed_follows_arrival_order() {
        let mut poses = HashMap::new();
        let world = world();
        let packets = [
            add(7),
            C::EntityEvent(play::EntityEvent { id: 7, event: 10 }),
            C::RotateHead(play::RotateHead { id: 7, head_yaw: 0 }),
            C::RemoveEntities(play::RemoveEntities { ids: vec![7] }),
            C::RotateHead(play::RotateHead { id: 7, head_yaw: 0 }),
        ];
        let kinds: Vec<u8> = packets
            .iter()
            .filter_map(|p| entry(p, 1, &world, &mut poses))
            .map(|e| e[0])
            .collect();
        // A move for an entity the Java client no longer has is not fed.
        assert_eq!(
            kinds,
            [
                Entry::Add as u8,
                Entry::Event as u8,
                Entry::Move as u8,
                Entry::Remove as u8
            ]
        );
    }
}
