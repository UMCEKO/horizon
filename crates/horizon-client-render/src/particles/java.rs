//! The Java client's quad particles of its last tick, as its recorder wrote them (SPEC-3).

use glam::{DVec3, Quat, Vec3};

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
        assert_eq!((second.facing, second.span), (Facing::Fixed, Span::SecondHalf));
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
}
