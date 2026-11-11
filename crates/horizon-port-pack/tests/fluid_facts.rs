//! The game's own fluid of every vanilla block state against the native derivation.
//! `HORIZON_FACTS=<facts.json> HORIZON_BLOCK_REGISTRY=<block-registry.nbt> cargo test -p horizon-port-pack --test fluid_facts -- --ignored`
use horizon_core::nbt::{self, Limits};
use horizon_port_pack::oracle::Facts;
use horizon_world::{
    fluid::{FluidKind, FluidState},
    registry::{BlockRegistry, BlockStateId},
};
use std::{collections::BTreeMap, path::PathBuf};

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set")))
}

fn properties(key: &str) -> BTreeMap<String, String> {
    key.split(',')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

#[test]
#[ignore = "needs the facts a game boot wrote and a prepared block registry"]
fn vanilla_state_fluids_match_the_native_derivation() {
    let facts = Facts::read(&env_path("HORIZON_FACTS")).expect("facts");
    let registry = BlockRegistry::from_nbt(
        &nbt::read_named(
            &std::fs::read(env_path("HORIZON_BLOCK_REGISTRY")).expect("registry file"),
            Limits::default(),
        )
        .expect("registry nbt")
        .0
        .tag,
    )
    .expect("registry");
    let mut mismatches = Vec::new();
    for i in 0..registry.len() {
        let state = registry.state(BlockStateId(i as u32)).unwrap();
        if !state.name.starts_with("minecraft:") {
            continue;
        }
        let Some(block) = facts.blocks.get(&state.name) else {
            continue;
        };
        let native = FluidState::from_block(state).expect("derivable");
        let wanted: BTreeMap<String, String> = state
            .properties
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let Some((key, exported)) = block
            .states
            .iter()
            .find(|(key, _)| properties(key) == wanted)
        else {
            mismatches.push(format!("{} {wanted:?}: not in the facts", state.name));
            continue;
        };
        let agrees = match &exported.fluid {
            None => native.kind() == FluidKind::Empty,
            Some(f) => {
                native.kind().name() == f.id
                    && native.amount() == f.amount
                    && native.falling() == f.falling
                    && native.is_source() == f.source
            }
        };
        if !agrees {
            mismatches.push(format!(
                "{}[{key}]: game {:?}, native {} amount {} falling {}",
                state.name,
                exported.fluid,
                native.kind().name(),
                native.amount(),
                native.falling()
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}
