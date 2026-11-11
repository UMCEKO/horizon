//! What mods' block entity renderers draw, recorded once per change with how each piece moves over time; the game replays it each frame.
use crate::hud::Rect;
use glam::{DVec3, Mat4, Quat, Vec3};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, RwLock},
};

/// A value over ticks since the recording.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Param {
    Const(f32),
    Linear(f32, f32),
    /// `a sin(w t) + b cos(w t) + c`.
    Sine(f32, f32, f32, f32),
}
impl Param {
    pub fn at(&self, t: f32) -> f32 {
        match *self {
            Param::Const(c) => c,
            Param::Linear(c, slope) => c + slope * t,
            Param::Sine(a, b, c, w) => a * (w * t).sin() + b * (w * t).cos() + c,
        }
    }
}

/// One `PoseStack` operation, applied in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Translate([Param; 3]),
    Scale([Param; 3]),
    Rotate(Vec3, Param),
    Quat(Quat),
    Matrix(Mat4),
}

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

/// An item the renderer shows, which the game draws with its own item models.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub stack: horizon_items::items::ItemStack,
    /// `ItemDisplayContext` ordinal.
    pub context: u8,
    pub seed: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Draw {
    pub translucent: bool,
    pub ops: Vec<Op>,
    pub quads: Vec<Quad>,
    pub item: Option<Item>,
}
impl Draw {
    /// The piece's matrix `ticks` after it was recorded.
    pub fn matrix(&self, ticks: f32) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        for op in &self.ops {
            m *= match op {
                Op::Translate(p) => Mat4::from_translation(Vec3::new(
                    p[0].at(ticks),
                    p[1].at(ticks),
                    p[2].at(ticks),
                )),
                Op::Scale(p) => {
                    Mat4::from_scale(Vec3::new(p[0].at(ticks), p[1].at(ticks), p[2].at(ticks)))
                }
                Op::Rotate(axis, angle) => Mat4::from_axis_angle(*axis, angle.at(ticks)),
                Op::Quat(q) => Mat4::from_quat(*q),
                Op::Matrix(m) => *m,
            };
        }
        m
    }
}

struct Entry {
    draws: Arc<[Draw]>,
    /// Game time the recording arrived.
    since: i64,
}

static REGISTRY: LazyLock<RwLock<HashMap<[i32; 3], Entry>>> = LazyLock::new(RwLock::default);

/// Whether two values move alike, wherever they are in their motion.
fn same_motion(a: &Param, b: &Param) -> bool {
    let close = |x: f32, y: f32| (x - y).abs() <= 1e-4 * (1.0 + x.abs());
    match (*a, *b) {
        (Param::Linear(_, p), Param::Linear(_, q)) => close(p, q),
        (Param::Sine(a1, b1, c1, w1), Param::Sine(a2, b2, c2, w2)) => {
            close(w1, w2) && close(c1, c2) && close(a1.hypot(b1), a2.hypot(b2))
        }
        _ => a == b,
    }
}
fn same_draw(a: &Draw, b: &Draw) -> bool {
    a.translucent == b.translucent
        && a.quads == b.quads
        && a.item == b.item
        && a.ops.len() == b.ops.len()
        && a.ops.iter().zip(&b.ops).all(|pair| match pair {
            (Op::Translate(p), Op::Translate(q)) | (Op::Scale(p), Op::Scale(q)) => {
                p.iter().zip(q).all(|(x, y)| same_motion(x, y))
            }
            (Op::Rotate(u, p), Op::Rotate(v, q)) => u.abs_diff_eq(*v, 1e-4) && same_motion(p, q),
            (x, y) => x == y,
        })
}

/// Stores a recording; one that only moves as the drawn one does keeps its place in the motion.
pub fn set(pos: [i32; 3], draws: Vec<Draw>, now: i64) {
    if REGISTRY.read().unwrap().get(&pos).is_some_and(|old| {
        old.draws.len() == draws.len() && old.draws.iter().zip(&draws).all(|(a, b)| same_draw(a, b))
    }) {
        return;
    }
    REGISTRY.write().unwrap().insert(
        pos,
        Entry {
            draws: draws.into(),
            since: now,
        },
    );
}
/// Whether a recording of block `old` no longer holds once the position becomes `new`: another block, or one with no block entity.
pub fn stale(old: &str, new: &str, new_has_block_entity: bool) -> bool {
    old != new || !new_has_block_entity
}
pub fn remove(pos: [i32; 3]) {
    if REGISTRY.read().unwrap().contains_key(&pos) {
        REGISTRY.write().unwrap().remove(&pos);
    }
}
pub fn clear() {
    REGISTRY.write().unwrap().clear();
}
pub fn is_empty() -> bool {
    REGISTRY.read().unwrap().is_empty()
}

/// The atlas name of a texture: vanilla's without its namespace.
pub fn atlas_key(texture: &str) -> &str {
    texture.strip_prefix("minecraft:").unwrap_or(texture)
}

/// Horizon's packed light (`sky*16 | block*16 << 8`) for a vertex, the brighter of the world's and the renderer's, and whether the renderer drew it full bright.
pub fn light(world: u8, coords: u32) -> (u32, bool) {
    let (world_sky, world_block) = (u32::from(world >> 4), u32::from(world & 15));
    let (block, sky) = ((coords & 0xFFFF) >> 4, (coords >> 16) >> 4);
    let (sky, block) = (world_sky.max(sky.min(15)), world_block.max(block.min(15)));
    (
        (sky * 16) | ((block * 16) << 8),
        coords != 0 && (coords & 0xFFFF) >> 4 >= 15,
    )
}

pub struct Frame<'a> {
    pub camera: DVec3,
    pub game_time: i64,
    pub partial: f32,
    pub sprites: &'a HashMap<String, Rect>,
    pub nether: bool,
    /// The world's packed light (`sky << 4 | block`) at a position.
    pub light: &'a dyn Fn([i32; 3]) -> u8,
    /// Whether a block's box may be on screen.
    pub visible: &'a dyn Fn([i32; 3]) -> bool,
}

/// `BlockEntityRenderer.getViewDistance`'s default, in blocks.
const VIEW_DISTANCE: f64 = 64.0;

/// A recorded block entity to draw this frame.
pub struct InView {
    pub pos: [i32; 3],
    pub draws: Arc<[Draw]>,
    /// Ticks since the recording, partial tick included: what [`Draw::matrix`] takes.
    pub ticks: f32,
}

/// The recorded block entities closer than the default view distance (`Vec3.closerThan`) and on screen, copied out so the registry is not locked while they are drawn.
pub fn in_view(frame: &Frame) -> Vec<InView> {
    let registry = REGISTRY.read().unwrap();
    registry
        .iter()
        .filter(|(pos, _)| {
            let corner = DVec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]));
            (corner + 0.5).distance_squared(frame.camera) < VIEW_DISTANCE * VIEW_DISTANCE
                && (frame.visible)(**pos)
        })
        .map(|(pos, entry)| InView {
            pos: *pos,
            draws: Arc::clone(&entry.draws),
            ticks: (frame.game_time - entry.since) as f32 + frame.partial,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The registry is process-wide: tests that fill it take turns.
    static REGISTRY_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn params_and_operations_rebuild_the_motion() {
        assert_eq!(Param::Const(2.0).at(9.0), 2.0);
        assert_eq!(Param::Linear(1.0, 0.5).at(4.0), 3.0);
        assert!(
            (Param::Sine(1.0, 0.0, 0.5, std::f32::consts::FRAC_PI_2).at(1.0) - 1.5).abs() < 1e-6
        );
        let spin = Draw {
            translucent: false,
            ops: vec![
                Op::Translate([Param::Const(0.5); 3]),
                Op::Rotate(Vec3::Y, Param::Linear(0.0, std::f32::consts::FRAC_PI_2)),
                Op::Translate([Param::Const(-0.5); 3]),
            ],
            quads: Vec::new(),
            item: None,
        };
        let p = spin.matrix(1.0).transform_point3(Vec3::new(1.0, 0.5, 0.5));
        assert!((p - Vec3::new(0.5, 0.5, 0.0)).length() < 1e-5, "{p}");
        assert_eq!(spin.matrix(0.0).transform_point3(Vec3::X), Vec3::X);
    }

    #[test]
    fn a_recording_moving_alike_keeps_the_running_one() {
        let _registry = REGISTRY_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        let spin = |phase: f32| Draw {
            translucent: false,
            ops: vec![Op::Rotate(Vec3::Y, Param::Linear(phase, 0.1))],
            quads: Vec::new(),
            item: None,
        };
        set([1, 2, 3], vec![spin(0.0)], 10);
        set([1, 2, 3], vec![spin(2.0)], 50);
        assert_eq!(REGISTRY.read().unwrap()[&[1, 2, 3]].since, 10);
        set(
            [1, 2, 3],
            vec![Draw {
                ops: vec![Op::Rotate(Vec3::Y, Param::Linear(0.0, 0.3))],
                ..spin(0.0)
            }],
            60,
        );
        assert_eq!(REGISTRY.read().unwrap()[&[1, 2, 3]].since, 60);
        clear();
    }

    #[test]
    fn a_recording_from_another_camera_replaces_the_positions_quads() {
        let _registry = REGISTRY_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        let face = |texture: &str| Quad {
            texture: texture.to_owned(),
            corners: [[0.0; 5]; 4],
            color: [0; 4],
            light: [0; 4],
        };
        let draw = |quads: Vec<Quad>| Draw {
            translucent: false,
            ops: Vec::new(),
            quads,
            item: None,
        };
        set([4, 5, 6], vec![draw(vec![face("a:top")])], 1);
        set(
            [4, 5, 6],
            vec![draw(vec![face("a:top"), face("a:side")])],
            2,
        );
        let registry = REGISTRY.read().unwrap();
        assert_eq!(registry[&[4, 5, 6]].draws[0].quads.len(), 2);
        assert_eq!(registry[&[4, 5, 6]].since, 2);
        drop(registry);
        clear();
    }

    #[test]
    fn a_block_change_to_air_drops_the_recorded_output() {
        let _registry = REGISTRY_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        let draw = Draw {
            translucent: false,
            ops: Vec::new(),
            quads: Vec::new(),
            item: None,
        };
        set([7, 8, 9], vec![draw], 1);
        assert!(stale("a:tank", "minecraft:air", false));
        assert!(!stale("a:tank", "a:tank", true));
        assert!(stale("a:tank", "a:tank", false));
        if stale("a:tank", "minecraft:air", false) {
            remove([7, 8, 9]);
        }
        assert!(!REGISTRY.read().unwrap().contains_key(&[7, 8, 9]));
    }

    #[test]
    fn a_renderers_full_bright_light_wins_over_the_world() {
        assert_eq!(light(0x5_3, 0), ((5 * 16) | ((3 * 16) << 8), false));
        assert_eq!(
            light(0x5_3, 0xF0_00F0),
            ((15 * 16) | ((15 * 16) << 8), true)
        );
    }

    #[test]
    fn only_recordings_near_and_on_screen_are_drawn_at_their_own_time() {
        let _registry = REGISTRY_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        let still = || Draw {
            translucent: false,
            ops: Vec::new(),
            quads: Vec::new(),
            item: None,
        };
        set([0, 0, 0], vec![still()], 100);
        set([0, 0, 64], vec![still()], 100);
        set([5, 0, 0], vec![still()], 110);
        set([0, 0, 63], vec![still()], 90);
        let sprites = HashMap::new();
        let frame = Frame {
            camera: DVec3::new(0.5, 0.5, 0.5),
            game_time: 120,
            partial: 0.25,
            sprites: &sprites,
            nether: false,
            light: &|_| 0,
            visible: &|p| p[0] != 5,
        };
        let mut drawn: Vec<_> = in_view(&frame).iter().map(|v| (v.pos, v.ticks)).collect();
        drawn.sort_by_key(|(pos, _)| *pos);
        assert_eq!(drawn, vec![([0, 0, 0], 20.25), ([0, 0, 63], 30.25)]);
        clear();
    }
}
