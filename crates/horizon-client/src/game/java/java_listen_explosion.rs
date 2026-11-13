// game-module-cfg: feature java-mods
// java-listener-family
//! A server explosion's start, detonation and knockbacks (`ExplosionEvent.Start`, `.Detonate`, `ExplosionKnockbackEvent`), posted on the level.

use super::Game;
use super::explosion::BlastEntity;
use super::java_listener::{Family, Subscriptions, crossed, listens, with_game};
use horizon_core::position::BlockPos;
use horizon_java_host::wire::{In, Out};
use mod_api::{
    ExplosionDetonateEvent, ExplosionKnockbackEvent, ExplosionStartEvent, Flow, HookId, Listener, ServerHost,
};
use std::cell::{Cell, RefCell};

const HOOKS: &[HookId] = &[
    HookId::ExplosionStart,
    HookId::ExplosionDetonate,
    HookId::ExplosionKnockback,
];
static SUBSCRIBED: Subscriptions = Subscriptions::new();

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(Explosions),
    }
}

/// The blast going off, as every one of its events carries it.
#[derive(Clone, Debug, PartialEq)]
struct Blast {
    serial: u64,
    center: [f64; 3],
    radius: f32,
    fire: bool,
    interaction: u8,
    damage_type: String,
    source: i32,
    owner: i32,
    /// The entities `ExplosionEvent.Detonate` left, once it was posted.
    survivors: Option<Vec<BlastEntity>>,
}

thread_local! {
    /// The blasts in progress, innermost last: one set off inside a listener nests.
    static BLASTS: RefCell<Vec<Blast>> = const { RefCell::new(Vec::new()) };
    static SERIAL: Cell<u64> = const { Cell::new(0) };
}

fn current() -> Option<Blast> {
    BLASTS.with_borrow(|blasts| blasts.last().cloned())
}

impl Game<'_> {
    /// Notes the blast starting when a mod listens to one of its events; whether it did, for [`Self::java_blast_leave`].
    pub(super) fn java_blast_enter(
        &self,
        center: [f64; 3],
        radius: f32,
        fire: bool,
        interaction: u8,
        damage_type: &str,
    ) -> bool {
        if !HOOKS.iter().any(|hook| listens(*hook)) {
            return false;
        }
        let serial = SERIAL.replace(SERIAL.get() + 1);
        let blast = Blast {
            serial,
            center,
            radius,
            fire,
            interaction,
            damage_type: damage_type.to_owned(),
            source: self.combat.explosion_source.map_or(-1, |id| id as i32),
            owner: self.blast_attacker().map_or(-1, |(id, _)| id as i32),
            survivors: None,
        };
        BLASTS.with_borrow_mut(|blasts| blasts.push(blast));
        true
    }

    /// The entities `ExplosionEvent.Detonate` left of the blast going off: None when it was not posted.
    pub(super) fn java_blast_survivors(&self) -> Option<Vec<BlastEntity>> {
        BLASTS.with_borrow(|blasts| blasts.last().and_then(|blast| blast.survivors.clone()))
    }

    /// The blast [`Self::java_blast_enter`] noted is over.
    pub(super) fn java_blast_leave(&self, entered: bool) {
        if entered {
            BLASTS.with_borrow_mut(|blasts| blasts.pop());
        }
    }
}

fn write_header(out: &mut Out, blast: &Blast) {
    out.long(blast.serial as i64);
    blast.center.iter().for_each(|c| out.double(*c));
    out.float(blast.radius);
    out.bool(blast.fire);
    out.0.push(blast.interaction);
    out.int(blast.source);
    out.int(blast.owner);
    out.text(&blast.damage_type);
}

fn write_blocks(out: &mut Out, blocks: &[[i32; 3]]) {
    out.int(blocks.len() as i32);
    for b in blocks {
        out.long(BlockPos::new(b[0], b[1], b[2]).pack());
    }
}

fn write_entities(out: &mut Out, entities: &[BlastEntity]) {
    out.int(entities.len() as i32);
    for entity in entities {
        match *entity {
            BlastEntity::Entity(id) => {
                out.0.push(0);
                out.int(id as i32);
            }
            BlastEntity::Player(uuid) => {
                out.0.push(1);
                write_uuid(out, uuid);
            }
        }
    }
}

fn write_uuid(out: &mut Out, [a, b, c, d]: [i32; 4]) {
    out.long(((a as i64) << 32) | (b as u32 as i64));
    out.long(((c as i64) << 32) | (d as u32 as i64));
}

fn read_entities(read: &mut In<'_>) -> Option<Vec<BlastEntity>> {
    let count = usize::try_from(read.int()?).ok()?;
    let mut entities = Vec::with_capacity(count.min(read.0.len()));
    for _ in 0..count {
        entities.push(match read.byte()? {
            0 => BlastEntity::Entity(u32::try_from(read.int()?).ok()?),
            1 => {
                let (hi, lo) = (read.long()?, read.long()?);
                BlastEntity::Player([(hi >> 32) as i32, hi as i32, (lo >> 32) as i32, lo as i32])
            }
            _ => return None,
        });
    }
    Some(entities)
}

fn read_blocks_from(read: &mut In<'_>) -> Option<Vec<[i32; 3]>> {
    let count = usize::try_from(read.int()?).ok()?;
    let mut blocks = Vec::with_capacity(count.min(read.0.len() / 8));
    for _ in 0..count {
        let pos = BlockPos::unpack(read.long()?);
        blocks.push([pos.x, pos.y, pos.z]);
    }
    Some(blocks)
}

/// Detonate's reply: the positions, then the entities, as the listeners left them.
fn read_detonated(result: &[u8]) -> Option<(Vec<[i32; 3]>, Vec<BlastEntity>)> {
    let mut read = In(result);
    let blocks = read_blocks_from(&mut read)?;
    Some((blocks, read_entities(&mut read)?))
}

fn read_vec3(result: &[u8]) -> Option<[f64; 3]> {
    let mut read = In(result);
    Some([read.double()?, read.double()?, read.double()?])
}

struct Explosions;

/// Posts `event` on the level with the current blast's header ahead of `fields`; the reply, when Java answered.
fn post(
    host: &mut dyn ServerHost,
    hook: HookId,
    event: &str,
    fields: impl FnOnce(&mut Out, &mut Game<'static>, &Blast),
) -> Option<(bool, Vec<u8>)> {
    crossed(hook);
    let Some(blast) = current() else {
        log::error!("Horizon: {event} fired outside a blast Horizon noted; it is not posted");
        return None;
    };
    with_game(host, |game| {
        let mut body = Out::default();
        write_header(&mut body, &blast);
        fields(&mut body, game, &blast);
        game.java_post_level_event(event, false, |out| out.0.extend(body.0))
    })
    .flatten()
}

impl mod_api::Hooks for Explosions {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED, false)
    }

    fn listeners(&self) -> &'static [Listener] {
        super::java_listener::listeners(HOOKS, &SUBSCRIBED, false)
    }

    fn explosion_start(&self, host: &mut dyn ServerHost, _: &mut ExplosionStartEvent<'_>) -> Flow {
        match post(host, HookId::ExplosionStart, "ExplosionEvent.Start", |_, _, _| {}) {
            Some((true, _)) => Flow::Cancel,
            _ => Flow::Continue,
        }
    }

    fn explosion_detonate(&self, host: &mut dyn ServerHost, event: &mut ExplosionDetonateEvent<'_>) {
        let blocks: &Vec<[i32; 3]> = event.blocks;
        let posted = post(host, HookId::ExplosionDetonate, "ExplosionEvent.Detonate", |out, game, blast| {
            write_blocks(out, blocks);
            write_entities(out, &game.explosion_entities(blast.center, blast.radius));
        });
        if let Some((left, survivors)) = posted.and_then(|(_, result)| read_detonated(&result)) {
            *event.blocks = left;
            BLASTS.with_borrow_mut(|blasts| {
                if let Some(blast) = blasts.last_mut() {
                    blast.survivors = Some(survivors);
                }
            });
        }
    }

    fn explosion_knockback(&self, host: &mut dyn ServerHost, event: &mut ExplosionKnockbackEvent<'_>) {
        let (entity, blocks, push) = (event.entity, event.blocks, *event.knockback_velocity);
        let posted = post(host, HookId::ExplosionKnockback, "ExplosionKnockbackEvent", |out, game, _| {
            match entity {
                Some(id) => {
                    out.0.push(0);
                    out.int(id as i32);
                }
                None => {
                    out.0.push(1);
                    write_uuid(out, game.acting_uuid());
                }
            }
            push.iter().for_each(|v| out.double(*v));
            write_blocks(out, blocks);
        });
        if let Some(pushed) = posted.and_then(|(_, result)| read_vec3(&result)) {
            *event.knockback_velocity = pushed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_detonate_reply_reads_blocks_and_entities() {
        let mut out = Out::default();
        write_blocks(&mut out, &[[1, 64, -3], [-7, 0, 9]]);
        out.int(2);
        out.0.push(0);
        out.int(7);
        out.0.push(1);
        out.long(0x0000_0001_0000_0002);
        out.long(-1);
        let (blocks, entities) = read_detonated(&out.0).unwrap();
        assert_eq!(blocks, [[1, 64, -3], [-7, 0, 9]]);
        assert_eq!(entities, [BlastEntity::Entity(7), BlastEntity::Player([1, 2, -1, -1])]);
    }

    #[test]
    fn entity_refs_round_trip_through_the_wire() {
        let entities = [BlastEntity::Player([i32::MIN, -5, 7, i32::MAX]), BlastEntity::Entity(3)];
        let mut out = Out::default();
        write_entities(&mut out, &entities);
        assert_eq!(read_entities(&mut In(&out.0)), Some(entities.to_vec()));
    }

    #[test]
    fn a_short_or_unknown_detonate_reply_is_not_read() {
        let mut out = Out::default();
        write_blocks(&mut out, &[[1, 64, -3]]);
        out.int(1);
        out.0.push(2);
        out.int(7);
        assert_eq!(read_detonated(&out.0), None);
        assert_eq!(read_detonated(&out.0[..4]), None);
        assert_eq!(read_detonated(&[]), None);
    }

    #[test]
    fn a_knockback_reply_is_three_doubles() {
        let mut out = Out::default();
        [0.5, -1.0, 2.25].iter().for_each(|v| out.double(*v));
        assert_eq!(read_vec3(&out.0), Some([0.5, -1.0, 2.25]));
        assert_eq!(read_vec3(&out.0[..23]), None);
    }

    #[test]
    fn the_header_reads_back_field_by_field() {
        let blast = Blast {
            serial: 4,
            center: [1.5, 2.5, -3.5],
            radius: 4.0,
            fire: true,
            interaction: 2,
            damage_type: "minecraft:explosion".into(),
            source: -1,
            owner: 12,
            survivors: None,
        };
        let mut out = Out::default();
        write_header(&mut out, &blast);
        let mut read = In(&out.0);
        assert_eq!(read.long(), Some(4));
        assert_eq!([read.double(), read.double(), read.double()], [Some(1.5), Some(2.5), Some(-3.5)]);
        assert_eq!(read.float(), Some(4.0));
        assert_eq!(read.bool(), Some(true));
        assert_eq!(read.byte(), Some(2));
        assert_eq!(read.int(), Some(-1));
        assert_eq!(read.int(), Some(12));
        assert_eq!(read.text(), Some("minecraft:explosion"));
        assert!(read.0.is_empty());
    }
}
