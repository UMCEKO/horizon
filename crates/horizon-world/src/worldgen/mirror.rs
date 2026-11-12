//! Proto chunks and region writes on the wire between native generation and Java's `ProtoChunk` (`ChunkMirror.java`).
use super::{
    biome::BiomeId,
    generator::ProtoChunk,
    generator_decoration::block_of,
    terrain::{PreLightChunkStage, TerrainError},
};
use crate::{heightmap::HeightmapType, registry::BlockStateId};
use horizon_core::{
    nbt::{self, Compound, JavaString, NamedTag, Tag},
    paletted::{PaletteError, PalettedContainer, Strategy, ceil_log2, storage_len},
    position::{BlockPos, ChunkPos},
    ticks::{SavedTick, TickPriority, TickTypeId},
};
use std::{collections::BTreeMap, sync::Arc};

/// The session header's `format`: bump when the layout below changes.
pub const FORMAT: i32 = 1;

/// Native block states against Java's, without `horizon-world` depending on the host.
pub trait StateMapping {
    fn to_java(&self, state: BlockStateId) -> Option<u32>;
    fn to_native(&self, java: u32) -> Option<BlockStateId>;
    /// `Block.BLOCK_STATE_REGISTRY.size()`: the global palette's width follows it.
    fn java_states(&self) -> usize;
    /// A fluid tick's native `FluidKind::tick_id` as Java's `BuiltInRegistries.FLUID` id.
    fn fluid_to_java(&self, fluid: u32) -> Option<u32>;
    fn fluid_to_native(&self, java: u32) -> Option<u32>;
}

/// Biome names in the export's order: a biome value on the wire is an index into them.
pub type BiomeNames = [Arc<str>];

#[derive(Debug)]
pub enum MirrorError {
    Truncated,
    Format(i32),
    WrongChunk { expected: ChunkPos, found: ChunkPos },
    WrongHeight { min_y: i32, sections: i32 },
    NoJavaState(BlockStateId),
    NoNativeState(u32),
    NoFluid(u32),
    Biome(u32),
    Status(u8),
    Entry(u8),
    MissingChunk(ChunkPos),
    Palette(PaletteError),
    Nbt(String),
    Terrain(TerrainError),
}
impl std::fmt::Display for MirrorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for MirrorError {}
impl From<PaletteError> for MirrorError {
    fn from(e: PaletteError) -> Self {
        Self::Palette(e)
    }
}
impl From<TerrainError> for MirrorError {
    fn from(e: TerrainError) -> Self {
        Self::Terrain(e)
    }
}
impl From<nbt::Error> for MirrorError {
    fn from(e: nbt::Error) -> Self {
        Self::Nbt(format!("{e:?}"))
    }
}

#[derive(Default)]
struct Out(Vec<u8>);
impl Out {
    fn byte(&mut self, v: u8) {
        self.0.push(v);
    }
    fn short(&mut self, v: i16) {
        self.0.extend(v.to_be_bytes());
    }
    fn int(&mut self, v: i32) {
        self.0.extend(v.to_be_bytes());
    }
    fn long(&mut self, v: i64) {
        self.0.extend(v.to_be_bytes());
    }
    fn bool(&mut self, v: bool) {
        self.0.push(v as u8);
    }
    fn len(&mut self, n: usize) {
        self.int(n as i32);
    }
    fn bytes(&mut self, v: &[u8]) {
        self.len(v.len());
        self.0.extend(v);
    }
    fn text(&mut self, v: &str) {
        self.bytes(v.as_bytes());
    }
    fn tag(&mut self, tag: &Compound) -> Result<(), MirrorError> {
        let named = NamedTag {
            name: JavaString::from(""),
            tag: Tag::Compound(tag.clone()),
        };
        self.bytes(&nbt::write_named(&named, nbt::Limits::saved())?);
        Ok(())
    }
    /// `FriendlyByteBuf.writeVarInt`.
    fn var_int(&mut self, v: u32) {
        let mut v = v;
        while v >= 0x80 {
            self.0.push((v as u8 & 0x7f) | 0x80);
            v >>= 7;
        }
        self.0.push(v as u8);
    }
    /// `PalettedContainer.write`: the storage's bits, its palette, then its longs with no count.
    fn container(&mut self, strategy: Strategy, values: &[u32]) {
        let container = PalettedContainer::from_values(strategy, values);
        let (bits, palette, words) = container.to_wire();
        self.byte(bits);
        if bits == 0 {
            self.var_int(palette[0]);
        } else if strategy.wire_has_palette(bits) {
            self.var_int(palette.len() as u32);
            palette.iter().for_each(|&v| self.var_int(v));
        }
        words.iter().for_each(|&w| self.long(w as i64));
    }
}

struct In<'a>(&'a [u8]);
impl<'a> In<'a> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], MirrorError> {
        let (head, rest) = self.0.split_first_chunk().ok_or(MirrorError::Truncated)?;
        self.0 = rest;
        Ok(*head)
    }
    fn byte(&mut self) -> Result<u8, MirrorError> {
        Ok(self.take::<1>()?[0])
    }
    fn short(&mut self) -> Result<i16, MirrorError> {
        self.take().map(i16::from_be_bytes)
    }
    fn int(&mut self) -> Result<i32, MirrorError> {
        self.take().map(i32::from_be_bytes)
    }
    fn long(&mut self) -> Result<i64, MirrorError> {
        self.take().map(i64::from_be_bytes)
    }
    fn bool(&mut self) -> Result<bool, MirrorError> {
        Ok(self.byte()? != 0)
    }
    fn len(&mut self) -> Result<usize, MirrorError> {
        usize::try_from(self.int()?).map_err(|_| MirrorError::Truncated)
    }
    fn bytes(&mut self) -> Result<&'a [u8], MirrorError> {
        let len = self.len()?;
        let (head, rest) = self.0.split_at_checked(len).ok_or(MirrorError::Truncated)?;
        self.0 = rest;
        Ok(head)
    }
    fn text(&mut self) -> Result<&'a str, MirrorError> {
        std::str::from_utf8(self.bytes()?).map_err(|e| MirrorError::Nbt(e.to_string()))
    }
    fn tag(&mut self) -> Result<Compound, MirrorError> {
        let (named, _) = nbt::read_named(self.bytes()?, nbt::Limits::saved())?;
        match named.tag {
            Tag::Compound(tag) => Ok(tag),
            _ => Err(MirrorError::Nbt("not a compound".into())),
        }
    }
    fn var_int(&mut self) -> Result<u32, MirrorError> {
        let mut value = 0u32;
        for shift in (0..35).step_by(7) {
            let b = self.byte()?;
            value |= u32::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(MirrorError::Truncated)
    }
    /// `PalettedContainer.read` into one value per entry.
    fn container(&mut self, strategy: Strategy, out: &mut [u32]) -> Result<(), MirrorError> {
        let bits = self.byte()?;
        let palette = if bits == 0 {
            vec![self.var_int()?]
        } else if strategy.wire_has_palette(bits) {
            let n = self.var_int()? as usize;
            (0..n).map(|_| self.var_int()).collect::<Result<_, _>>()?
        } else {
            Vec::new()
        };
        let words = storage_len(strategy.wire_storage_bits(bits), strategy.entries());
        let words = (0..words)
            .map(|_| self.long().map(|w| w as u64))
            .collect::<Result<_, _>>()?;
        PalettedContainer::from_wire(strategy, bits, palette, words)?.unpack(out);
        Ok(())
    }
}

/// `ChunkPos.pack`.
fn pack_chunk(p: ChunkPos) -> i64 {
    i64::from(p.x as u32) | i64::from(p.z) << 32
}
fn priority(byte: u8) -> TickPriority {
    TickPriority::by_value(i32::from(byte as i8))
}
fn block_pos([x, y, z]: [i32; 3]) -> BlockPos {
    BlockPos::new(x, y, z)
}
fn heightmap_words(height: i32) -> usize {
    storage_len(ceil_log2(height as usize + 1), 256)
}

/// Once per region session: what both sides need before any chunk or write log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionHeader {
    pub level: String,
    pub biomes: Vec<Arc<str>>,
    pub seed: i64,
}
impl SessionHeader {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Out::default();
        out.int(FORMAT);
        out.text(&self.level);
        out.len(self.biomes.len());
        self.biomes.iter().for_each(|b| out.text(b));
        out.long(self.seed);
        out.0
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, MirrorError> {
        let mut input = In(bytes);
        let format = input.int()?;
        if format != FORMAT {
            return Err(MirrorError::Format(format));
        }
        let level = input.text()?.to_owned();
        let n = input.len()?;
        let biomes = (0..n)
            .map(|_| input.text().map(Arc::from))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            level,
            biomes,
            seed: input.long()?,
        })
    }
}

/// The whole chunk: sections, heightmaps, block entities, entities, ticks, marks, status and structures.
pub fn encode_chunk(
    chunk: &ProtoChunk,
    states: &dyn StateMapping,
    biomes: &BiomeNames,
) -> Result<Vec<u8>, MirrorError> {
    let terrain = &chunk.terrain;
    let position = chunk.position();
    let (min_y, height) = (terrain.min_y(), terrain.height());
    let sections = height / 16;
    let mut out = Out::default();
    out.int(position.x);
    out.int(position.z);
    out.int(min_y);
    out.int(sections);
    let block_strategy = Strategy::blocks(ceil_log2(states.java_states()));
    let biome_strategy = Strategy::biomes(ceil_log2(biomes.len()));
    let mut blocks = [0u32; 4096];
    // Most sections hold a few states: map each distinct one once.
    let mut last = (u32::MAX, 0u32);
    for i in 0..sections {
        out.bool(true);
        terrain.section_states(min_y + i * 16, &mut blocks);
        for value in &mut blocks {
            if last.0 != *value {
                let java = states
                    .to_java(BlockStateId(*value))
                    .ok_or(MirrorError::NoJavaState(BlockStateId(*value)))?;
                last = (*value, java);
            }
            *value = last.1;
        }
        out.container(block_strategy, &blocks);
        let section = chunk
            .biomes()
            .section(i as usize)
            .ok_or(MirrorError::WrongHeight { min_y, sections })?;
        let values: Vec<u32> = section.iter().map(|b| b.0).collect();
        out.container(biome_strategy, &values);
    }
    let maps: Vec<_> = HeightmapType::ALL
        .into_iter()
        .filter_map(|kind| terrain.heightmap(kind))
        .collect();
    out.len(maps.len());
    for map in maps {
        out.byte(map.kind() as u8);
        map.raw_data().iter().for_each(|&w| out.long(w as i64));
    }
    out.len(chunk.block_entities.len());
    for (pos, tag) in &chunk.block_entities {
        out.long(block_pos(*pos).pack());
        out.tag(tag)?;
    }
    out.len(chunk.entities.len());
    for tag in &chunk.entities {
        out.tag(tag)?;
    }
    let block_ticks = terrain.block_ticks().pack(0);
    out.len(block_ticks.len());
    for tick in block_ticks {
        let java = states
            .to_java(BlockStateId(tick.kind.0))
            .ok_or(MirrorError::NoJavaState(BlockStateId(tick.kind.0)))?;
        tick_out(&mut out, tick, java);
    }
    let fluid_ticks = terrain.fluid_ticks().pack(0);
    out.len(fluid_ticks.len());
    for tick in fluid_ticks {
        let java = states
            .fluid_to_java(tick.kind.0)
            .ok_or(MirrorError::NoFluid(tick.kind.0))?;
        tick_out(&mut out, tick, java);
    }
    let marks: Vec<(u16, u8)> = terrain
        .postprocessing()
        .iter()
        .enumerate()
        .flat_map(|(i, list)| list.iter().map(move |&p| (p, i as u8)))
        .collect();
    out.len(marks.len());
    for (packed, section) in marks {
        out.short(packed as i16);
        out.byte(section);
    }
    out.byte(terrain.generation_stage().status().index() as u8);
    let starts: Vec<_> = chunk
        .own_starts()
        .into_iter()
        .filter_map(|start| match start.save() {
            Some(Tag::Compound(tag)) => Some((start.name.clone(), tag)),
            _ => {
                log::warn!(
                    "Horizon: structure start {} not mirrored: one of its pieces cannot be saved",
                    start.name
                );
                None
            }
        })
        .collect();
    out.len(starts.len());
    for (id, tag) in &starts {
        out.text(id);
        out.tag(tag)?;
    }
    let mut references: BTreeMap<&str, Vec<i64>> = BTreeMap::new();
    for start in &chunk.structure_starts {
        references
            .entry(&start.name)
            .or_default()
            .push(pack_chunk(start.chunk));
    }
    out.len(references.len());
    for (id, chunks) in references {
        out.text(id);
        out.len(chunks.len());
        chunks.iter().for_each(|&c| out.long(c));
    }
    Ok(out.0)
}
fn tick_out(out: &mut Out, tick: &SavedTick, java: u32) {
    out.long(tick.pos.pack());
    out.int(java as i32);
    out.int(tick.delay);
    out.byte(tick.priority as i8 as u8);
}
fn tick_in(
    input: &mut In<'_>,
    kind: impl Fn(u32) -> Result<u32, MirrorError>,
) -> Result<SavedTick, MirrorError> {
    let pos = BlockPos::unpack(input.long()?);
    let kind = TickTypeId(kind(input.int()? as u32)?);
    Ok(SavedTick {
        kind,
        pos,
        delay: input.int()?,
        priority: priority(input.byte()?),
    })
}

/// Replaces all but the structures, which native can't rebuild from their tags: it keeps its own.
pub fn decode_chunk_into(
    chunk: &mut ProtoChunk,
    bytes: &[u8],
    states: &dyn StateMapping,
) -> Result<(), MirrorError> {
    let mut input = In(bytes);
    let found = ChunkPos::new(input.int()?, input.int()?);
    if found != chunk.position() {
        return Err(MirrorError::WrongChunk {
            expected: chunk.position(),
            found,
        });
    }
    let (min_y, sections) = (input.int()?, input.int()?);
    if min_y != chunk.terrain.min_y() || sections != chunk.terrain.height() / 16 {
        return Err(MirrorError::WrongHeight { min_y, sections });
    }
    let block_strategy = Strategy::blocks(ceil_log2(states.java_states()));
    let names = chunk.data().biome_names().len();
    let biome_strategy = Strategy::biomes(ceil_log2(names));
    let (bx, bz) = (found.x * 16, found.z * 16);
    let mut blocks = [0u32; 4096];
    let mut current = [0u32; 4096];
    let mut biomes = [0u32; 64];
    for i in 0..sections {
        if !input.bool()? {
            continue;
        }
        input.container(block_strategy, &mut blocks)?;
        let y0 = min_y + i * 16;
        chunk.terrain.section_states(y0, &mut current);
        let mut last = (u32::MAX, BlockStateId(0));
        for (index, (&java, &old)) in blocks.iter().zip(&current).enumerate() {
            if last.0 != java {
                let native = states
                    .to_native(java)
                    .ok_or(MirrorError::NoNativeState(java))?;
                last = (java, native);
            }
            if last.1.0 != old {
                let (x, y, z) = (index & 15, index >> 8, (index >> 4) & 15);
                chunk
                    .terrain
                    .set_block_raw(bx + x as i32, y0 + y as i32, bz + z as i32, last.1)?;
            }
        }
        input.container(biome_strategy, &mut biomes)?;
        if let Some(&bad) = biomes.iter().find(|&&b| b as usize >= names) {
            return Err(MirrorError::Biome(bad));
        }
        let section = chunk
            .biomes_mut()
            .section_mut(i as usize)
            .ok_or(MirrorError::WrongHeight { min_y, sections })?;
        for (cell, &b) in section.iter_mut().zip(&biomes) {
            *cell = BiomeId(b);
        }
    }
    let words = heightmap_words(chunk.terrain.height());
    let mut sent = Vec::new();
    for _ in 0..input.len()? {
        let kind = input.byte()?;
        let kind = *HeightmapType::ALL
            .get(kind as usize)
            .ok_or(MirrorError::Entry(kind))?;
        let raw = (0..words)
            .map(|_| input.long().map(|w| w as u64))
            .collect::<Result<Vec<_>, _>>()?;
        chunk.terrain.set_heightmap_raw(kind, &raw)?;
        sent.push(kind);
    }
    let stale: Vec<_> = HeightmapType::ALL
        .into_iter()
        .filter(|&k| chunk.terrain.heightmap(k).is_some() && !sent.contains(&k))
        .collect();
    chunk.terrain.prime_heightmaps(&stale)?;
    chunk.block_entities = (0..input.len()?)
        .map(|_| {
            let pos = BlockPos::unpack(input.long()?);
            Ok(([pos.x, pos.y, pos.z], input.tag()?))
        })
        .collect::<Result<_, MirrorError>>()?;
    chunk.entities = (0..input.len()?)
        .map(|_| input.tag())
        .collect::<Result<_, _>>()?;
    let block_ticks = (0..input.len()?)
        .map(|_| {
            tick_in(&mut input, |java| {
                states
                    .to_native(java)
                    .map(|s| s.0)
                    .ok_or(MirrorError::NoNativeState(java))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    *chunk.terrain.block_ticks_mut() =
        horizon_core::ticks::ProtoChunkTicks::from_saved(block_ticks);
    let fluid_ticks = (0..input.len()?)
        .map(|_| {
            tick_in(&mut input, |java| {
                states
                    .fluid_to_native(java)
                    .ok_or(MirrorError::NoFluid(java))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    *chunk.terrain.fluid_ticks_mut() =
        horizon_core::ticks::ProtoChunkTicks::from_saved(fluid_ticks);
    chunk.terrain.clear_postprocessing();
    for _ in 0..input.len()? {
        let packed = input.short()? as u16;
        let section = i32::from(input.byte()?);
        let (x, y, z) = (packed & 15, (packed >> 4) & 15, (packed >> 8) & 15);
        chunk.terrain.mark_postprocessing(
            bx + i32::from(x),
            min_y + section * 16 + i32::from(y),
            bz + i32::from(z),
        );
    }
    let status = input.byte()?;
    let stage = *PreLightChunkStage::ALL
        .get(status as usize)
        .ok_or(MirrorError::Status(status))?;
    chunk.terrain.set_generation_stage(stage);
    for _ in 0..input.len()? {
        input.text()?;
        input.bytes()?;
    }
    for _ in 0..input.len()? {
        input.text()?;
        for _ in 0..input.len()? {
            input.long()?;
        }
    }
    Ok(())
}

/// One write in a region session, with native ids; the wire form carries Java's.
#[derive(Clone, Debug, PartialEq)]
pub enum Write {
    Block {
        pos: BlockPos,
        state: BlockStateId,
        flags: i32,
    },
    BlockEntitySet {
        pos: BlockPos,
        tag: Compound,
    },
    BlockEntityRemoved {
        pos: BlockPos,
    },
    EntityAdded {
        tag: Compound,
    },
    /// `block` is the block's default state, as proto ticks name blocks.
    BlockTick {
        pos: BlockPos,
        block: BlockStateId,
        delay: i32,
        priority: TickPriority,
    },
    /// `fluid` is native's `FluidKind::tick_id`.
    FluidTick {
        pos: BlockPos,
        fluid: u32,
        delay: i32,
        priority: TickPriority,
    },
    PostProcess {
        pos: BlockPos,
    },
}

/// A region session's writes since the last turn, in the order they happened.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WriteLog {
    writes: Vec<Write>,
}
impl WriteLog {
    pub fn record(&mut self, write: Write) {
        self.writes.push(write);
    }
    pub fn writes(&self) -> &[Write] {
        &self.writes
    }
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }
    pub fn clear(&mut self) {
        self.writes.clear();
    }
    pub fn encode(&self, states: &dyn StateMapping) -> Result<Vec<u8>, MirrorError> {
        let java = |s: BlockStateId| states.to_java(s).ok_or(MirrorError::NoJavaState(s));
        let mut out = Out::default();
        out.len(self.writes.len());
        for write in &self.writes {
            match write {
                Write::Block { pos, state, flags } => {
                    out.byte(0);
                    out.long(pos.pack());
                    out.int(java(*state)? as i32);
                    out.int(*flags);
                }
                Write::BlockEntitySet { pos, tag } => {
                    out.byte(1);
                    out.long(pos.pack());
                    out.tag(tag)?;
                }
                Write::BlockEntityRemoved { pos } => {
                    out.byte(2);
                    out.long(pos.pack());
                }
                Write::EntityAdded { tag } => {
                    out.byte(3);
                    out.tag(tag)?;
                }
                Write::BlockTick {
                    pos,
                    block,
                    delay,
                    priority,
                } => {
                    out.byte(4);
                    out.long(pos.pack());
                    out.int(java(*block)? as i32);
                    out.int(*delay);
                    out.byte(*priority as i8 as u8);
                }
                Write::FluidTick {
                    pos,
                    fluid,
                    delay,
                    priority,
                } => {
                    out.byte(5);
                    out.long(pos.pack());
                    let id = states
                        .fluid_to_java(*fluid)
                        .ok_or(MirrorError::NoFluid(*fluid))?;
                    out.int(id as i32);
                    out.int(*delay);
                    out.byte(*priority as i8 as u8);
                }
                Write::PostProcess { pos } => {
                    out.byte(6);
                    out.long(pos.pack());
                }
            }
        }
        Ok(out.0)
    }
    pub fn decode(bytes: &[u8], states: &dyn StateMapping) -> Result<Self, MirrorError> {
        let native = |java: u32| {
            states
                .to_native(java)
                .ok_or(MirrorError::NoNativeState(java))
        };
        let mut input = In(bytes);
        let n = input.len()?;
        let mut writes = Vec::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            let kind = input.byte()?;
            writes.push(match kind {
                0 => Write::Block {
                    pos: BlockPos::unpack(input.long()?),
                    state: native(input.int()? as u32)?,
                    flags: input.int()?,
                },
                1 => Write::BlockEntitySet {
                    pos: BlockPos::unpack(input.long()?),
                    tag: input.tag()?,
                },
                2 => Write::BlockEntityRemoved {
                    pos: BlockPos::unpack(input.long()?),
                },
                3 => Write::EntityAdded { tag: input.tag()? },
                4 => Write::BlockTick {
                    pos: BlockPos::unpack(input.long()?),
                    block: native(input.int()? as u32)?,
                    delay: input.int()?,
                    priority: priority(input.byte()?),
                },
                5 => {
                    let pos = BlockPos::unpack(input.long()?);
                    let java = input.int()? as u32;
                    Write::FluidTick {
                        pos,
                        fluid: states
                            .fluid_to_native(java)
                            .ok_or(MirrorError::NoFluid(java))?,
                        delay: input.int()?,
                        priority: priority(input.byte()?),
                    }
                }
                6 => Write::PostProcess {
                    pos: BlockPos::unpack(input.long()?),
                },
                other => return Err(MirrorError::Entry(other)),
            });
        }
        Ok(Self { writes })
    }
    /// Replays each write on the chunk holding it, then primes touched heightmaps as `restore_proto` does.
    pub fn apply(&self, region: &mut [ProtoChunk]) -> Result<(), MirrorError> {
        let mut touched = Vec::new();
        for write in &self.writes {
            let pos = match write {
                Write::EntityAdded { tag } => match block_of(tag) {
                    Some(at) => block_pos(at),
                    None => continue,
                },
                Write::Block { pos, .. }
                | Write::BlockEntitySet { pos, .. }
                | Write::BlockEntityRemoved { pos }
                | Write::BlockTick { pos, .. }
                | Write::FluidTick { pos, .. }
                | Write::PostProcess { pos } => *pos,
            };
            let at = pos.chunk();
            let index = region
                .iter()
                .position(|c| c.position() == at)
                .ok_or(MirrorError::MissingChunk(at))?;
            let chunk = &mut region[index];
            let xyz = [pos.x, pos.y, pos.z];
            match write {
                Write::Block { state, .. } => {
                    chunk.terrain.set_block_raw(pos.x, pos.y, pos.z, *state)?;
                    if !touched.contains(&index) {
                        touched.push(index);
                    }
                }
                Write::BlockEntitySet { tag, .. } => {
                    chunk.block_entities.retain(|(p, _)| *p != xyz);
                    chunk.block_entities.push((xyz, tag.clone()));
                }
                Write::BlockEntityRemoved { .. } => chunk.block_entities.retain(|(p, _)| *p != xyz),
                Write::EntityAdded { tag } => chunk.entities.push(tag.clone()),
                Write::BlockTick {
                    block,
                    delay,
                    priority,
                    ..
                } => {
                    chunk.terrain.block_ticks_mut().schedule_saved(SavedTick {
                        kind: TickTypeId(block.0),
                        pos,
                        delay: *delay,
                        priority: *priority,
                    });
                }
                Write::FluidTick {
                    fluid,
                    delay,
                    priority,
                    ..
                } => {
                    chunk.terrain.fluid_ticks_mut().schedule_saved(SavedTick {
                        kind: TickTypeId(*fluid),
                        pos,
                        delay: *delay,
                        priority: *priority,
                    });
                }
                Write::PostProcess { .. } => chunk.terrain.mark_postprocessing(pos.x, pos.y, pos.z),
            }
        }
        for index in touched {
            let terrain = &mut region[index].terrain;
            let stage = terrain.generation_stage().heightmaps();
            let kinds: Vec<_> = HeightmapType::ALL
                .into_iter()
                .filter(|&k| terrain.heightmap(k).is_some() || stage.contains(&k))
                .collect();
            terrain.prime_heightmaps(&kinds)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "mirror_tests.rs"]
mod tests;
