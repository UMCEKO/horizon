//! Geometry the Java client bakes from a mod block entity's data (cable parts, inserted cells): the
//! replies are parsed here into `dynamic_models`, which the mesher draws from, and the sections remesh.

use super::*;
use horizon_assets::item_model::FoilType;
use horizon_assets::model::{BakedLayer, BakedQuadData, Direction};
use horizon_client::item_render::{ItemRenderState, LayerGeometry, LayerRender, RecordedQuad};
use horizon_client::{dynamic_models, dynamic_renders};
use horizon_java_host::wire::In;
use std::cell::RefCell;

/// A stack (its saved form) and an `ItemDisplayContext` ordinal.
pub(in crate::game) type ItemKey = (Vec<u8>, u8);

/// One piece of a recorded item: its quads in the frame of a pose, and how that pose moves.
struct ItemPiece {
    motion: dynamic_renders::Draw,
    foil: FoilType,
    quads: Arc<[RecordedQuad]>,
    extents: Arc<Vec<glam::Vec3>>,
}

/// `ItemStackRenderState` as the Java client filled it for a stack.
struct ItemRecording {
    uses_block_light: bool,
    oversized_in_gui: bool,
    animated: bool,
    pieces: Vec<ItemPiece>,
    /// Game time the recording arrived.
    since: i64,
}

/// Mod items only their own code can draw: which they are, what the Java client recorded of them, and what to ask it next.
#[derive(Default)]
struct ItemRecordings {
    external: std::collections::HashSet<String>,
    cache: HashMap<ItemKey, Option<Arc<ItemRecording>>>,
    /// Each item's last recording per context: a stack whose data just changed keeps drawing it until its own arrives.
    latest: HashMap<(String, u8), Arc<ItemRecording>>,
    queued: Vec<ItemKey>,
    decorated: std::collections::HashSet<String>,
    decorations: HashMap<Vec<u8>, Option<Arc<[u8]>>>,
    decorations_queued: Vec<Vec<u8>>,
}

thread_local! {
    static ITEMS: RefCell<ItemRecordings> = RefCell::default();
}

fn read_ids(input: &mut In) -> std::collections::HashSet<String> {
    let count = input.int().unwrap_or(0);
    (0..count)
        .map_while(|_| input.text().map(str::to_owned))
        .collect()
}

/// Takes the lists of item ids the Java client draws, and of those a mod's decorators draw over.
pub(in crate::game) fn java_items_external(reply: &[u8]) {
    let mut input = In(reply);
    let external = read_ids(&mut input);
    let decorated = read_ids(&mut input);
    ITEMS.with_borrow_mut(|items| {
        items.external = external;
        items.decorated = decorated;
    });
}

/// The queued stacks to record, as the request and its encoded form; None when there are none.
pub(in crate::game) fn java_items_request() -> Option<(Vec<ItemKey>, Vec<u8>)> {
    let keys = ITEMS.with_borrow_mut(|items| std::mem::take(&mut items.queued));
    if keys.is_empty() {
        return None;
    }
    let mut out = horizon_java_host::wire::Out::default();
    out.int(keys.len() as i32);
    for (nbt, context) in &keys {
        out.bytes(nbt);
        out.0.push(*context);
    }
    Some((keys, out.0))
}

/// The queued stacks whose decorations to record, as the request and its encoded form.
pub(in crate::game) fn java_decorations_request() -> Option<(Vec<Vec<u8>>, Vec<u8>)> {
    let keys = ITEMS.with_borrow_mut(|items| std::mem::take(&mut items.decorations_queued));
    if keys.is_empty() {
        return None;
    }
    let mut out = horizon_java_host::wire::Out::default();
    out.int(keys.len() as i32);
    for nbt in &keys {
        out.bytes(nbt);
    }
    Some((keys, out.0))
}

fn recorded_quad(input: &mut In) -> Option<RecordedQuad> {
    let texture = input.text()?.to_owned();
    let mut corners = [[0.0; 5]; 4];
    let mut color = 0;
    for corner in &mut corners {
        for value in corner.iter_mut() {
            *value = input.float()?;
        }
        color = input.int()? as u32;
    }
    let normal = [input.float()?, input.float()?, input.float()?];
    let translucent = input.bool()?;
    let cull = input.bool()?;
    Some(RecordedQuad {
        texture,
        corners,
        color,
        normal,
        translucent,
        cull,
    })
}

fn moves(op: &dynamic_renders::Op) -> bool {
    use dynamic_renders::{Op, Param};
    let varies = |p: &Param| !matches!(p, Param::Const(_));
    match op {
        Op::Translate(p) | Op::Scale(p) => p.iter().any(varies),
        Op::Rotate(_, p) => varies(p),
        Op::Quat(_) | Op::Matrix(_) => false,
    }
}

/// One stack's recording; `Some(None)` when its model failed in Java, None when the reply is cut short.
fn parse_recording(input: &mut In, since: i64) -> Option<Option<ItemRecording>> {
    let pieces = input.int()?;
    if pieces < 0 {
        return Some(None);
    }
    let uses_block_light = input.bool()?;
    let oversized_in_gui = input.bool()?;
    let mut list = Vec::new();
    for _ in 0..pieces {
        let ops = (0..input.int()?)
            .map(|_| op(input))
            .collect::<Option<Vec<_>>>()?;
        let foil = match input.byte()? {
            0 => FoilType::None,
            1 => FoilType::Standard,
            _ => FoilType::Special,
        };
        let quads = (0..input.int()?)
            .map(|_| recorded_quad(input))
            .collect::<Option<Vec<_>>>()?;
        let extents = quads
            .iter()
            .flat_map(|q| q.corners.map(|c| glam::Vec3::new(c[0], c[1], c[2])))
            .collect();
        list.push(ItemPiece {
            motion: dynamic_renders::Draw {
                translucent: false,
                ops,
                quads: Vec::new(),
                item: None,
            },
            foil,
            quads: quads.into(),
            extents: Arc::new(extents),
        });
    }
    Some(Some(ItemRecording {
        uses_block_light,
        oversized_in_gui,
        animated: list.iter().any(|p: &ItemPiece| p.motion.ops.iter().any(moves)),
        pieces: list,
        since,
    }))
}

/// Stores the recordings answering `keys`.
pub(in crate::game) fn java_items_apply(keys: Vec<ItemKey>, reply: &[u8], now: i64) {
    let mut input = In(reply);
    if input.int().is_none() {
        return;
    }
    let mut answers = Vec::new();
    for _ in 0..keys.len() {
        match parse_recording(&mut input, now) {
            Some(answer) => answers.push(answer.map(Arc::new)),
            None => {
                log::warn!("Horizon: a malformed Java items reply was dropped");
                return;
            }
        }
    }
    ITEMS.with_borrow_mut(|items| {
        if items.cache.len() > 4096 {
            items.cache.clear();
        }
        for ((nbt, context), answer) in keys.into_iter().zip(answers) {
            if let Some(recording) = &answer
                && let Some(id) = item_id(&nbt)
            {
                items.latest.insert((id, context), Arc::clone(recording));
            }
            items.cache.insert((nbt, context), answer);
        }
    });
}

/// The item id of a stack's saved form.
fn item_id(nbt: &[u8]) -> Option<String> {
    let tag = super::java_mods::read_tag(nbt)?;
    horizon_client::save::item_from_nbt(&tag).map(|stack| stack.item.to_string())
}

/// Stores the decorations answering `keys`.
pub(in crate::game) fn java_decorations_apply(keys: Vec<Vec<u8>>, reply: &[u8]) {
    let mut input = In(reply);
    if input.int().is_none() {
        return;
    }
    let mut answers = Vec::new();
    for _ in 0..keys.len() {
        let Some(length) = input.int() else {
            log::warn!("Horizon: a malformed Java decorations reply was dropped");
            return;
        };
        let Ok(length) = usize::try_from(length) else {
            answers.push(None);
            continue;
        };
        if input.0.len() < length {
            log::warn!("Horizon: a malformed Java decorations reply was dropped");
            return;
        }
        let (frame, rest) = input.0.split_at(length);
        input.0 = rest;
        answers.push(Some(Arc::<[u8]>::from(frame)));
    }
    ITEMS.with_borrow_mut(|items| {
        if items.decorations.len() > 4096 {
            items.decorations.clear();
        }
        items.decorations.extend(keys.into_iter().zip(answers));
    });
}

/// A stack's saved form with its count taken away: what its drawing can depend on besides the count.
fn stack_key(stack: &ItemStack) -> Option<Vec<u8>> {
    let mut one = stack.clone();
    one.count = 1;
    super::java_mods::write_tag(horizon_items::item_nbt::item_to_nbt(&one))
}

/// What a mod's decorators recorded over this stack, once the Java client answered; asks for it otherwise.
pub(in crate::game) fn java_decorations(stack: &ItemStack) -> Option<Arc<[u8]>> {
    if !ITEMS.with_borrow(|items| items.decorated.contains(&*stack.item)) {
        return None;
    }
    // A decorator sees the whole stack, count included.
    let key = super::java_mods::write_tag(horizon_items::item_nbt::item_to_nbt(stack))?;
    ITEMS.with_borrow_mut(|items| match items.decorations.get(&key) {
        Some(answer) => answer.clone(),
        None => {
            if !items.decorations_queued.contains(&key) {
                items.decorations_queued.push(key);
            }
            None
        }
    })
}

/// Whether item recordings are waiting to be asked for.
pub(in crate::game) fn java_items_queued() -> bool {
    ITEMS.with_borrow(|items| !items.queued.is_empty() || !items.decorations_queued.is_empty())
}

struct Entry {
    pos: [i32; 3],
    key: i32,
    quads: Option<(Vec<BakedQuadData>, Vec<i32>)>,
}

/// One quad and the tint color the block's code resolved for it (-1 for none).
fn quad(input: &mut In) -> Option<(BakedQuadData, i32)> {
    let mut vertices = [[0.0; 5]; 4];
    for value in vertices.iter_mut().flatten() {
        *value = input.float()?;
    }
    let sprite = input.text()?.to_owned();
    let face = *Direction::ALL.get(usize::from(input.byte()?))?;
    let cull = Direction::ALL.get(usize::from(input.byte()?)).copied();
    let tint = input.int()?;
    let color = input.int()?;
    let shade = input.bool()?;
    let emission = input.byte()?.min(15);
    let layer = match input.byte()? {
        0 => BakedLayer::Solid,
        1 => BakedLayer::Cutout,
        2 => BakedLayer::CutoutMipped,
        3 => BakedLayer::Translucent,
        4 => BakedLayer::Tripwire,
        _ => return None,
    };
    Some((
        BakedQuadData {
            vertices,
            sprite,
            face,
            cull,
            tint,
            shade,
            emission,
            layer,
        },
        color,
    ))
}

/// The models reply: entries, or None when it is cut short or malformed.
fn parse_models(reply: &[u8]) -> Option<Vec<Entry>> {
    let mut input = In(reply);
    let count = usize::try_from(input.int()?).ok()?;
    let mut entries = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let pos = BlockPos::unpack(input.long()?);
        let key = input.int()?;
        let quads = if input.bool()? {
            let quads = usize::try_from(input.int()?).ok()?;
            let mut list = Vec::with_capacity(quads.min(4096));
            let mut colors = Vec::with_capacity(quads.min(4096));
            for _ in 0..quads {
                let (quad, color) = quad(&mut input)?;
                list.push(quad);
                colors.push(color);
            }
            Some((list, colors))
        } else {
            None
        };
        entries.push(Entry {
            pos: [pos.x, pos.y, pos.z],
            key,
            quads,
        });
    }
    Some(entries)
}

/// The states reply as Horizon's ids; Java ids with no Horizon state are skipped.
fn parse_states(reply: &[u8], to_horizon: &[Option<BlockStateId>]) -> Option<Vec<BlockStateId>> {
    let mut input = In(reply);
    let count = input.int()?;
    let mut states = Vec::new();
    for _ in 0..count {
        let java = usize::try_from(input.int()?).ok()?;
        states.extend(to_horizon.get(java).copied().flatten());
    }
    Some(states)
}

fn param(input: &mut In) -> Option<dynamic_renders::Param> {
    use dynamic_renders::Param;
    Some(match input.float()? as i32 {
        0 => Param::Const(input.float()?),
        1 => Param::Linear(input.float()?, input.float()?),
        2 => Param::Sine(
            input.float()?,
            input.float()?,
            input.float()?,
            input.float()?,
        ),
        _ => return None,
    })
}

fn op(input: &mut In) -> Option<dynamic_renders::Op> {
    use dynamic_renders::Op;
    fn floats(input: &mut In, n: usize) -> Option<Vec<f32>> {
        (0..n).map(|_| input.float()).collect()
    }
    Some(match input.byte()? {
        b'T' => Op::Translate([param(input)?, param(input)?, param(input)?]),
        b'S' => Op::Scale([param(input)?, param(input)?, param(input)?]),
        b'R' => {
            let axis = floats(input, 3)?;
            Op::Rotate(
                glam::Vec3::new(axis[0], axis[1], axis[2]).normalize_or(glam::Vec3::Y),
                param(input)?,
            )
        }
        b'Q' => {
            let q = floats(input, 4)?;
            Op::Quat(glam::Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize())
        }
        b'M' => Op::Matrix(glam::Mat4::from_cols_slice(&floats(input, 16)?)),
        _ => return None,
    })
}

/// The renders reply: each position's pieces, or None there when nothing draws now.
fn parse_renders(reply: &[u8]) -> Option<Vec<([i32; 3], Option<Vec<dynamic_renders::Draw>>)>> {
    let mut input = In(reply);
    let count = usize::try_from(input.int()?).ok()?;
    let mut entries = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let pos = BlockPos::unpack(input.long()?);
        let pos = [pos.x, pos.y, pos.z];
        let draws = input.int()?;
        if draws < 0 {
            entries.push((pos, None));
            continue;
        }
        let mut list = Vec::new();
        for _ in 0..draws {
            let translucent = input.bool()?;
            let ops = (0..input.int()?)
                .map(|_| op(&mut input))
                .collect::<Option<Vec<_>>>()?;
            let quads = (0..input.int()?)
                .map(|_| {
                    let texture = input.text()?.to_owned();
                    let mut corners = [[0.0; 5]; 4];
                    let (mut color, mut light) = ([0; 4], [0; 4]);
                    for k in 0..4 {
                        for value in &mut corners[k] {
                            *value = input.float()?;
                        }
                        color[k] = input.int()? as u32;
                        light[k] = input.int()? as u32;
                    }
                    Some(dynamic_renders::Quad {
                        texture,
                        corners,
                        color,
                        light,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            let item = if input.bool()? {
                let tag = super::java_mods::read_tag(input.bytes()?);
                let (context, seed) = (input.byte()?, input.int()?);
                tag.and_then(|tag| horizon_client::save::item_from_nbt(&tag))
                    .map(|stack| dynamic_renders::Item {
                        stack,
                        context,
                        seed,
                    })
            } else {
                None
            };
            list.push(dynamic_renders::Draw {
                translucent,
                ops,
                quads,
                item,
            });
        }
        entries.push((pos, Some(list)));
    }
    Some(entries)
}

impl Game<'_> {
    /// `ItemModelResolver.updateForTopItem` of a mod's item that only its code can draw: what the Java client recorded, with the pieces where they are now; nothing until a first recording arrives.
    pub(in crate::game) fn java_item_state(
        &self,
        stack: &ItemStack,
        display: horizon_assets::item_model::DisplayContext,
        state: &mut ItemRenderState,
    ) {
        if !ITEMS.with_borrow(|items| items.external.contains(&*stack.item)) {
            return;
        }
        state.layers.clear();
        let Some(nbt) = stack_key(stack) else {
            return;
        };
        let Some(context) = horizon_assets::item_model::DisplayContext::ALL
            .iter()
            .position(|c| *c == display)
            .map(|c| c as u8)
        else {
            return;
        };
        let key = (nbt, context);
        let recording = ITEMS.with_borrow_mut(|items| match items.cache.get(&key) {
            Some(answer) => answer.clone(),
            None => {
                if !items.queued.contains(&key) {
                    items.queued.push(key);
                }
                items
                    .latest
                    .get(&(stack.item.to_string(), context))
                    .cloned()
            }
        });
        let Some(recording) = recording else {
            return;
        };
        let ticks = (self.world.game_time - recording.since) as f32 + self.clock.fraction() as f32;
        state.oversized_in_gui = recording.oversized_in_gui;
        state.animated = recording.animated;
        state.layers = recording
            .pieces
            .iter()
            .map(|piece| LayerRender {
                geometry: LayerGeometry::Recorded(Arc::clone(&piece.quads)),
                tints: Vec::new(),
                foil: piece.foil,
                matrix: piece.motion.matrix(ticks),
                particle: String::new(),
                uses_block_light: recording.uses_block_light,
                extents: Arc::clone(&piece.extents),
            })
            .collect();
    }

    /// Takes a renders reply: what mods' block entity renderers draw at each changed position.
    pub(in crate::game) fn java_renders_apply(&mut self, reply: &[u8]) {
        let Some(entries) = parse_renders(reply) else {
            log::warn!("Horizon: a malformed Java renders reply was dropped");
            return;
        };
        for (pos, draws) in entries {
            match draws {
                Some(draws) => dynamic_renders::set(pos, draws, self.world.game_time),
                None => dynamic_renders::remove(pos),
            }
        }
    }

    /// Takes a models reply: stores the geometry and remeshes the sections whose blocks changed.
    pub(in crate::game) fn java_models_apply(&mut self, reply: &[u8]) {
        let Some(entries) = parse_models(reply) else {
            log::warn!("Horizon: a malformed Java models reply was dropped");
            return;
        };
        let mut changed = Vec::new();
        for Entry { pos, key, quads } in entries {
            if key < 0 {
                if dynamic_models::place(pos, None) {
                    changed.push(pos);
                }
                continue;
            }
            if let Some((quads, colors)) = quads
                && dynamic_models::put_geometry(key, quads.into(), colors.into())
            {
                changed.extend(dynamic_models::positions_of(key));
            }
            if dynamic_models::place(pos, Some(key)) {
                changed.push(pos);
            }
        }
        for [x, y, z] in changed {
            self.world.mark_block_dirty(x, y, z);
        }
    }

    /// Takes the list of block states whose geometry the Java client bakes (Java state ids).
    pub(in crate::game) fn java_models_states(&mut self, reply: &[u8]) {
        let Some(map) = horizon_java_host::bridge::STATES.get() else {
            return;
        };
        match parse_states(reply, &map.to_horizon) {
            Some(states) => dynamic_models::set_dynamic_states(states),
            None => log::warn!("Horizon: a malformed Java dynamic states reply was dropped"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_java_host::wire::Out;

    fn write_quad(out: &mut Out, sprite: &str, cull: u8) {
        for v in 0..20 {
            out.float(v as f32 / 4.0);
        }
        out.text(sprite);
        out.0.extend([1, cull]);
        out.int(0x00ff_00ff);
        out.int(0x7f12_3456);
        out.bool(false);
        out.0.extend([7, 3]);
    }

    #[test]
    fn two_entries_share_one_key_and_one_quad_list() {
        let mut out = Out::default();
        out.int(3);
        out.long(BlockPos::new(-3, 64, 9).pack());
        out.int(5);
        out.bool(true);
        out.int(2);
        write_quad(&mut out, "ae2:block/cable", 6);
        write_quad(&mut out, "ae2:block/cable_glass", 4);
        out.long(BlockPos::new(100, -2, -50).pack());
        out.int(5);
        out.bool(false);
        out.long(BlockPos::new(1, 2, 3).pack());
        out.int(-1);
        out.bool(false);

        let entries = parse_models(&out.0).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].pos, [-3, 64, 9]);
        assert_eq!((entries[0].key, entries[1].key, entries[2].key), (5, 5, -1));
        assert_eq!(entries[1].pos, [100, -2, -50]);
        assert!(entries[1].quads.is_none() && entries[2].quads.is_none());
        let (quads, colors) = entries[0].quads.as_ref().unwrap();
        assert_eq!(quads.len(), 2);
        assert_eq!(colors, &[0x7f12_3456; 2]);
        assert_eq!(quads[0].vertices[3][4], 19.0 / 4.0);
        assert_eq!(quads[0].sprite, "ae2:block/cable");
        assert_eq!((quads[0].face, quads[0].cull), (Direction::Up, None));
        assert_eq!(quads[1].cull, Some(Direction::West));
        assert_eq!(quads[0].tint, 0x00ff_00ff);
        assert!(!quads[0].shade);
        assert_eq!(
            (quads[0].emission, quads[0].layer),
            (7, BakedLayer::Translucent)
        );

        assert!(parse_models(&out.0[..out.0.len() - 1]).is_none());
        assert!(parse_models(&[]).is_none());
    }

    #[test]
    fn states_map_to_horizon_ids_and_skip_unmapped_ones() {
        let mut out = Out::default();
        out.int(3);
        for java in [1, 2, 9] {
            out.int(java);
        }
        let map = [None, Some(BlockStateId(40)), None];
        assert_eq!(parse_states(&out.0, &map), Some(vec![BlockStateId(40)]));
        assert_eq!(parse_states(&out.0[..6], &map), None);
    }
}
