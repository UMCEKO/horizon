// game-module-cfg: feature java-mods
//! The Java registry's particle types: names by id, and the ones mods registered.

use super::Game;
use horizon_java_host::wire::{In, Out};
use horizon_world::registry::BlockStateId;
use std::collections::HashMap;
use std::sync::OnceLock;

pub(super) struct ParticleFacts {
    names: Vec<String>,
    owned: Vec<bool>,
}

fn parse(reply: &[u8]) -> Option<ParticleFacts> {
    let mut reply = In(reply);
    let count = usize::try_from(reply.int()?).ok()?;
    let mut names = Vec::new();
    let mut owned = Vec::new();
    for _ in 0..count {
        names.push(reply.text()?.to_owned());
        owned.push(reply.bool()?);
    }
    Some(ParticleFacts { names, owned })
}

impl ParticleFacts {
    /// The mods' particle types, by name.
    fn owned_names(&self) -> impl Iterator<Item = &str> {
        self.names
            .iter()
            .zip(&self.owned)
            .filter_map(|(name, owned)| owned.then_some(name.as_str()))
    }

    fn name(&self, id: i32) -> Option<&str> {
        self.names
            .get(usize::try_from(id).ok()?)
            .map(String::as_str)
    }
}

/// Fetched once from the booted JVM; None without one, and empty when the reply doesn't read.
fn java_particle_facts() -> Option<&'static ParticleFacts> {
    static FACTS: OnceLock<Option<ParticleFacts>> = OnceLock::new();
    FACTS
        .get_or_init(|| {
            let host = horizon_java_host::get()?;
            let reply = host.request("particle_facts", &[])?;
            Some(parse(&reply).unwrap_or_else(|| {
                log::error!("Horizon: the Java particle facts were malformed");
                ParticleFacts {
                    names: Vec::new(),
                    owned: Vec::new(),
                }
            }))
        })
        .as_ref()
}

/// The particle types mods registered, for the game's particle key table.
pub(super) fn java_owned_particles() -> Vec<String> {
    java_particle_facts()
        .map(|facts| facts.owned_names().map(str::to_owned).collect())
        .unwrap_or_default()
}

/// The Java registry's name for a particle id on the wire.
pub(super) fn java_particle_name(id: i32) -> Option<&'static str> {
    java_particle_facts()?.name(id)
}

/// By Horizon's state id: 1 when the state's mod makes its destroy particles in Java, 2 its hit particles.
pub(super) fn java_block_effects(state: BlockStateId) -> u8 {
    static BY_STATE: OnceLock<HashMap<BlockStateId, u8>> = OnceLock::new();
    let table = BY_STATE.get_or_init(|| {
        let Some(map) = horizon_java_host::bridge::STATES.get() else {
            return HashMap::new();
        };
        effects_by_state(super::java_models::client_block_effects(), &map.to_horizon)
    });
    table.get(&state).copied().unwrap_or(0)
}

fn effects_by_state(
    effects: &[(i32, u8)],
    to_horizon: &[Option<BlockStateId>],
) -> HashMap<BlockStateId, u8> {
    effects
        .iter()
        .filter_map(|&(java, bits)| {
            let state = (*to_horizon.get(usize::try_from(java).ok()?)?)?;
            Some((state, bits))
        })
        .collect()
}

/// Vanilla's 3D data value of a face normal: down, up, north, south, west, east.
pub(super) fn direction(face: [i32; 3]) -> u8 {
    match face {
        [0, -1, 0] => 0,
        [0, 1, 0] => 1,
        [0, 0, -1] => 2,
        [0, 0, 1] => 3,
        [-1, 0, 0] => 4,
        _ => 5,
    }
}

/// Whether a Java mod registered this particle type.
pub(super) fn java_particle_owned(name: &str) -> bool {
    java_particle_facts().is_some_and(|facts| facts.owned_names().any(|owned| owned == name))
}

#[allow(clippy::too_many_arguments)]
fn server_particles_request(
    dim: &str,
    spec: &str,
    pos: [f64; 3],
    delta: [f32; 3],
    speed: f32,
    count: i32,
    force: bool,
    viewers: &[u128],
) -> Vec<u8> {
    let mut out = Out::default();
    out.text(dim);
    out.text(spec);
    pos.into_iter().for_each(|v| out.double(v));
    delta.into_iter().for_each(|v| out.float(v));
    out.float(speed);
    out.int(count);
    out.bool(force);
    out.int(viewers.len() as i32);
    for uuid in viewers {
        out.long((uuid >> 64) as i64);
        out.long(*uuid as i64);
    }
    out.0
}

/// The players reached, or the codec's message after `-1`.
fn parse_server_particles(reply: &[u8]) -> Result<i32, String> {
    let malformed = || "the Java particle reply was malformed".to_owned();
    let mut reply = In(reply);
    match reply.int().ok_or_else(malformed)? {
        n if n >= 0 => Ok(n),
        _ => Err(reply.text().ok_or_else(malformed)?.to_owned()),
    }
}

impl Game<'_> {
    /// `ParticleCommand.sendParticles` for players Horizon chose, in Java.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn java_server_particles(
        &mut self,
        dim: &str,
        spec: &str,
        pos: [f64; 3],
        delta: [f32; 3],
        speed: f32,
        count: i32,
        force: bool,
        viewers: &[u128],
    ) -> Result<i32, String> {
        let request = server_particles_request(dim, spec, pos, delta, speed, count, force, viewers);
        let reply = self
            .java_request("server_particles", &request)
            .ok_or_else(|| "no Java".to_owned())?;
        parse_server_particles(&reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_java_host::wire::Out;

    fn reply(entries: &[(&str, bool)]) -> Vec<u8> {
        let mut out = Out::default();
        out.int(entries.len() as i32);
        for (name, owned) in entries {
            out.text(name);
            out.bool(*owned);
        }
        out.0
    }

    #[test]
    fn a_reply_names_the_types_and_which_are_mods() {
        let bytes = reply(&[
            ("minecraft:flame", false),
            ("mekanism:laser", true),
            ("ae2:vibrant_fx", true),
        ]);
        let facts = parse(&bytes).unwrap();
        assert_eq!(
            facts.owned_names().collect::<Vec<_>>(),
            ["mekanism:laser", "ae2:vibrant_fx"]
        );
        assert_eq!(facts.name(2), Some("ae2:vibrant_fx"));
        assert_eq!(facts.name(3), None);
        assert_eq!(facts.name(-1), None);
    }

    #[test]
    fn a_reply_cut_before_a_flag_is_refused() {
        let bytes = reply(&[("minecraft:flame", false), ("mekanism:laser", true)]);
        assert!(parse(&bytes[..bytes.len() - 1]).is_none());
    }

    #[test]
    fn faces_map_to_vanillas_data_values() {
        let faces = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];
        assert_eq!(faces.map(direction), [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn effects_map_java_ids_through_the_state_table() {
        let to_horizon = [None, Some(BlockStateId(40)), Some(BlockStateId(41))];
        let table = effects_by_state(&[(1, 1), (2, 3), (9, 2), (-1, 1), (0, 2)], &to_horizon);
        assert_eq!(table.len(), 2);
        assert_eq!(table[&BlockStateId(40)], 1);
        assert_eq!(table[&BlockStateId(41)], 3);
    }

    #[test]
    fn the_server_particles_request_has_the_documented_layout() {
        let bytes = server_particles_request(
            "overworld",
            "x:spark",
            [1.0, 2.0, 3.0],
            [0.0; 3],
            0.5,
            4,
            false,
            &[(1u128 << 64) | 2],
        );
        let mut want = Out::default();
        want.text("overworld");
        want.text("x:spark");
        for v in [1.0f64, 2.0, 3.0] {
            want.double(v);
        }
        for v in [0.0f32, 0.0, 0.0, 0.5] {
            want.float(v);
        }
        want.int(4);
        want.bool(false);
        want.int(1);
        want.long(1);
        want.long(2);
        assert_eq!(bytes, want.0);
    }

    #[test]
    fn a_particle_reply_is_a_count_or_the_codecs_message() {
        let mut ok = Out::default();
        ok.int(3);
        assert_eq!(parse_server_particles(&ok.0), Ok(3));
        let mut bad = Out::default();
        bad.int(-1);
        bad.text("bad");
        assert_eq!(parse_server_particles(&bad.0), Err("bad".to_owned()));
    }
}
