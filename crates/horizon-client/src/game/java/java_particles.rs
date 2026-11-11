// game-module-cfg: feature java-mods
//! The Java registry's particle types: names by id, and the ones mods registered.

use horizon_java_host::wire::In;
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
}
