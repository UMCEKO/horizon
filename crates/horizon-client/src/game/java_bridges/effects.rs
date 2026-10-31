//! Particles, explosions, drops, items used up.
#![allow(clippy::too_many_arguments)]

use super::*;
use explosion::BlockInteraction;
use horizon_client::particles::{ParticleOptions, ParticleType};
use horizon_java_host::wire::{In, Out};
use horizon_protocol::packets::play::{self, Clientbound as C};

fn stack_of(bytes: &[u8]) -> Option<ItemStack> {
    horizon_client::save::item_from_nbt(&java_mods::read_tag(bytes)?)
}

fn rgb(r: &mut In) -> Option<[f32; 3]> {
    Some([r.float()?, r.float()?, r.float()?])
}

/// What a Java `ParticleOptions` stands for here, and the block of a block particle; None for a type or payload Horizon can't carry.
fn particle_of(
    game: &Game<'_>,
    bytes: &[u8],
    at: [f64; 3],
) -> Option<(ParticleOptions, Option<BlockStateId>)> {
    let mut r = In(bytes);
    let ty = ParticleType::from_name(r.text()?)?;
    Some(match r.int()? {
        0 => (ParticleOptions::simple(ty), None),
        1 => {
            let java = r.int()? as usize;
            let state = horizon_java_host::bridge::STATES
                .get()?
                .to_horizon
                .get(java)
                .copied()
                .flatten()?;
            let block = at.map(|v| v.floor() as i32);
            let (sprite, tint) = game.particle_sprite(state, block)?;
            (ParticleOptions::block(ty, sprite, tint), Some(state))
        }
        2 => {
            let colour = rgb(&mut r)?;
            (ParticleOptions::dust(colour, r.float()?), None)
        }
        3 => {
            let (from, to) = (rgb(&mut r)?, rgb(&mut r)?);
            (ParticleOptions::dust_transition(from, to, r.float()?), None)
        }
        4 => {
            let colour = rgb(&mut r)?;
            (
                ParticleOptions::colored(ty, [colour[0], colour[1], colour[2], r.float()?]),
                None,
            )
        }
        5 => {
            let colour = rgb(&mut r)?;
            let options = ParticleOptions::colored(ty, [colour[0], colour[1], colour[2], 1.0]);
            (
                ParticleOptions {
                    scale: r.float()?,
                    ..options
                },
                None,
            )
        }
        6 => (
            ParticleOptions {
                scale: r.float()?,
                ..ParticleOptions::simple(ty)
            },
            None,
        ),
        _ => return None,
    })
}

/// The players `ServerLevel.sendParticles` reaches: those placed in the dimension whose block is within `limit`.
fn audience(game: &Game<'_>, dimension: &str, at: [f64; 3], limit: f64) -> i32 {
    let Some(host) = game.lan.as_ref() else {
        return 0;
    };
    let near = |g: &horizon_client::lan::guest::Guest| {
        let d: f64 = (0..3)
            .map(|k| (g.position[k].floor() + 0.5 - at[k]).powi(2))
            .sum();
        g.placed && g.dimension == dimension && d < limit * limit
    };
    host.hub.guests.iter().filter(|g| near(g)).count() as i32
}

/// The drops of `Block.dropResources` and `popResource` follow the `block_drops` rule.
fn drops_allowed(game: &Game<'_>) -> bool {
    game.level_rules().block_drops
}

fn drop_resources(game: &mut Game<'static>, state: State, level: Text, pos: Pos) {
    let Some(state) = state.0 else { return };
    if level.0.is_empty() || !drops_allowed(game) {
        return;
    }
    game.with_level(|_, level, _| level.drop_resources_now(pos.0, state));
    game.after_java_change(())
}

horizon_java_host::java_bridges! { table for Game<'static> {
    "net/minecraft/world/level/block/Block".popResource
    "(Lnet/minecraft/world/level/Level;Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/item/ItemStack;)V" =>
    fn block_pop_resource(game, level: Text, pos: Pos, stack: Bytes) -> () {
        let Some(stack) = stack_of(&stack.0) else { return };
        if level.0.is_empty() || !drops_allowed(game) {
            return;
        }
        let at = [pos.0.x, pos.0.y, pos.0.z];
        game.with_level(|_, level, _| level.pop_resource(at, stack));
    }

    "net/minecraft/world/level/block/Block".popResource
    "(Lnet/minecraft/world/level/Level;Ljava/util/function/Supplier;Lnet/minecraft/world/item/ItemStack;)V" =>
    fn block_pop_resource_entity(game, level: Text, entity: Bytes, stack: Bytes) -> () {
        if level.0.is_empty() || stack.0.is_empty() || !drops_allowed(game) {
            return;
        }
        if let Some(tag) = java_mods::read_tag(&entity.0) {
            game.spawn_entity_from_nbt(tag);
        }
    }

    "net/minecraft/world/level/block/Block".dropResources
    "(Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/world/level/Level;Lnet/minecraft/core/BlockPos;)V" =>
    fn block_drop_resources(game, state: State, level: Text, pos: Pos) -> () {
        drop_resources(game, state, level, pos);
    }

    "net/minecraft/world/level/block/Block".dropResources
    "(Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/world/level/LevelAccessor;Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/block/entity/BlockEntity;)V" =>
    fn block_drop_resources_of(game, state: State, level: Text, pos: Pos, _block_entity: Skip) -> () {
        drop_resources(game, state, level, pos);
    }

    "net/minecraft/world/level/block/state/BlockBehaviour".getDrops
    "(Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/world/level/storage/loot/LootParams$Builder;)Ljava/util/List;" =>
    fn block_get_drops(game, state: State, params: Bytes) -> Bytes {
        let drops = (|| {
            let state = state.0?;
            let mut r = In(&params.0);
            let held = stack_of(r.bytes()?);
            let has_origin = r.bool()?;
            let origin = [r.double()?, r.double()?, r.double()?];
            let breaker = r.bool()?;
            let has_radius = r.bool()?;
            let radius = r.float()?;
            let at = if has_origin { origin.map(|v| v.floor() as i32) } else { [0; 3] };
            Some(game.with_level(|_, level, _| {
                let dynamic = level.pending.dynamic(at);
                let (items, registry, seed) = (level.items, level.registry, level.seed);
                let radius = has_radius.then_some(radius);
                match items.block_loot_sequence(registry, state) {
                    Some(sequence) => {
                        let random = level.rand_sequences.get(&sequence, seed);
                        items.block_drops_exploded(registry, state, held.as_ref(), &dynamic, random, None, radius, breaker)
                    }
                    None => items.block_drops_exploded(registry, state, held.as_ref(), &dynamic, &mut level.world.random, None, radius, breaker),
                }
            }))
        })()
        .unwrap_or_default();
        let mut out = Out::default();
        let stacks: Vec<_> = drops
            .iter()
            .filter(|s| &*s.item != "minecraft:air" && s.count > 0)
            .filter_map(|s| java_mods::write_tag(horizon_client::save::item_to_nbt(s)))
            .collect();
        out.int(stacks.len() as i32);
        for bytes in stacks {
            out.bytes(&bytes);
        }
        Bytes(out.0)
    }

    "net/minecraft/advancements/triggers/ItemDurabilityTrigger".trigger
    "(Lnet/minecraft/server/level/ServerPlayer;Lnet/minecraft/world/item/ItemStack;I)V" =>
    fn item_durability_trigger(game, _player: Skip, stack: Bytes, new_damage: i32) -> () {
        if let Some(stack) = stack_of(&stack.0) {
            game.trigger_item_durability(&stack, new_damage.max(0) as u32);
        }
    }

    "net/minecraft/server/level/ServerLevel".sendParticles
    "(Lnet/minecraft/core/particles/ParticleOptions;ZZDDDIDDDDDDLnet/minecraft/network/protocol/game/ClientboundLevelParticlesPacket$RandomizationType;)I" =>
    fn level_send_particles(game, options: Bytes, limiter: bool, always: bool, x: f64, y: f64, z: f64, count: i32, dx: f64, dy: f64, dz: f64, sx: f64, sy: f64, sz: f64, randomization: i32) -> i32 {
        let at = [x, y, z];
        let Some((options, state)) = particle_of(game, &options.0, at) else {
            log::warn!("Horizon: a Java mod's particle carries a payload Horizon can't send");
            return 0;
        };
        let dimension = game.dim.id();
        let uniform = sx == sy && sx == sz && randomization == 0 && !limiter && !always;
        if uniform {
            game.lan_send_particles(dimension, &options, state, at, [dx, dy, dz], sx, count);
        } else if let (Some(particle), Some(host)) = (game.network_particle(&options, state), game.lan.as_ref()) {
            let limit: f64 = if limiter { 512.0 } else { 32.0 };
            let body = C::LevelParticles(play::LevelParticles {
                particle,
                override_limiter: limiter,
                always_show: always,
                pos: at,
                dist: [dx as f32, dy as f32, dz as f32],
                max_speed: [sx as f32, sy as f32, sz as f32],
                count,
                randomization,
            })
            .encode();
            for g in host.hub.guests.iter().filter(|g| g.placed && g.dimension == dimension) {
                let d: f64 = (0..3).map(|k| (g.position[k].floor() + 0.5 - at[k]).powi(2)).sum();
                if d < limit * limit {
                    g.sender.send(body.clone());
                }
            }
        }
        audience(game, dimension, at, if limiter { 512.0 } else { 32.0 })
    }

    "net/minecraft/server/level/ServerLevel".explode
    "(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/world/damagesource/DamageSource;Lnet/minecraft/world/level/ExplosionDamageCalculator;DDDFZLnet/minecraft/world/level/Level$ExplosionInteraction;Lnet/minecraft/core/particles/ParticleOptions;Lnet/minecraft/core/particles/ParticleOptions;Lnet/minecraft/util/random/WeightedList;Lnet/minecraft/core/Holder;)V" =>
    fn level_explode(game, _source: Skip, damage: Text, _calculator: Skip, x: f64, y: f64, z: f64, radius: f32, fire: bool, interaction: i32, _small: Skip, _large: Skip, _block_particles: Skip, _sound: Skip) -> () {
        let interaction = match interaction {
            1 => game.destroy_type("block_explosion_drop_decay"),
            2 if game.ui.rules.bool("mob_griefing") => game.destroy_type("mob_explosion_drop_decay"),
            3 => game.destroy_type("tnt_explosion_drop_decay"),
            4 => {
                log::warn!("Horizon: a Java mod's trigger-only explosion destroys nothing and triggers no blocks");
                BlockInteraction::Keep
            }
            _ => BlockInteraction::Keep,
        };
        let damage_type = if damage.0.is_empty() { "minecraft:explosion" } else { &damage.0 };
        if let Err(e) = game.server_explode_as([x, y, z], radius, interaction, fire, damage_type, None) {
            log::warn!("Horizon: a Java mod's explosion: {e}");
        }
        game.after_java_change(())
    }
}}
