//! Geometry the Java client bakes from a mod block entity's data (cable parts, inserted cells): the
//! replies are parsed here into `dynamic_models`, which the mesher draws from, and the sections remesh.

use super::*;
use horizon_assets::model::{BakedLayer, BakedQuadData, Direction};
use horizon_client::{dynamic_models, dynamic_renders};
use horizon_java_host::wire::In;

struct Entry {
    pos: [i32; 3],
    key: i32,
    quads: Option<Vec<BakedQuadData>>,
}

fn quad(input: &mut In) -> Option<BakedQuadData> {
    let mut vertices = [[0.0; 5]; 4];
    for value in vertices.iter_mut().flatten() {
        *value = input.float()?;
    }
    let sprite = input.text()?.to_owned();
    let face = *Direction::ALL.get(usize::from(input.byte()?))?;
    let cull = Direction::ALL.get(usize::from(input.byte()?)).copied();
    let tint = input.int()?;
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
    Some(BakedQuadData {
        vertices,
        sprite,
        face,
        cull,
        tint,
        shade,
        emission,
        layer,
    })
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
            for _ in 0..quads {
                list.push(quad(&mut input)?);
            }
            Some(list)
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
            if let Some(quads) = quads
                && dynamic_models::put_geometry(key, quads.into())
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
        let quads = entries[0].quads.as_ref().unwrap();
        assert_eq!(quads.len(), 2);
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
