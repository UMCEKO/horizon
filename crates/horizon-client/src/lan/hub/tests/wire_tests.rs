//! The framed bytes each connection receives for a scripted tick sequence.
use super::*;
use crate::lan::conn::{drain_wire, wired_sender};

fn guest(conn: u32, position: [f64; 3], sender: crate::lan::PlayerSender) -> Guest {
    let mut g = Guest::new(
        crate::lan::Joined {
            conn,
            uuid: u128::from(conn),
            name: format!("Player{conn}"),
            properties: Vec::new(),
            info: horizon_protocol::packets::configuration::ClientInformation::new(8),
            brand: "vanilla".into(),
            sender,
            latency: Arc::new(std::sync::atomic::AtomicI32::new(0)),
            address: format!("127.0.0.1:{}", 25000 + conn).parse().unwrap(),
            expected: false,
            configured: Default::default(),
        },
        conn as i32,
        position,
        8,
    );
    g.placed = true;
    g.dimension = "minecraft:overworld".into();
    g
}

/// Lit columns of pseudo-random blocks, so bodies compress and exceed the shared-frame size.
fn level() -> World {
    let mut world = World::new(horizon_world::registry::BlockStateId(0), 0);
    let mut state = 12345u32;
    for x in -5..=5 {
        for z in -5..=5 {
            let mut chunk = Chunk::empty(x, z);
            for y in 0..48 {
                for i in 0..256 {
                    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                    chunk.set(i % 16, y, i / 16, (state >> 24) % 7);
                }
            }
            chunk.lit = true;
            world.chunks.insert((x, z), Arc::new(chunk));
        }
    }
    world
}

fn mob(id: i32, kind: &str, at: [f64; 3], round: usize) -> EntityState {
    EntityState {
        id,
        uuid: id as u128,
        kind: kind.into(),
        position: [
            at[0] + round as f64 * 0.7,
            at[1],
            at[2] - round as f64 * 0.3,
        ],
        yaw: round as f32 * 11.0,
        head_yaw: round as f32 * 5.0,
        ..EntityState::default()
    }
}

/// Each guest's wire bytes after a scripted server: chunks, entities, broadcasts, block and light changes, a disconnect.
fn wire(threshold: Option<i32>) -> Vec<Vec<u8>> {
    let mut sync = SyncData::empty();
    sync.builtin.registries.insert(
        "minecraft:entity_type".into(),
        horizon_protocol::registry::Ids::from_names(vec![
            "minecraft:zombie".into(),
            "minecraft:cow".into(),
        ]),
    );
    let (mut hub, _) = hub_with_sync(sync);
    hub.guests.clear();
    let at = [
        [0.5, 64.0, 0.5],
        [40.5, 64.0, 8.5],
        [-30.5, 64.0, -20.5],
        [-8.5, 64.0, 70.5],
    ];
    let mut clients = Vec::new();
    for (i, position) in at.into_iter().enumerate() {
        let (sender, client) = wired_sender(threshold);
        clients.push(client);
        let mut g = guest(i as u32 + 1, position, sender);
        if i == 2 {
            g.dimension = "minecraft:the_nether".into();
        }
        g.permission = if i == 0 { 4 } else { 0 };
        hub.guests.push(g);
    }
    let mut world = level();
    let border = crate::worldborder::WorldBorder::default();
    let mut next_entity = 100;
    hub.ticks = 10;
    for round in 0..6usize {
        let holding = (round % 2 == 0).then(crate::lan::Holding::start);
        hub.ticks += 1;
        for g in &mut hub.guests {
            g.unacknowledged = 0;
        }
        let mobs = [
            mob(50, "minecraft:zombie", [3.5, 64.0, 3.5], round),
            mob(51, "minecraft:cow", [-4.5, 64.0, 6.5], round),
            mob(52, "minecraft:cow", [45.5, 64.0, 9.5], round),
        ];
        let mut ctx = TickCtx {
            world: &mut world,
            entities: &mobs,
            next_entity: &mut next_entity,
            spawn: [0.5, 64.0, 0.5],
            difficulty: 2,
            difficulty_locked: false,
            clock_rate: 1.0,
            tick_rate: 20.0,
            tick_frozen: false,
            tick_steps: Default::default(),
            collision: None,
            border: &border,
            streaming: None,
        };
        hub.update_chunks(&ctx);
        hub.track_entities(&mut ctx);
        let text = |s: &str| {
            C::SystemChat(play::SystemChat {
                content: Tag::String(JavaString::from(s)),
                overlay: false,
            })
        };
        hub.broadcast(&text(&format!("all {round}")));
        hub.broadcast_level(&C::GameEvent(play::GameEvent {
            event: 7,
            param: round as f32,
        }));
        hub.broadcast_except(2, &text(&format!("except {round}")));
        hub.broadcast_near([0.5, 64.0, 0.5], 60.0, &text("near"), Some(1));
        hub.send_to_tracking(
            50,
            &C::EntityEvent(play::EntityEvent {
                id: 50,
                event: round as i8,
            }),
        );
        hub.critical_hit(50, 51, round % 2 == 0);
        hub.system_message_to(Tag::String(JavaString::from("named")), |n| n != "Player3");
        hub.send_low_disk_space_warning();
        let log = world.block_log.get_or_insert_with(Vec::new);
        log.extend([
            (1, 64, 1),
            (2, 64, 1),
            (20, 70, 20 + round as i32),
            (-40, 10, 3),
        ]);
        let light = world.light_log.get_or_insert_with(Default::default);
        light.insert((5, 5 - round as i32), crate::section_mask::SectionMask::ALL);
        light.insert((0, 0), crate::section_mask::SectionMask::single(3));
        hub.broadcast_changes(&mut world);
        drop(holding);
        crate::lan::flush_all(hub.guests.iter().map(|g| &g.sender));
    }
    hub.broadcast(&C::RemoveEntities(play::RemoveEntities {
        ids: vec![50, 51],
    }));
    for g in &hub.guests {
        g.sender.disconnect(Tag::Byte(0));
    }
    clients.into_iter().map(drain_wire).collect()
}

fn digest(wires: &[Vec<u8>]) -> Vec<(usize, u64)> {
    wires
        .iter()
        .map(|w| (w.len(), xxhash_rust::xxh3::xxh3_64(w)))
        .collect()
}

#[test]
fn every_connection_receives_the_bytes_it_always_did() {
    let got = [None, Some(256)].map(|threshold| digest(&wire(threshold)));
    for wires in &got {
        assert!(wires.iter().all(|(len, _)| *len > 100), "{got:?}");
        assert!(wires[0].0 > 20_000 && wires[1].0 > 20_000, "{got:?}");
    }
    assert_eq!(got, [GOLDEN_PLAIN, GOLDEN_COMPRESSED], "{got:?}");
}

/// Recorded from the code before bodies were shared: (bytes, XXH3-64) per guest.
const GOLDEN_PLAIN: [(usize, u64); 4] = [
    (345202, 16174403723829076939),
    (447477, 1192182911043227177),
    (159, 5588722873675948582),
    (348928, 319723604551496966),
];
const GOLDEN_COMPRESSED: [(usize, u64); 4] = [
    (241431, 12058507524931665851),
    (241457, 990453012972211213),
    (173, 1205002214307861980),
    (241029, 13738444853290308447),
];

#[test]
fn the_tracked_square_holds_every_column_the_player_tracks() {
    let (mut hub, _) = hub_with_sync(SyncData::empty());
    for view in 2..=16 {
        hub.guests[0].view_distance = view;
        hub.guests[0].center = Some((3, -4));
        let squares = hub.tracked_squares();
        for x in -25..=30 {
            for z in -30..=25 {
                if tracked(3, -4, view, x, z) {
                    assert!(in_square(&squares[0], x, z), "view {view} at {x} {z}");
                }
            }
        }
    }
    hub.guests[0].center = None;
    assert!(in_square(&hub.tracked_squares()[0], i32::MAX, i32::MIN));
}
