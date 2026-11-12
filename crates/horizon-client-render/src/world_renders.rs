//! What mods draw in the level's frame (their events' submitted geometry and draws on their own GPU buffers), recorded by the Java client as world-space geometry and drawn here every frame.
use crate::{hud::Rect, mesher::Vertex};
use glam::{DVec3, Mat3, Vec3};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, LazyLock, Mutex, RwLock,
        atomic::{AtomicU8, Ordering},
    },
};

/// At most this many quads and lines are drawn from one recording.
pub const MAX_QUADS: usize = 262_144;
pub const MAX_LINES: usize = 65_536;

/// The overlay of quads drawn before translucent terrain without depth writes.
const TRANSLUCENT_OVERLAY: u32 = crate::gfx::TRANSLUCENT_ITEM_OVERLAY;

/// The native pass a piece belongs to, as the Java side classified its pipeline's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    Opaque,
    Translucent,
    AfterTranslucent,
    Additive,
    SeeThrough,
    Lines,
    LinesOnTop,
    /// The custom sky, cloud and weather renderers' draws (anchored pieces).
    Sky,
    Clouds,
    Weather,
}
impl Pass {
    fn from_byte(b: u8) -> Option<Pass> {
        Some(match b {
            0 => Pass::Opaque,
            1 => Pass::Translucent,
            2 => Pass::AfterTranslucent,
            3 => Pass::Additive,
            4 => Pass::SeeThrough,
            5 => Pass::Lines,
            6 => Pass::LinesOnTop,
            7 => Pass::Sky,
            8 => Pass::Clouds,
            9 => Pass::Weather,
            _ => return None,
        })
    }
}

/// A recorded textured quad in its mesh's frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Quad {
    /// The sprite's texture: `namespace:path` under `textures/`.
    pub texture: String,
    /// Position and UV within the texture, per corner.
    pub corners: [[f32; 5]; 4],
    /// ARGB.
    pub color: [u32; 4],
    /// Lightmap coordinates as the renderer gave them; 0 takes the world's light.
    pub light: [u32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub ends: [f32; 6],
    pub color: u32,
    pub width: f32,
}

/// Geometry in a mesh's own frame, sent once.
#[derive(Debug)]
pub struct Mesh {
    pub quads: Vec<Quad>,
    pub lines: Vec<Line>,
    bounds: (Vec3, Vec3),
}

/// A mesh placed in the world: `origin + linear * local`.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub mesh: u64,
    pub pass: Pass,
    pub no_cull: bool,
    pub depth_write: bool,
    /// Placed relative to the camera, not the world.
    pub anchored: bool,
    pub origin: DVec3,
    pub linear: Mat3,
    pub modulator: [f32; 4],
}

#[derive(Default)]
struct Registry {
    generation: i32,
    meshes: HashMap<u64, Arc<Mesh>>,
    pieces: Vec<Piece>,
}

static REGISTRY: LazyLock<RwLock<Registry>> = LazyLock::new(|| {
    RwLock::new(Registry {
        generation: i32::MIN,
        ..Default::default()
    })
});
static WARNED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Mutex::default);

fn warn_once(what: &str) {
    if WARNED.lock().unwrap().insert(what.to_owned()) {
        log::warn!("Horizon: {what}");
    }
}

/// What the Java client's custom sky, cloud and weather renderers replaced, as the last tick's status says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Environment {
    pub sky: bool,
    pub clouds: bool,
    pub weather: bool,
    /// The rain tick (splash particles, rain sounds) is the mod's.
    pub rain_tick: bool,
}
impl Environment {
    pub fn from_bits(bits: i32) -> Self {
        Environment {
            sky: bits & 1 != 0,
            clouds: bits & 2 != 0,
            weather: bits & 4 != 0,
            rain_tick: bits & 8 != 0,
        }
    }
}

static ENVIRONMENT: AtomicU8 = AtomicU8::new(0);

pub fn set_environment(bits: i32) {
    ENVIRONMENT.store((bits & 15) as u8, Ordering::Relaxed);
}

pub fn environment() -> Environment {
    Environment::from_bits(i32::from(ENVIRONMENT.load(Ordering::Relaxed)))
}

pub fn clear() {
    ENVIRONMENT.store(0, Ordering::Relaxed);
    let mut r = REGISTRY.write().unwrap();
    r.meshes.clear();
    r.pieces.clear();
    r.generation = i32::MIN;
}

pub fn is_empty() -> bool {
    REGISTRY.read().unwrap().pieces.is_empty()
}

/// The pieces and meshes held, for the log.
pub fn counts() -> (usize, usize) {
    let r = REGISTRY.read().unwrap();
    (r.pieces.len(), r.meshes.len())
}

struct In<'a>(&'a [u8]);
impl<'a> In<'a> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_at_checked(N)?;
        self.0 = rest;
        head.try_into().ok()
    }
    fn byte(&mut self) -> Option<u8> {
        self.take::<1>().map(|b| b[0])
    }
    fn int(&mut self) -> Option<i32> {
        self.take().map(i32::from_be_bytes)
    }
    fn long(&mut self) -> Option<i64> {
        self.take().map(i64::from_be_bytes)
    }
    fn float(&mut self) -> Option<f32> {
        self.take().map(f32::from_be_bytes)
    }
    fn double(&mut self) -> Option<f64> {
        self.take().map(f64::from_be_bytes)
    }
    fn count(&mut self) -> Option<usize> {
        usize::try_from(self.int()?).ok()
    }
    fn text(&mut self) -> Option<String> {
        let n = self.count()?;
        let (head, rest) = self.0.split_at_checked(n)?;
        self.0 = rest;
        String::from_utf8(head.to_vec()).ok()
    }
    fn floats<const N: usize>(&mut self) -> Option<[f32; N]> {
        let mut out = [0.0; N];
        for v in &mut out {
            *v = self.float()?;
        }
        Some(out)
    }
}

fn mesh(input: &mut In) -> Option<(u64, Mesh)> {
    let id = input.long()? as u64;
    let n = input.count()?;
    let mut quads = Vec::with_capacity(n.min(MAX_QUADS));
    for _ in 0..n {
        let texture = input.text()?;
        let flat = input.floats::<20>()?;
        let mut corners = [[0.0; 5]; 4];
        for (k, c) in corners.iter_mut().enumerate() {
            c.copy_from_slice(&flat[k * 5..k * 5 + 5]);
        }
        let mut color = [0u32; 4];
        for c in &mut color {
            *c = input.int()? as u32;
        }
        let mut light = [0u32; 4];
        for l in &mut light {
            *l = input.int()? as u32;
        }
        if input.byte()? != 0 {
            input.floats::<3>()?;
        }
        quads.push(Quad {
            texture,
            corners,
            color,
            light,
        });
    }
    let n = input.count()?;
    let mut lines = Vec::with_capacity(n.min(MAX_LINES));
    for _ in 0..n {
        lines.push(Line {
            ends: input.floats::<6>()?,
            color: input.int()? as u32,
            width: input.float()?,
        });
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let points = quads
        .iter()
        .flat_map(|q| q.corners.iter().map(|c| Vec3::new(c[0], c[1], c[2])))
        .chain(lines.iter().flat_map(|l| {
            [
                Vec3::from_slice(&l.ends[..3]),
                Vec3::from_slice(&l.ends[3..]),
            ]
        }));
    for p in points {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    Some((
        id,
        Mesh {
            quads,
            lines,
            bounds: (lo, hi),
        },
    ))
}

/// Applies a recording's blob: the meshes it drops, the new ones, then the pieces. Unreadable bytes change nothing. An older generation than the one held is ignored.
pub fn apply(bytes: &[u8]) -> bool {
    let mut input = In(bytes);
    let Some(generation) = input.int() else {
        return false;
    };
    let mut dropped = Vec::new();
    let mut fresh = Vec::new();
    let mut pieces = Vec::new();
    let ok = (|| {
        for _ in 0..input.count()? {
            dropped.push(input.long()? as u64);
        }
        for _ in 0..input.count()? {
            fresh.push(mesh(&mut input)?);
        }
        for _ in 0..input.count()? {
            let mesh = input.long()? as u64;
            let pass = Pass::from_byte(input.byte()?)?;
            let flags = input.byte()?;
            let origin = DVec3::new(input.double()?, input.double()?, input.double()?);
            let linear = Mat3::from_cols_array(&input.floats::<9>()?);
            pieces.push(Piece {
                mesh,
                pass,
                no_cull: flags & 1 != 0,
                depth_write: flags & 2 != 0,
                anchored: flags & 4 != 0,
                origin,
                linear,
                modulator: input.floats::<4>()?,
            });
        }
        Some(())
    })();
    if ok.is_none() {
        log::warn!("Horizon: a world recording ended in pieces");
        return false;
    }
    let mut r = REGISTRY.write().unwrap();
    if generation <= r.generation {
        return false;
    }
    r.generation = generation;
    for id in dropped {
        r.meshes.remove(&id);
    }
    for (id, m) in fresh {
        r.meshes.insert(id, Arc::new(m));
    }
    let (mut quads, mut lines) = (0, 0);
    pieces.retain(|p| {
        let Some(m) = r.meshes.get(&p.mesh) else {
            return false;
        };
        if quads + m.quads.len() > MAX_QUADS || lines + m.lines.len() > MAX_LINES {
            warn_once("a world recording is over its limit, a piece is dropped");
            return false;
        }
        quads += m.quads.len();
        lines += m.lines.len();
        true
    });
    r.pieces = pieces;
    true
}

/// A line in the world, for the gizmos' line passes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldLine {
    pub start: [f64; 3],
    pub end: [f64; 3],
    pub color: u32,
    pub width: f32,
}

/// What the world draws the recordings from: the camera, the atlas's sprites, the world's light and the view.
pub struct Frame<'a> {
    pub camera: DVec3,
    pub sprites: &'a HashMap<String, Rect>,
    /// The world's packed light (`sky << 4 | block`) at a position.
    pub light: &'a dyn Fn([i32; 3]) -> u8,
    /// Whether a block's box may be on screen.
    pub visible: &'a dyn Fn([i32; 3]) -> bool,
}

/// Horizon's packed light (`sky*16 | block*16 << 8`) for a vertex: the brighter of the world's and the renderer's.
fn light(world: u8, coords: u32) -> u32 {
    let (world_sky, world_block) = (u32::from(world >> 4), u32::from(world & 15));
    let (block, sky) = ((coords & 0xFFFF) >> 4, (coords >> 16) >> 4);
    let (sky, block) = (world_sky.max(sky.min(15)), world_block.max(block.min(15)));
    (sky * 16) | ((block * 16) << 8)
}

/// What a frame's pieces draw into, per native pass.
#[derive(Default)]
pub struct Outputs {
    /// Opaque quads (overlay 0) and translucent ones (`TRANSLUCENT_ITEM_OVERLAY`), drawn with the entities.
    pub entities: Vec<Vertex>,
    /// `world_primitives::Batch::recorded`: after translucent terrain, blended without depth writes.
    pub after_translucent: Vec<Vertex>,
    /// `world_primitives::Batch::recorded_depth`: after translucent terrain, by a pipeline writing depth.
    pub after_translucent_depth: Vec<Vertex>,
    pub additive: Vec<Vertex>,
    pub see_through: Vec<Vertex>,
    /// Camera-relative quads of the custom sky, cloud and weather renderers.
    pub sky: Vec<Vertex>,
    pub clouds: Vec<Vertex>,
    pub weather: Vec<Vertex>,
    pub lines: Vec<WorldLine>,
    pub on_top: Vec<WorldLine>,
}
impl Outputs {
    pub fn clear(&mut self) {
        self.entities.clear();
        self.after_translucent.clear();
        self.after_translucent_depth.clear();
        self.additive.clear();
        self.see_through.clear();
        self.sky.clear();
        self.clouds.clear();
        self.weather.clear();
        self.lines.clear();
        self.on_top.clear();
    }
}

fn argb_to_rgba(argb: u32, modulator: [f32; 4]) -> u32 {
    let channel = |shift: u32, m: f32| (((argb >> shift) & 255) as f32 * m.clamp(0.0, 1.0)) as u32;
    channel(24, modulator[3]) << 24
        | channel(0, modulator[2]) << 16
        | channel(8, modulator[1]) << 8
        | channel(16, modulator[0])
}

/// Whether a box's cells, sampled at most five to an axis, are in view.
fn visible(frame: &Frame, lo: Vec3, hi: Vec3, to_world: &dyn Fn(Vec3) -> DVec3) -> bool {
    let size = hi - lo;
    let steps = |extent: f32| ((extent / 4.0).ceil() as usize).clamp(1, 4);
    let (nx, ny, nz) = (steps(size.x), steps(size.y), steps(size.z));
    for i in 0..=nx {
        for j in 0..=ny {
            for k in 0..=nz {
                let t = Vec3::new(
                    i as f32 / nx as f32,
                    j as f32 / ny as f32,
                    k as f32 / nz as f32,
                );
                let w = to_world(lo + size * t);
                if (frame.visible)([w.x.floor() as i32, w.y.floor() as i32, w.z.floor() as i32]) {
                    return true;
                }
            }
        }
    }
    false
}

/// Every recorded piece in view, as camera-relative vertices and lines per pass.
pub fn draw(frame: &Frame, out: &mut Outputs) {
    let registry = REGISTRY.read().unwrap();
    if registry.pieces.is_empty() {
        return;
    }
    let mut last_light: Option<([i32; 3], u8)> = None;
    for piece in &registry.pieces {
        let Some(mesh) = registry.meshes.get(&piece.mesh) else {
            continue;
        };
        let to_world = |p: Vec3| piece.origin + (piece.linear * p).as_dvec3();
        let (lo, hi) = mesh.bounds;
        if lo.x > hi.x || (!piece.anchored && !visible(frame, lo, hi, &to_world)) {
            continue;
        }
        let relative = if piece.anchored {
            piece.origin.as_vec3()
        } else {
            (piece.origin - frame.camera).as_vec3()
        };
        let place = |p: [f32; 3]| relative + piece.linear * Vec3::from_slice(&p);
        for line in mesh.lines.iter().filter(|_| !piece.anchored) {
            let target = if piece.pass == Pass::LinesOnTop {
                &mut out.on_top
            } else {
                &mut out.lines
            };
            target.push(WorldLine {
                start: to_world(Vec3::from_slice(&line.ends[..3])).to_array(),
                end: to_world(Vec3::from_slice(&line.ends[3..])).to_array(),
                color: line.color,
                width: line.width,
            });
        }
        let (target, layer) = match piece.pass {
            Pass::Opaque => (&mut out.entities, 0),
            Pass::Translucent => (&mut out.entities, TRANSLUCENT_OVERLAY),
            Pass::AfterTranslucent if piece.depth_write => (&mut out.after_translucent_depth, 0),
            Pass::AfterTranslucent => (&mut out.after_translucent, 0),
            Pass::Additive => (&mut out.additive, 0),
            Pass::SeeThrough => (&mut out.see_through, 0),
            Pass::Sky => (&mut out.sky, 0),
            Pass::Clouds => (&mut out.clouds, 0),
            Pass::Weather => (&mut out.weather, 0),
            Pass::Lines | Pass::LinesOnTop => continue,
        };
        let normal = if piece.no_cull {
            [0; 4]
        } else {
            crate::mesher::CULL_BACK
        };
        let start = target.len();
        for q in &mesh.quads {
            let key = q.texture.strip_prefix("minecraft:").unwrap_or(&q.texture);
            let Some(r) = frame.sprites.get(key) else {
                continue;
            };
            let at = to_world(Vec3::new(q.corners[0][0], q.corners[0][1], q.corners[0][2]));
            let block = [
                at.x.floor() as i32,
                at.y.floor() as i32,
                at.z.floor() as i32,
            ];
            let world = match last_light {
                _ if piece.anchored => 0xFF,
                Some((b, l)) if b == block => l,
                _ => {
                    let l = (frame.light)(block);
                    last_light = Some((block, l));
                    l
                }
            };
            for (k, c) in q.corners.iter().enumerate() {
                let light = light(world, q.light[k]);
                target.push(Vertex {
                    normal,
                    pos: place([c[0], c[1], c[2]]).to_array(),
                    uv: [r.u0 + (r.u1 - r.u0) * c[3], r.v0 + (r.v1 - r.v0) * c[4]],
                    layer,
                    color: argb_to_rgba(q.color[k], piece.modulator),
                    light,
                    glint_uv: [0.0, 0.0],
                });
            }
        }
        if matches!(piece.pass, Pass::Translucent | Pass::AfterTranslucent) {
            sort_far_to_near(&mut target[start..]);
        }
    }
}

/// Vanilla's `MeshData.sortQuads` with `VertexSorting.byDistance` from the camera: blended quads far to near.
fn sort_far_to_near(vertices: &mut [Vertex]) {
    let distance = |q: &[Vertex; 4]| {
        (q.iter()
            .fold(Vec3::ZERO, |sum, v| sum + Vec3::from_array(v.pos))
            / 4.0)
            .length_squared()
    };
    vertices
        .as_chunks_mut::<4>()
        .0
        .sort_by(|a, b| distance(b).total_cmp(&distance(a)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blended_quads_are_drawn_far_to_near_from_the_camera() {
        let quad = |z: f32, color: u32| {
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]].map(|[x, y]| Vertex {
                normal: [0; 4],
                pos: [x, y, z],
                uv: [0.0, 0.0],
                layer: 0,
                color,
                light: 0,
                glint_uv: [0.0, 0.0],
            })
        };
        let mut vertices: Vec<Vertex> = [quad(-2.0, 1), quad(-5.0, 2), quad(3.0, 3)].concat();
        sort_far_to_near(&mut vertices);
        let order: Vec<u32> = vertices
            .as_chunks::<4>()
            .0
            .iter()
            .map(|q| q[0].color)
            .collect();
        assert_eq!(order, vec![2, 3, 1]);
        assert!(
            vertices
                .as_chunks::<4>()
                .0
                .iter()
                .all(|q| q.iter().all(|v| v.color == q[0].color))
        );
    }

    /// The registry is process-wide: tests that fill it take turns.
    static TESTS: Mutex<()> = Mutex::new(());

    struct Out(Vec<u8>);
    impl Out {
        fn int(&mut self, v: i32) {
            self.0.extend(v.to_be_bytes());
        }
        fn long(&mut self, v: i64) {
            self.0.extend(v.to_be_bytes());
        }
        fn float(&mut self, v: f32) {
            self.0.extend(v.to_be_bytes());
        }
        fn double(&mut self, v: f64) {
            self.0.extend(v.to_be_bytes());
        }
        fn text(&mut self, v: &str) {
            self.int(v.len() as i32);
            self.0.extend(v.as_bytes());
        }
    }

    fn quad(out: &mut Out, x: f32) {
        out.text("horizon/white");
        for k in 0..4 {
            for v in [x + (k % 2) as f32, (k / 2) as f32, 0.0, 0.5, 0.5] {
                out.float(v);
            }
        }
        for _ in 0..4 {
            out.int(0x80FF_0000u32 as i32);
        }
        for _ in 0..4 {
            out.int(0);
        }
        out.0.push(0);
    }

    fn piece(out: &mut Out, mesh: i64, pass: u8, flags: u8, origin: [f64; 3]) {
        out.long(mesh);
        out.0.push(pass);
        out.0.push(flags);
        origin.iter().for_each(|v| out.double(*v));
        for v in [
            1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ] {
            out.float(v);
        }
    }

    /// A recording of meshes `ids` with `quads` quads each (and one line), and one piece per entry of `pieces`.
    fn blob(
        generation: i32,
        dropped: &[i64],
        fresh: &[(i64, usize)],
        pieces: &[(i64, u8, [f64; 3])],
    ) -> Vec<u8> {
        let mut out = Out(Vec::new());
        out.int(generation);
        out.int(dropped.len() as i32);
        dropped.iter().for_each(|d| out.long(*d));
        out.int(fresh.len() as i32);
        for (id, quads) in fresh {
            out.long(*id);
            out.int(*quads as i32);
            (0..*quads).for_each(|q| quad(&mut out, q as f32 * 2.0));
            out.int(1);
            for v in [0.0, 0.0, 0.0, 1.0, 1.0, 1.0] {
                out.float(v);
            }
            out.int(0xFFFF_0000u32 as i32);
            out.float(2.0);
        }
        out.int(pieces.len() as i32);
        for (mesh, pass, origin) in pieces {
            piece(&mut out, *mesh, *pass, 0, *origin);
        }
        out.0
    }

    fn frame<'a>(sprites: &'a HashMap<String, Rect>, camera: DVec3) -> Frame<'a> {
        Frame {
            camera,
            sprites,
            light: &|_| 0x5_3,
            visible: &|_| true,
        }
    }

    fn sprites() -> HashMap<String, Rect> {
        HashMap::from([(
            "horizon/white".to_owned(),
            Rect {
                u0: 0.0,
                v0: 0.0,
                u1: 0.5,
                v1: 0.5,
                w: 4,
                h: 4,
            },
        )])
    }

    #[test]
    fn a_new_mesh_a_reference_and_a_dropped_mesh() {
        let _t = TESTS.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        assert!(apply(&blob(
            1,
            &[],
            &[(7, 2)],
            &[(7, 1, [10.0, 20.0, 30.0])]
        )));
        assert_eq!(counts(), (1, 1));
        // A later recording refers to the mesh without sending it.
        assert!(apply(&blob(
            2,
            &[],
            &[],
            &[(7, 1, [1.0, 2.0, 3.0]), (7, 0, [4.0, 5.0, 6.0])]
        )));
        assert_eq!(counts(), (2, 1));
        // A stale generation changes nothing.
        assert!(!apply(&blob(2, &[7], &[], &[])));
        assert_eq!(counts(), (2, 1));
        // A dropped mesh takes its pieces with it.
        assert!(apply(&blob(3, &[7], &[], &[(7, 1, [0.0; 3])])));
        assert_eq!(counts(), (0, 0));
        // Half a blob changes nothing.
        let whole = blob(4, &[], &[(8, 1)], &[(8, 0, [0.0; 3])]);
        assert!(!apply(&whole[..whole.len() - 3]));
        assert_eq!(counts(), (0, 0));
        clear();
    }

    #[test]
    fn the_pass_byte_routes_a_piece() {
        let _t = TESTS.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        let pieces: Vec<_> = (0..7).map(|p| (5, p as u8, [0.0; 3])).collect();
        assert!(apply(&blob(1, &[], &[(5, 1)], &pieces)));
        let sprites = sprites();
        let mut out = Outputs::default();
        draw(&frame(&sprites, DVec3::ZERO), &mut out);
        let layers: Vec<u32> = out.entities.iter().step_by(4).map(|v| v.layer).collect();
        assert_eq!(layers, [0, TRANSLUCENT_OVERLAY]);
        assert_eq!(out.after_translucent.len(), 4);
        assert!(out.after_translucent_depth.is_empty());
        assert_eq!(out.additive.len(), 4);
        assert_eq!(out.see_through.len(), 4);
        // Every piece's mesh has a line; the line passes take theirs.
        assert_eq!(out.lines.len() + out.on_top.len(), 7);
        assert_eq!(out.on_top.len(), 1);
        clear();
    }

    #[test]
    fn anchored_pieces_stay_on_the_camera_and_the_environment_bits_say_what_native_skips() {
        let _t = TESTS.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        let mut bytes = Out(Vec::new());
        bytes.int(1);
        bytes.int(0);
        bytes.int(1);
        bytes.long(5);
        bytes.int(1);
        quad(&mut bytes, 0.0);
        bytes.int(0);
        bytes.int(3);
        piece(&mut bytes, 5, 7, 4, [0.0, 100.0, 0.0]);
        piece(&mut bytes, 5, 8, 4, [0.0, 100.0, 0.0]);
        piece(&mut bytes, 5, 9, 4, [0.0, 100.0, 0.0]);
        assert!(apply(&bytes.0));
        set_environment(5);
        let environment = environment();
        assert!(environment.sky && !environment.clouds && environment.weather);
        assert!(!environment.rain_tick);
        let sprites = sprites();
        let mut out = Outputs::default();
        // A camera far away and a view that sees nothing of the world: anchored pieces are still drawn.
        let far = Frame {
            visible: &|_| false,
            ..frame(&sprites, DVec3::new(5_000.0, 10.0, -7_000.0))
        };
        draw(&far, &mut out);
        for quads in [&out.sky, &out.clouds, &out.weather] {
            assert_eq!(quads.len(), 4);
            assert_eq!(quads[0].pos, [0.0, 100.0, 0.0]);
        }
        assert!(out.lines.is_empty());
        clear();
        assert_eq!(super::environment(), Environment::default());
    }

    #[test]
    fn flags_pick_the_normals_and_the_depth_writing_layer() {
        let _t = TESTS.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        let mut bytes = Out(Vec::new());
        bytes.int(1);
        bytes.int(0);
        bytes.int(1);
        bytes.long(5);
        bytes.int(1);
        quad(&mut bytes, 0.0);
        bytes.int(0);
        let cases = [(2u8, 0u8), (2, 1), (2, 2), (2, 3), (1, 2), (0, 2)];
        bytes.int(cases.len() as i32);
        for (pass, flags) in cases {
            piece(&mut bytes, 5, pass, flags, [0.0; 3]);
        }
        assert!(apply(&bytes.0));
        let sprites = sprites();
        let mut out = Outputs::default();
        draw(&frame(&sprites, DVec3::ZERO), &mut out);
        let normals = |v: &[Vertex]| v.iter().step_by(4).map(|v| v.normal).collect::<Vec<_>>();
        let layers = |v: &[Vertex]| v.iter().step_by(4).map(|v| v.layer).collect::<Vec<_>>();
        let culled = crate::mesher::CULL_BACK;
        assert_eq!(normals(&out.after_translucent), [culled, [0; 4]]);
        assert_eq!(normals(&out.after_translucent_depth), [culled, [0; 4]]);
        assert_eq!(layers(&out.entities), [TRANSLUCENT_OVERLAY, 0]);
        assert_eq!(normals(&out.entities), [culled, culled]);
        clear();
    }

    #[test]
    fn pieces_sit_at_the_world_minus_the_camera() {
        let _t = TESTS.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        assert!(apply(&blob(
            1,
            &[],
            &[(1, 1)],
            &[(1, 0, [1_000_000.0, 64.0, -2.0])]
        )));
        let sprites = sprites();
        let mut out = Outputs::default();
        draw(
            &frame(&sprites, DVec3::new(1_000_000.5, 60.0, -2.0)),
            &mut out,
        );
        assert_eq!(out.entities[0].pos, [-0.5, 4.0, 0.0]);
        assert_eq!(out.lines[0].start, [1_000_000.0, 64.0, -2.0]);
        // The camera moving leaves the world position alone.
        out.clear();
        draw(
            &frame(&sprites, DVec3::new(1_000_003.5, 60.0, -2.0)),
            &mut out,
        );
        assert_eq!(out.entities[0].pos, [-3.5, 4.0, 0.0]);
        assert_eq!(out.lines[0].start, [1_000_000.0, 64.0, -2.0]);
        // The world's light fills a quad that brought none.
        assert_eq!(out.entities[0].light, (5 * 16) | ((3 * 16) << 8));
        clear();
    }

    #[test]
    fn a_recording_over_the_cap_drops_its_last_pieces() {
        let _t = TESTS.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        let half = MAX_QUADS / 2 + 1;
        let pieces = [(1, 0, [0.0; 3]), (1, 0, [5.0; 3]), (2, 0, [9.0; 3])];
        assert!(apply(&blob(1, &[], &[(1, half), (2, 1)], &pieces)));
        // The second piece would pass the cap; the small one after it fits.
        assert_eq!(counts().0, 2);
        clear();
    }

    #[test]
    #[ignore = "manual cost measurement"]
    fn twenty_thousand_quads_cost_per_frame() {
        let _t = TESTS.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        assert!(apply(&blob(1, &[], &[(1, 20_000)], &[(1, 1, [0.0; 3])])));
        let sprites = sprites();
        let mut out = Outputs::default();
        let started = std::time::Instant::now();
        for _ in 0..20 {
            out.clear();
            draw(&frame(&sprites, DVec3::ZERO), &mut out);
        }
        println!("20000 quads: {:?} per frame", started.elapsed() / 20);
        clear();
    }
}
