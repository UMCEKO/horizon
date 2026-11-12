//! The Java client's quad particles of its last tick, as its recorder wrote them (SPEC-3).

use crate::{hud::Rect, mesher::Vertex};
use glam::{DVec3, Quat, Vec3};
use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
};

/// The Java client's quad particles of its last tick: two samples each, drawn interpolated.
#[derive(Clone, Debug, Default)]
pub struct JavaParticles {
    pub(super) anchor: DVec3,
    pub(super) names: Vec<String>,
    pub(super) quads: Vec<JavaQuad>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Sample {
    pub(super) pos: Vec3,
    pub(super) rot: Quat,
    pub(super) size: f32,
    /// `[u0, u1, v0, v1]` within the sprite.
    pub(super) uv: [f32; 4],
    pub(super) color: u32,
    pub(super) light: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct JavaQuad {
    pub(super) translucent: bool,
    pub(super) facing: Facing,
    pub(super) span: Span,
    pub(super) sprite: u32,
    pub(super) a: Sample,
    pub(super) b: Sample,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Facing {
    Camera,
    CameraYaw,
    Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Span {
    Whole,
    FirstHalf,
    SecondHalf,
}

/// Big-endian fields as Java's `DataOutputStream` wrote them.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_first_chunk()?;
        self.0 = rest;
        Some(*head)
    }
    fn byte(&mut self) -> Option<u8> {
        self.take::<1>().map(|b| b[0])
    }
    fn int(&mut self) -> Option<i32> {
        self.take().map(i32::from_be_bytes)
    }
    fn float(&mut self) -> Option<f32> {
        self.take().map(f32::from_be_bytes)
    }
    fn double(&mut self) -> Option<f64> {
        self.take().map(f64::from_be_bytes)
    }
    fn text(&mut self) -> Option<&'a str> {
        let len = usize::try_from(self.int()?).ok()?;
        let (head, rest) = self.0.split_at_checked(len)?;
        self.0 = rest;
        std::str::from_utf8(head).ok()
    }
    fn sample(&mut self) -> Option<Sample> {
        let pos = Vec3::new(self.float()?, self.float()?, self.float()?);
        let rot = Quat::from_xyzw(self.float()?, self.float()?, self.float()?, self.float()?);
        let size = self.float()?;
        let uv = [self.float()?, self.float()?, self.float()?, self.float()?];
        let color = self.int()? as u32;
        let light = self.int()? as u32;
        Some(Sample {
            pos,
            rot,
            size,
            uv,
            color,
            light,
        })
    }
}

impl JavaParticles {
    /// None when the blob doesn't read to its end.
    pub fn parse(blob: &[u8]) -> Option<Self> {
        let mut r = Reader(blob);
        let anchor = DVec3::new(r.double()?, r.double()?, r.double()?);
        let names = (0..usize::try_from(r.int()?).ok()?)
            .map(|_| r.text().map(str::to_owned))
            .collect::<Option<Vec<_>>>()?;
        let count = usize::try_from(r.int()?).ok()?;
        let mut quads = Vec::with_capacity(count.min(r.0.len() / 100));
        for _ in 0..count {
            let flags = r.byte()?;
            let facing = match flags >> 1 & 3 {
                0 => Facing::Camera,
                1 => Facing::CameraYaw,
                2 => Facing::Fixed,
                _ => return None,
            };
            let span = match flags >> 3 & 3 {
                0 => Span::Whole,
                1 => Span::FirstHalf,
                2 => Span::SecondHalf,
                _ => return None,
            };
            let sprite = u32::try_from(r.int()?).ok()?;
            if sprite as usize >= names.len() {
                return None;
            }
            quads.push(JavaQuad {
                translucent: flags & 1 != 0,
                facing,
                span,
                sprite,
                a: r.sample()?,
                b: r.sample()?,
            });
        }
        r.0.is_empty().then_some(Self {
            anchor,
            names,
            quads,
        })
    }

    pub fn len(&self) -> usize {
        self.quads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }

    /// Appends the quads at `partial`, seen from `camera` turned by `base` (the frame's `Camera.rotation()`).
    pub(super) fn draw(
        &self,
        camera: DVec3,
        base: Quat,
        partial: f32,
        atlas: &HashMap<String, (Rect, bool)>,
        keep: &mut dyn FnMut(Vec3, f32) -> bool,
        out: &mut Vec<Vertex>,
    ) {
        let t = partial;
        let yaw = Quat::from_xyzw(0.0, base.y, 0.0, base.w).normalize();
        for q in &self.quads {
            match q.span {
                Span::FirstHalf if t >= 0.5 => continue,
                Span::SecondHalf if t < 0.5 => continue,
                _ => {}
            }
            let s = if t < 0.5 { &q.a } else { &q.b };
            let name = &self.names[q.sprite as usize];
            let key = name.strip_prefix("minecraft:").unwrap_or(name);
            let Some(&(rect, _)) = atlas.get(key) else {
                warn_missing(name);
                continue;
            };
            let center = (self.anchor + q.a.pos.lerp(q.b.pos, t).as_dvec3() - camera).as_vec3();
            let size = q.a.size + (q.b.size - q.a.size) * t;
            if !keep(center, size) {
                continue;
            }
            let roll = q.a.rot.normalize().slerp(q.b.rot.normalize(), t);
            let rotation = match q.facing {
                Facing::Camera => base * roll,
                Facing::CameraYaw => yaw * roll,
                Facing::Fixed => roll,
            };
            let color = lerp_color(q.a.color, q.b.color, t);
            let block = ((s.light & 0xFFFF) >> 4).min(15);
            let sky = ((s.light >> 16) >> 4).min(15);
            let light = (sky * 16) | ((block * 16) << 8);
            let (du, dv) = (rect.u1 - rect.u0, rect.v1 - rect.v0);
            let [u0, u1, v0, v1] = [
                rect.u0 + du * s.uv[0],
                rect.u0 + du * s.uv[1],
                rect.v0 + dv * s.uv[2],
                rect.v0 + dv * s.uv[3],
            ];
            let layer = if q.translucent {
                crate::gfx::TRANSLUCENT_PARTICLE_OVERLAY
            } else {
                0
            };
            for (n, uv) in [
                ((1.0, -1.0), [u1, v1]),
                ((1.0, 1.0), [u1, v0]),
                ((-1.0, 1.0), [u0, v0]),
                ((-1.0, -1.0), [u0, v1]),
            ] {
                out.push(Vertex {
                    normal: [0; 4],
                    pos: (center + rotation * Vec3::new(n.0, n.1, 0.0) * size).to_array(),
                    uv,
                    layer,
                    color,
                    light,
                    glint_uv: [0.0, 0.0],
                });
            }
        }
    }
}

/// Java's ARGB ints lerped per channel, packed as native does (`a << 24 | b << 16 | g << 8 | r`).
fn lerp_color(a: u32, b: u32, t: f32) -> u32 {
    let ch = |shift: u32| {
        let (a, b) = ((a >> shift & 255) as f32, (b >> shift & 255) as f32);
        (a + (b - a) * t).round().clamp(0.0, 255.0) as u32
    };
    ch(24) << 24 | ch(0) << 16 | ch(8) << 8 | ch(16)
}

fn warn_missing(name: &str) {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let mut seen = SEEN
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if seen.insert(name.to_owned()) {
        log::warn!("Horizon: a Java particle's sprite {name} isn't in the atlas");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(out: &mut Vec<u8>, base: f32, color: i32, light: i32) {
        for i in 0..12 {
            out.extend((base + i as f32).to_be_bytes());
        }
        out.extend(color.to_be_bytes());
        out.extend(light.to_be_bytes());
    }

    fn blob(names: &[&str], quads: &[(u8, i32)]) -> Vec<u8> {
        let mut out = Vec::new();
        for v in [1.5f64, 2.5, 3.5] {
            out.extend(v.to_be_bytes());
        }
        out.extend((names.len() as i32).to_be_bytes());
        for name in names {
            out.extend((name.len() as i32).to_be_bytes());
            out.extend(name.as_bytes());
        }
        out.extend((quads.len() as i32).to_be_bytes());
        for &(flags, sprite) in quads {
            out.push(flags);
            out.extend(sprite.to_be_bytes());
            sample(&mut out, 1.0, -1, 0x00f0_00f0);
            sample(&mut out, 100.0, 0x80ff_0000_u32 as i32, 0x0050_0000);
        }
        out
    }

    #[test]
    fn two_quads_read_back_field_by_field() {
        let bytes = blob(
            &["minecraft:particle/flame", "minecraft:particle/smoke"],
            &[(0b0000_0001, 0), (0b0001_0100, 1)],
        );
        let parsed = JavaParticles::parse(&bytes).unwrap();
        assert_eq!(parsed.anchor, DVec3::new(1.5, 2.5, 3.5));
        assert_eq!(
            parsed.names,
            ["minecraft:particle/flame", "minecraft:particle/smoke"]
        );
        assert_eq!(parsed.len(), 2);
        let first = parsed.quads[0];
        assert!(first.translucent);
        assert_eq!((first.facing, first.span), (Facing::Camera, Span::Whole));
        assert_eq!(first.sprite, 0);
        assert_eq!(first.a.pos, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(first.a.rot, Quat::from_xyzw(4.0, 5.0, 6.0, 7.0));
        assert_eq!(first.a.size, 8.0);
        assert_eq!(first.a.uv, [9.0, 10.0, 11.0, 12.0]);
        assert_eq!(first.a.color, u32::MAX);
        assert_eq!(first.a.light, 0x00f0_00f0);
        assert_eq!(first.b.pos, Vec3::new(100.0, 101.0, 102.0));
        assert_eq!(first.b.color, 0x80ff_0000);
        assert_eq!(first.b.light, 0x0050_0000);
        let second = parsed.quads[1];
        assert!(!second.translucent);
        assert_eq!(
            (second.facing, second.span),
            (Facing::Fixed, Span::SecondHalf)
        );
        assert_eq!(second.sprite, 1);
    }

    #[test]
    fn a_blob_cut_short_or_with_a_bad_sprite_is_unreadable() {
        let bytes = blob(&["a", "b"], &[(0, 0), (0, 1)]);
        assert!(JavaParticles::parse(&bytes[..bytes.len() - 1]).is_none());
        assert!(JavaParticles::parse(&blob(&["a", "b"], &[(0, 2)])).is_none());
        assert!(JavaParticles::parse(&blob(&["a"], &[(0b0000_0110, 0)])).is_none());
        assert!(JavaParticles::parse(&blob(&["a"], &[(0b0001_1000, 0)])).is_none());
    }

    #[test]
    fn an_empty_blob_has_no_quads() {
        let parsed = JavaParticles::parse(&blob(&[], &[])).unwrap();
        assert_eq!(parsed.len(), 0);
        assert!(parsed.is_empty());
    }

    fn at(pos: Vec3, rot: Quat, size: f32, color: u32, light: u32) -> Sample {
        Sample {
            pos,
            rot,
            size,
            uv: [0.0, 1.0, 0.0, 1.0],
            color,
            light,
        }
    }

    fn one(facing: Facing, span: Span, a: Sample, b: Sample) -> JavaParticles {
        JavaParticles {
            anchor: DVec3::ZERO,
            names: vec!["minecraft:flame".into()],
            quads: vec![JavaQuad {
                translucent: false,
                facing,
                span,
                sprite: 0,
                a,
                b,
            }],
        }
    }

    fn atlas() -> HashMap<String, (Rect, bool)> {
        let rect = Rect {
            u0: 0.0,
            v0: 0.0,
            u1: 1.0,
            v1: 1.0,
            w: 16,
            h: 16,
        };
        HashMap::from([("flame".to_owned(), (rect, false))])
    }

    fn drawn(p: &JavaParticles, base: Quat, partial: f32) -> Vec<Vertex> {
        let mut out = Vec::new();
        p.draw(
            DVec3::ZERO,
            base,
            partial,
            &atlas(),
            &mut |_, _| true,
            &mut out,
        );
        out
    }

    fn base_of(yaw: f32, pitch: f32) -> Quat {
        Quat::from_rotation_y(std::f32::consts::PI - yaw.to_radians())
            * Quat::from_rotation_x(-pitch.to_radians())
    }

    fn assert_corners(out: &[Vertex], want: [[f32; 3]; 4]) {
        assert_eq!(out.len(), 4);
        for (v, w) in out.iter().zip(want) {
            for i in 0..3 {
                assert!((v.pos[i] - w[i]).abs() < 1e-5, "{:?} vs {w:?}", v.pos);
            }
        }
    }

    // Case 1 is ParticleCornersCheck's hand values; cases 2 and 3 are quaternion math done apart from this code.
    #[test]
    fn corners_match_vanilla_render_rotated_quad() {
        let s = at(
            Vec3::new(1.0, 2.0, 3.0),
            Quat::IDENTITY,
            0.5,
            0xFFFF_FFFF,
            0,
        );
        let out = drawn(
            &one(Facing::Camera, Span::Whole, s, s),
            base_of(180.0, 0.0),
            0.0,
        );
        assert_corners(
            &out,
            [
                [1.5, 1.5, 3.0],
                [1.5, 2.5, 3.0],
                [0.5, 2.5, 3.0],
                [0.5, 1.5, 3.0],
            ],
        );
        let roll = Quat::from_rotation_z(0.3);
        let s = at(Vec3::new(0.0, 1.0, -2.0), roll, 0.25, 0xFFFF_FFFF, 0);
        let camera = base_of(90.0, 30.0);
        let out = drawn(&one(Facing::Camera, Span::Whole, s, s), camera, 0.0);
        assert_corners(
            &out,
            [
                [0.082477, 0.857146, -2.312714],
                [-0.156357, 1.270818, -2.164954],
                [-0.082477, 1.142854, -1.687286],
                [0.156357, 0.729182, -1.835046],
            ],
        );
        let out = drawn(&one(Facing::CameraYaw, Span::Whole, s, s), camera, 0.0);
        assert_corners(
            &out,
            [
                [0.0, 0.835046, -2.312714],
                [0.0, 1.312714, -2.164954],
                [0.0, 1.164954, -1.687286],
                [0.0, 0.687286, -1.835046],
            ],
        );
    }

    #[test]
    fn a_quad_between_two_samples_is_interpolated() {
        let a = at(Vec3::ZERO, Quat::IDENTITY, 0.2, 0xFF00_0000, 0);
        let b = at(Vec3::X, Quat::from_rotation_z(1.0), 0.4, 0xFFFF_FFFF, 0);
        let out = drawn(&one(Facing::Fixed, Span::Whole, a, b), Quat::IDENTITY, 0.25);
        let centre = out.iter().map(|v| Vec3::from(v.pos)).sum::<Vec3>() / 4.0;
        assert!((centre.x - 0.25).abs() < 1e-5);
        let corner = Quat::from_rotation_z(0.25) * Vec3::new(1.0, -1.0, 0.0) * 0.25;
        let got = Vec3::from(out[0].pos) - centre;
        assert!((got - corner).length() < 1e-5, "{got:?} vs {corner:?}");
        // Alpha stays 0xFF, the colour channels sit a quarter of the way up.
        assert_eq!(out[0].color, 255 << 24 | 64 << 16 | 64 << 8 | 64);
    }

    #[test]
    fn a_half_span_draws_only_in_its_half() {
        let s = at(Vec3::ZERO, Quat::IDENTITY, 1.0, 0xFFFF_FFFF, 0);
        let first = one(Facing::Fixed, Span::FirstHalf, s, s);
        assert_eq!(drawn(&first, Quat::IDENTITY, 0.4).len(), 4);
        assert!(drawn(&first, Quat::IDENTITY, 0.6).is_empty());
        let second = one(Facing::Fixed, Span::SecondHalf, s, s);
        assert!(drawn(&second, Quat::IDENTITY, 0.4).is_empty());
        assert_eq!(drawn(&second, Quat::IDENTITY, 0.6).len(), 4);
    }

    #[test]
    fn light_coords_pack_as_native_does() {
        let s = at(
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0,
            0xFFFF_FFFF,
            7 << 4 | 12 << 20,
        );
        let out = drawn(&one(Facing::Fixed, Span::Whole, s, s), Quat::IDENTITY, 0.0);
        assert_eq!(out[0].light, (12 * 16) | ((7 * 16) << 8));
    }

    #[test]
    fn a_sprite_missing_from_the_atlas_is_skipped() {
        let s = at(Vec3::ZERO, Quat::IDENTITY, 1.0, 0xFFFF_FFFF, 0);
        let mut p = one(Facing::Fixed, Span::Whole, s, s);
        p.names[0] = "mod:nothing".into();
        assert!(drawn(&p, Quat::IDENTITY, 0.0).is_empty());
    }
}
