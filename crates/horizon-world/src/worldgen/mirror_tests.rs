use super::*;
use crate::worldgen::generator::{GeneratorData, GeneratorWorker, OverworldGenerator};
use std::path::{Path, PathBuf};

/// Horizon's ids as Java's: vanilla's states and fluids share their ids on both sides.
struct Same(usize);
impl StateMapping for Same {
    fn to_java(&self, state: BlockStateId) -> Option<u32> {
        ((state.0 as usize) < self.0).then_some(state.0)
    }
    fn to_native(&self, java: u32) -> Option<BlockStateId> {
        ((java as usize) < self.0).then_some(BlockStateId(java))
    }
    fn java_states(&self) -> usize {
        self.0
    }
    fn fluid_to_java(&self, fluid: u32) -> Option<u32> {
        Some(fluid)
    }
    fn fluid_to_native(&self, java: u32) -> Option<u32> {
        Some(java)
    }
}

fn data() -> Option<Arc<GeneratorData>> {
    let root = std::env::var_os("HORIZON_TEST_RESOURCES")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/horizon-client/26.3")
        });
    if !root.join("worldgen/world_presets.json").exists() {
        eprintln!("skipping: no worldgen bundle at {}", root.display());
        return None;
    }
    let registry = Box::leak(Box::new(crate::level::data::load_registry(&root).unwrap()));
    Some(GeneratorData::load(&root.join("worldgen"), registry).unwrap())
}

fn worker(data: &Arc<GeneratorData>, seed: i64) -> GeneratorWorker {
    OverworldGenerator::new(data.clone(), seed)
        .unwrap()
        .worker()
}

/// The 3×3 terrain-stage chunks around `center`, in `decorate`'s order.
fn region(worker: &mut GeneratorWorker, center: ChunkPos) -> [ProtoChunk; 9] {
    std::array::from_fn(|i| {
        let pos = ChunkPos::new(center.x + i as i32 % 3 - 1, center.z + i as i32 / 3 - 1);
        worker.generate_proto(pos).unwrap()
    })
}

fn same(data: &GeneratorData) -> Same {
    Same(data.registry().len())
}

fn assert_same_blocks(a: &ProtoChunk, b: &ProtoChunk) {
    let (mut left, mut right) = ([0u32; 4096], [0u32; 4096]);
    let min_y = a.terrain.min_y();
    for i in 0..a.terrain.height() / 16 {
        a.terrain.section_states(min_y + i * 16, &mut left);
        b.terrain.section_states(min_y + i * 16, &mut right);
        assert!(left == right, "section {i} of {:?}", a.position());
    }
}

fn tag(entries: &[(&str, Tag)]) -> Compound {
    entries
        .iter()
        .map(|(k, v)| (JavaString::from(*k), v.clone()))
        .collect()
}

#[test]
fn a_decorated_chunk_crosses_the_wire_whole() {
    let Some(data) = data() else { return };
    let states = same(&data);
    let mut first = worker(&data, 0);
    let mut chunks = region(&mut first, ChunkPos::new(0, 0));
    first.decorate(&mut chunks).unwrap();
    let mut source = std::mem::replace(
        &mut chunks[4],
        first.generate_proto(ChunkPos::new(0, 0)).unwrap(),
    );
    source.block_entities.push((
        [3, 70, 5],
        tag(&[("id", Tag::String("minecraft:chest".into()))]),
    ));
    source.entities.push(tag(&[(
        "Pos",
        Tag::List(
            [1.5f64, 70.0, 2.5]
                .iter()
                .map(|v| Tag::Double(v.to_bits()))
                .collect(),
        ),
    )]));
    source.terrain.block_ticks_mut().schedule_saved(SavedTick {
        kind: TickTypeId(1),
        pos: BlockPos::new(4, 60, 9),
        delay: 3,
        priority: TickPriority::High,
    });
    let bytes = encode_chunk(&source, &states, data.biome_names()).unwrap();
    if let Some(path) = std::env::var_os("HORIZON_WRITE_MIRROR_FIXTURE") {
        write_fixture(Path::new(&path), &data, &bytes);
    }

    // Another seed's chunk: other blocks and biomes, all of which the bytes replace.
    let mut target = worker(&data, 1)
        .generate_proto(ChunkPos::new(0, 0))
        .unwrap();
    assert!((0..16).any(|y| source.block(7, y * 8, 7) != target.block(7, y * 8, 7)));
    decode_chunk_into(&mut target, &bytes, &states).unwrap();
    assert_same_blocks(&source, &target);
    for i in 0..source.terrain.height() as usize / 16 {
        assert_eq!(
            source.biomes().section(i),
            target.biomes().section(i),
            "biomes of section {i}"
        );
    }
    assert_eq!(source.block_entities, target.block_entities);
    assert_eq!(source.entities, target.entities);
    assert_eq!(
        source.terrain.block_ticks().pack(0),
        target.terrain.block_ticks().pack(0)
    );
    assert_eq!(
        source.terrain.fluid_ticks().pack(0),
        target.terrain.fluid_ticks().pack(0)
    );
    assert!(
        !source.terrain.fluid_ticks().pack(0).is_empty()
            || !source.terrain.postprocessing().iter().all(Vec::is_empty)
    );
    assert_eq!(
        source.terrain.postprocessing(),
        target.terrain.postprocessing()
    );
    assert_eq!(
        source.terrain.generation_stage(),
        target.terrain.generation_stage()
    );
    for kind in HeightmapType::ALL {
        assert_eq!(
            source.terrain.heightmap(kind).map(|m| m.raw_data()),
            target.terrain.heightmap(kind).map(|m| m.raw_data()),
            "{kind:?}"
        );
    }
    let wrong = worker(&data, 1)
        .generate_proto(ChunkPos::new(1, 0))
        .unwrap();
    let mut wrong = wrong;
    assert!(matches!(
        decode_chunk_into(&mut wrong, &bytes, &states),
        Err(MirrorError::WrongChunk { .. })
    ));
}

/// The Java check's input (`testmod/res/mirror/chunk.bin.gz`): the session header, then the chunk.
fn write_fixture(path: &Path, data: &GeneratorData, chunk: &[u8]) {
    use std::io::Write as _;
    let header = SessionHeader {
        level: "minecraft:overworld".into(),
        biomes: data.biome_names().to_vec(),
        seed: 0,
    }
    .encode();
    let mut out = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    for part in [&header[..], chunk] {
        out.write_all(&(part.len() as i32).to_be_bytes()).unwrap();
        out.write_all(part).unwrap();
    }
    std::fs::write(path, out.finish().unwrap()).unwrap();
}

#[test]
fn a_write_log_lands_on_the_chunk_holding_each_position() {
    let Some(data) = data() else { return };
    let states = same(&data);
    let mut generator = worker(&data, 0);
    let center = ChunkPos::new(5, -3);
    let mut chunks = region(&mut generator, center);
    let stone = data.registry().default_state("minecraft:stone").unwrap();
    let glass = data.registry().default_state("minecraft:glass").unwrap();
    // North-west and south-east neighbours, and one block in the center.
    let west = BlockPos::new(4 * 16 + 3, 200, -4 * 16 + 7);
    let east = BlockPos::new(6 * 16 + 15, 201, -2 * 16);
    let middle = BlockPos::new(5 * 16 + 8, 202, -3 * 16 + 8);
    let mut log = WriteLog::default();
    log.record(Write::Block {
        pos: west,
        state: stone,
        flags: 3,
    });
    log.record(Write::Block {
        pos: east,
        state: glass,
        flags: 2,
    });
    log.record(Write::Block {
        pos: middle,
        state: stone,
        flags: 18,
    });
    log.record(Write::BlockEntitySet {
        pos: west,
        tag: tag(&[("id", Tag::String("minecraft:barrel".into()))]),
    });
    log.record(Write::BlockTick {
        pos: east,
        block: glass,
        delay: 2,
        priority: TickPriority::Low,
    });
    log.record(Write::FluidTick {
        pos: middle,
        fluid: 2,
        delay: 5,
        priority: TickPriority::Normal,
    });
    log.record(Write::PostProcess { pos: middle });
    let wire = log.encode(&states).unwrap();
    let back = WriteLog::decode(&wire, &states).unwrap();
    assert_eq!(back, log);
    back.apply(&mut chunks).unwrap();

    let at = |p: BlockPos| (p.chunk(), [p.x, p.y, p.z]);
    for (index, pos, state) in [(0, west, stone), (8, east, glass), (4, middle, stone)] {
        assert_eq!(chunks[index].position(), at(pos).0, "chunk {index}");
        assert_eq!(chunks[index].block(pos.x, pos.y, pos.z), state);
        // The same local spot in the other chunks is still air, high above the terrain.
        for (other, c) in chunks.iter().enumerate().filter(|&(i, _)| i != index) {
            let p = c.position();
            assert_ne!(
                c.block(p.x * 16 + (pos.x & 15), pos.y, p.z * 16 + (pos.z & 15)),
                state,
                "chunk {other}"
            );
        }
    }
    assert_eq!(
        chunks[0].block_entities.last().map(|(p, _)| *p),
        Some(at(west).1)
    );
    assert!(
        chunks[8]
            .terrain
            .block_ticks()
            .has_scheduled(east, TickTypeId(glass.0))
    );
    assert!(
        chunks[4]
            .terrain
            .fluid_ticks()
            .has_scheduled(middle, TickTypeId(2))
    );
    assert!(
        chunks[4]
            .terrain
            .postprocessing()
            .iter()
            .any(|l| !l.is_empty())
    );

    let outside = BlockPos::new(100 * 16, 64, 0);
    let mut stray = WriteLog::default();
    stray.record(Write::Block {
        pos: outside,
        state: stone,
        flags: 3,
    });
    assert!(matches!(
        stray.apply(&mut chunks),
        Err(MirrorError::MissingChunk(_))
    ));
}

#[test]
fn decoration_records_writes_that_replay_it_on_fresh_chunks() {
    let Some(data) = data() else { return };
    let states = same(&data);
    let center = ChunkPos::new(2, 1);
    let mut generator = worker(&data, 0);
    let mut fresh = region(&mut generator, center);
    let mut decorated = region(&mut generator, center);
    assert!(generator.record_writes(Some(WriteLog::default())).is_none());
    generator.decorate(&mut decorated).unwrap();
    let log = generator.record_writes(None).unwrap();
    assert!(
        log.writes()
            .iter()
            .any(|w| matches!(w, Write::Block { .. }))
    );

    WriteLog::decode(&log.encode(&states).unwrap(), &states)
        .unwrap()
        .apply(&mut fresh)
        .unwrap();
    let positions = |c: &ProtoChunk| {
        let mut list: Vec<_> = c.block_entities.iter().map(|(p, _)| *p).collect();
        list.sort();
        list
    };
    for (a, b) in decorated.iter().zip(&fresh) {
        assert_same_blocks(a, b);
        assert_eq!(positions(a), positions(b), "{:?}", a.position());
        assert_eq!(a.entities.len(), b.entities.len());
        assert_eq!(
            a.terrain.block_ticks().pack(0),
            b.terrain.block_ticks().pack(0)
        );
        assert_eq!(
            a.terrain.fluid_ticks().pack(0),
            b.terrain.fluid_ticks().pack(0)
        );
        assert_eq!(a.terrain.postprocessing(), b.terrain.postprocessing());
    }

    // Off by default: a worker that was never asked records nothing.
    let mut plain = region(&mut generator, ChunkPos::new(-4, 4));
    generator.decorate(&mut plain).unwrap();
    assert!(generator.record_writes(None).is_none());
}

#[test]
fn the_session_header_round_trips_and_rejects_another_format() {
    let header = SessionHeader {
        level: "minecraft:the_nether".into(),
        biomes: vec!["minecraft:plains".into(), "minecraft:desert".into()],
        seed: -7,
    };
    let bytes = header.encode();
    assert_eq!(&bytes[..4], &FORMAT.to_be_bytes());
    assert_eq!(SessionHeader::decode(&bytes).unwrap(), header);
    let mut other = bytes.clone();
    other[3] ^= 0x40;
    assert!(matches!(
        SessionHeader::decode(&other),
        Err(MirrorError::Format(_))
    ));
    assert!(matches!(
        SessionHeader::decode(&bytes[..bytes.len() - 1]),
        Err(MirrorError::Truncated)
    ));
}

#[test]
fn a_single_value_section_is_vanillas_network_form() {
    // `PalettedContainer.write` of an all-stone section: zero bits, the value as a VarInt, no longs.
    let mut out = Out::default();
    out.container(Strategy::blocks(15), &[300; 4096]);
    assert_eq!(out.0, [0, 0xac, 0x02]);
    // Two states: linear palette at 4 bits, its size and ids, then 256 longs.
    let mut values = [7u32; 4096];
    values[1] = 9;
    let mut out = Out::default();
    out.container(Strategy::blocks(15), &values);
    assert_eq!(&out.0[..4], &[4, 2, 7, 9]);
    assert_eq!(out.0.len(), 4 + 256 * 8);
    assert_eq!(&out.0[4..12], &(1u64 << 4).to_be_bytes());
    let mut back = [0u32; 4096];
    In(&out.0)
        .container(Strategy::blocks(15), &mut back)
        .unwrap();
    assert_eq!(back, values);
}

/// The bytes `ChunkMirrorCheck.java` hard-codes for the same header and biome section.
#[test]
fn a_header_and_a_biome_section_write_the_bytes_java_expects() {
    let header = SessionHeader {
        level: "minecraft:overworld".into(),
        biomes: vec!["minecraft:plains".into()],
        seed: 7,
    };
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        hex(&header.encode()),
        "00000001000000136d696e6563726166743a6f766572776f726c6400000001000000106d696e6563726166743a706c61696e730000000000000007"
    );
    let values: Vec<u32> = (0..64).map(|i| if i % 2 == 0 { 5 } else { 9 }).collect();
    let mut out = Out::default();
    out.container(Strategy::biomes(6), &values);
    assert_eq!(hex(&out.0), "01020509aaaaaaaaaaaaaaaa");
}
