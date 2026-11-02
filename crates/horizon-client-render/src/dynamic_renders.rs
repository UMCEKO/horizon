//! What mods' block entity renderers draw, recorded once per change with how each piece moves over time, drawn here every frame.
use crate::{block_entity_render, hud::Rect, mesher::Vertex};
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
fn atlas_key(texture: &str) -> &str {
    texture.strip_prefix("minecraft:").unwrap_or(texture)
}

/// Horizon's packed light (`sky*16 | block*16 << 8`) for a vertex: the brighter of the world's and the renderer's.
fn light(world: u8, coords: u32) -> (u32, bool) {
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

/// The recorded block entities in view, with their camera-relative origin and the ticks since each was recorded.
fn in_view<'a>(
    registry: &'a HashMap<[i32; 3], Entry>,
    frame: &'a Frame,
) -> impl Iterator<Item = ([i32; 3], &'a Entry, Mat4, f32)> + 'a {
    registry.iter().filter_map(move |(pos, entry)| {
        let corner = DVec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]));
        if (corner + DVec3::splat(0.5)).distance_squared(frame.camera) > 64.0 * 64.0
            || !(frame.visible)(*pos)
        {
            return None;
        }
        let ticks = (frame.game_time - entry.since) as f32 + frame.partial;
        Some((
            *pos,
            entry,
            Mat4::from_translation((corner - frame.camera).as_vec3()),
            ticks,
        ))
    })
}

/// The items recorded renderers show, with their camera-relative pose and block position.
pub fn items(frame: &Frame) -> Vec<(Mat4, Item, [i32; 3])> {
    let registry = REGISTRY.read().unwrap();
    let mut out = Vec::new();
    for (pos, entry, origin, ticks) in in_view(&registry, frame) {
        for d in entry.draws.iter() {
            if let Some(item) = &d.item {
                out.push((origin * d.matrix(ticks), item.clone(), pos));
            }
        }
    }
    out
}

/// Every recorded block entity within the renderers' default view distance.
pub fn draw(frame: &Frame, out: &mut Vec<Vertex>) {
    let registry = REGISTRY.read().unwrap();
    if registry.is_empty() {
        return;
    }
    let lights = block_entity_render::diffuse_lights(frame.nether);
    for (pos, entry, origin, ticks) in in_view(&registry, frame) {
        let world = (frame.light)(pos);
        for d in entry.draws.iter() {
            let m = origin * d.matrix(ticks);
            for q in &d.quads {
                let Some(r) = frame.sprites.get(atlas_key(&q.texture)) else {
                    continue;
                };
                let pos = q
                    .corners
                    .map(|c| m.transform_point3(Vec3::new(c[0], c[1], c[2])));
                let normal = (pos[1] - pos[0]).cross(pos[2] - pos[0]).normalize_or(
                    (pos[2] - pos[1])
                        .cross(pos[3] - pos[1])
                        .normalize_or(Vec3::Y),
                );
                let shade = block_entity_render::diffuse(normal, lights);
                for k in 0..4 {
                    let (light, emissive) = light(world, q.light[k]);
                    let brightness = if emissive { 1.0 } else { shade };
                    let argb = q.color[k];
                    let channel = |shift: u32| (((argb >> shift) & 255) as f32 * brightness) as u32;
                    out.push(Vertex {
                        normal: [0; 4],
                        pos: pos[k].to_array(),
                        uv: [
                            r.u0 + (r.u1 - r.u0) * q.corners[k][3],
                            r.v0 + (r.v1 - r.v0) * q.corners[k][4],
                        ],
                        layer: 0,
                        color: (argb >> 24) << 24
                            | channel(0) << 16
                            | channel(8) << 8
                            | channel(16),
                        light,
                        glint_uv: [0.0, 0.0],
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_renderers_full_bright_light_wins_over_the_world() {
        assert_eq!(light(0x5_3, 0), ((5 * 16) | ((3 * 16) << 8), false));
        assert_eq!(
            light(0x5_3, 0xF0_00F0),
            ((15 * 16) | ((15 * 16) << 8), true)
        );
    }
}
