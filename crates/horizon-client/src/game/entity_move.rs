//! `Entity.move` for the kinds without a port of their own, in vanilla's order through `move_entity`.
use super::commands::Target;
use super::*;
use horizon_client::{
    Result, collisions::SceneCollisions, landing::SceneSound, projectiles::ProjectileKind,
    riding::Ent,
};
use horizon_core::{
    geometry::{Aabb, Direction, VoxelShape, traverse_blocks},
    position::BlockPos,
};
use horizon_world::nbt::Tag;
use horizon_world::{
    block_entity_push::BlockPushWorld,
    block_landing::{
        BlockLandingWorld, DamageSourceOwner, LandingDamage, LandingEntityKind, LandingEvent,
        LandingSound,
    },
    collision_context::CollisionContext,
    entity_move::{
        CollisionFlags, EntityMoveWorld, MoveState, MovementHistory, MoverType, move_entity,
    },
    entity_obstruction::EntityIdentity,
    fluid::{FluidHeightWorld, FluidKind, FluidState},
    ground_support::find_supporting_block,
    movement_emission::{
        self as emission, MovementDistances, MovementEmission, MovementEmissionWorld, MovementEvent,
    },
    movement_properties::{MovementBlockProvider, RegistryMovementWorld},
    movement_restitution::{self, RestitutionInput, RestitutionWorld},
    movement_support::{self, MovementLocation},
    registry::BlockStateId,
};

/// `minecraft:safe_fall_distance`'s default.
const SAFE_FALL_DISTANCE: f64 = 3.0;

/// The `checkFallDamage` the class has.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FallRule {
    Entity,
    /// `LivingEntity.checkFallDamage`: the block's crumbs where a fall landed, then `Entity`'s.
    Living,
    /// `AbstractBoat.checkFallDamage`: the fall is kept and reset, nothing lands on a block.
    Boat,
}

/// One entity's fields `Entity.move` reads and writes, borrowed from the entity taken out of its vector.
pub(super) struct BodyRef<'b> {
    pub id: u32,
    pub position: &'b mut Vec3,
    pub velocity: &'b mut Vec3,
    pub moving: &'b mut MoveState,
    /// `Entity.needsSync`, for the kinds that have one.
    pub sync: Option<&'b mut bool>,
    pub size: [f64; 2],
    pub removed: bool,
    pub no_physics: bool,
    pub step_height: f32,
    /// `getGravity()`.
    pub gravity: f64,
    /// `getAirDrag()`.
    pub air_drag: f32,
    pub emission: MovementEmission,
    pub fall: FallRule,
    /// Whether its first passenger steers it (`getControllingPassenger`).
    pub steered: bool,
}

impl BodyRef<'_> {
    pub(super) fn size_of(bounds: Aabb) -> [f64; 2] {
        let (lo, hi) = (bounds.min_position(), bounds.max_position());
        [hi.x - lo.x, hi.y - lo.y]
    }
}

pub(super) struct KindMoveWorld<'g, 'w, 'b> {
    game: &'g mut Game<'w>,
    body: BodyRef<'b>,
}

struct Blocks<'a>(&'a horizon_client::world::World);

impl MovementBlockProvider for Blocks<'_> {
    type Error = std::io::Error;
    fn movement_block_state(&mut self, p: BlockPos) -> std::io::Result<BlockStateId> {
        Ok(self.0.block(p.x, p.y, p.z))
    }
}

struct FluidView<'a> {
    world: &'a horizon_client::world::World,
    fluids: &'a horizon_client::motion::FluidResources,
}

impl FluidHeightWorld for FluidView<'_> {
    type Error = std::io::Error;
    fn fluid(&mut self, p: [i32; 3]) -> std::io::Result<FluidState> {
        Ok(self.fluids.fluid(self.world.block(p[0], p[1], p[2])))
    }
    fn sturdy_down(&mut self, p: [i32; 3]) -> std::io::Result<bool> {
        let state = self.world.block(p[0], p[1], p[2]);
        Ok(self.fluids.sturdy(state, Direction::Down))
    }
}

fn centred(at: Vec3, size: [f64; 2]) -> Aabb {
    let half = size[0] / 2.0;
    Aabb::new(
        Vec3::new(at.x - half, at.y, at.z - half),
        Vec3::new(at.x + half, at.y + size[1], at.z + half),
    )
}

impl KindMoveWorld<'_, '_, '_> {
    fn bounds(&self) -> Aabb {
        centred(*self.body.position, self.body.size)
    }
    fn block(&self, p: BlockPos) -> BlockStateId {
        self.game.world.block(p.x, p.y, p.z)
    }
    fn at(&self) -> [f64; 3] {
        let p = &*self.body.position;
        [p.x, p.y, p.z]
    }
    fn support(&mut self, offset: f32) -> Result<BlockPos> {
        let res = self.game.res.clone();
        let mut blocks = Blocks(&self.game.world);
        Ok(movement_support::support_position(
            &mut RegistryMovementWorld {
                properties: &res.motion().blocks,
                world: &mut blocks,
            },
            *self.body.position,
            self.body.moving.ground.supporting_block,
            offset,
        )
        .map_err(|e| e.to_string())?)
    }
    /// `Entity.updateFluidInteraction`'s answer for the box now: some water surface reaches its bottom.
    fn touching_water(&self) -> bool {
        let res = &self.game.res;
        let bounds = self.bounds().deflate(Vec3::new(0.001, 0.001, 0.001));
        let (lo, hi) = (bounds.min_position(), bounds.max_position());
        let water = |x: i32, y: i32, z: i32| {
            let state = res.motion().fluids.fluid(self.game.world.block(x, y, z));
            matches!(state.kind(), FluidKind::Water | FluidKind::FlowingWater).then_some(state)
        };
        (lo.x.floor() as i32..=hi.x.floor() as i32).any(|x| {
            (lo.y.floor() as i32..hi.y.ceil() as i32).any(|y| {
                (lo.z.floor() as i32..=hi.z.floor() as i32).any(|z| {
                    water(x, y, z).is_some_and(|state| {
                        let height = if water(x, y + 1, z).is_some() {
                            1.0
                        } else {
                            state.own_height()
                        };
                        f64::from(y) + f64::from(height) >= lo.y
                    })
                })
            })
        })
    }
    fn water_clip(&self, from: Vec3, to: Vec3, p: BlockPos) -> Result<bool> {
        let fluids = &self.game.res.motion().fluids;
        let fluid = fluids.fluid(self.block(p));
        if !matches!(fluid.kind(), FluidKind::Water | FluidKind::FlowingWater) {
            return Ok(false);
        }
        let mut view = FluidView {
            world: &self.game.world,
            fluids,
        };
        let shape = fluids
            .shapes
            .lock()
            .map_err(|_| "fluid shape cache poisoned")?
            .shape(&mut view, fluid, p)?;
        Ok(shape.clip(from, to, p).is_some())
    }
    /// `isClientAuthoritative`: a player steering it decides where it is.
    fn client_authoritative(&self) -> bool {
        self.body.steered
            && self
                .game
                .first_passenger(Ent::Id(self.body.id))
                .is_some_and(Ent::is_player)
    }
    fn step_sound(&mut self, state: BlockStateId, muffled: bool) {
        let sounds = self.game.res.facts().get(state).sounds.clone();
        let (volume, pitch) = if muffled {
            (sounds.volume * 0.05, sounds.pitch * 0.8)
        } else {
            (sounds.volume * 0.15, sounds.pitch)
        };
        let at = self.at();
        self.game.audio.play(&sounds.step, Some(at), volume, pitch);
    }
    /// `LivingEntity.checkFallDamage`'s particles for a fall past the safe distance.
    fn landing_crumbs(&mut self, state: BlockStateId, position: BlockPos) {
        let power = (self.body.moving.fall_distance + 1.0e-6 - SAFE_FALL_DISTANCE)
            .floor()
            .max(0.0);
        if power <= 0.0 || self.game.world.is_air(state) {
            return;
        }
        let at = *self.body.position;
        let (mut x, mut z) = (at.x, at.z);
        let standing = BlockPos::containing(at);
        if position.x != standing.x || position.z != standing.z {
            let x_diff = at.x - f64::from(position.x) - 0.5;
            let z_diff = at.z - f64::from(position.z) - 0.5;
            let widest = x_diff.abs().max(z_diff.abs());
            x = f64::from(position.x) + 0.5 + x_diff / widest * 0.5;
            z = f64::from(position.z) + 0.5 + z_diff / widest * 0.5;
        }
        let scale = (f64::from(0.2_f32) + power / 15.0).min(2.5);
        let count = (150.0 * scale) as i32;
        self.game.landing_dust(
            None,
            state,
            Vec3::new(x, at.y, z),
            count,
            f64::from(0.15_f32),
        );
    }
    fn game_event(&mut self, event: &'static str, state: Option<BlockStateId>) {
        let ctx = self.game.who_context(None, state);
        let at = self.at();
        self.game.post_game_event(event, at, ctx);
    }
}

impl EntityMoveWorld for KindMoveWorld<'_, '_, '_> {
    type Error = Box<dyn std::error::Error>;
    fn no_physics(&self) -> bool {
        self.body.no_physics
    }
    fn position(&self) -> Vec3 {
        *self.body.position
    }
    fn set_position(&mut self, position: Vec3) -> Result<()> {
        *self.body.position = position;
        Ok(())
    }
    fn record_movement(&mut self, _: MoverType, _: Vec3) -> Result<()> {
        Ok(())
    }
    fn collision_flags(&self) -> CollisionFlags {
        self.body.moving.collision
    }
    fn set_collision_flags(&mut self, flags: CollisionFlags) {
        self.body.moving.collision = flags;
    }
    fn limit_piston_movement(&mut self, movement: Vec3) -> Result<Vec3> {
        let time = self.game.world.game_time;
        self.body
            .moving
            .piston
            .limit(movement, || Ok::<i64, Box<dyn std::error::Error>>(time))
    }
    fn stuck_multiplier(&self) -> Vec3 {
        self.body.moving.stuck
    }
    fn set_stuck_multiplier(&mut self, value: Vec3) {
        self.body.moving.stuck = value;
    }
    fn velocity(&mut self) -> Result<Vec3> {
        Ok(*self.body.velocity)
    }
    fn set_velocity(&mut self, value: Vec3) -> Result<()> {
        *self.body.velocity = value;
        Ok(())
    }
    fn back_off_edge(&mut self, movement: Vec3, _: MoverType) -> Result<Vec3> {
        Ok(movement)
    }
    fn collide(&mut self, movement: Vec3) -> Result<Vec3> {
        let bounds = self.bounds();
        let mut scene = SceneCollisions {
            world: &self.game.world,
            binding: self.game.collision,
            subject: None,
        };
        scene.resolve(
            bounds,
            movement,
            self.body.step_height,
            self.body.moving.ground.on_ground,
        )
    }
    fn fall_distance(&self) -> f64 {
        self.body.moving.fall_distance
    }
    fn fall_reset_clip(&mut self, from: Vec3, to: Vec3) -> Result<bool> {
        let res = self.game.res.clone();
        let hit = traverse_blocks(from, to, 1_000_000, |p| {
            let water = match self.water_clip(from, to, p) {
                Ok(hit) => hit,
                Err(e) => return Some(Err(e)),
            };
            let name = &self.game.res.facts().get(self.block(p)).name;
            let reset = res.motion().fall_reset.contains(&**name)
                && VoxelShape::block().clip(from, to, p).is_some();
            (water || reset).then_some(Ok(()))
        })?;
        hit.transpose().map(|h| h.is_some())
    }
    fn reset_fall_distance(&mut self) -> Result<()> {
        self.body.moving.fall_distance = 0.0;
        Ok(())
    }
    fn movement_history(&mut self) -> &mut MovementHistory {
        &mut self.body.moving.history
    }
    fn locally_authoritative(&mut self) -> Result<bool> {
        Ok(if self.game.remote.is_some() {
            self.body.steered && self.game.first_passenger(Ent::Id(self.body.id)) == Some(Ent::Host)
        } else {
            !self.client_authoritative()
        })
    }
    fn set_on_ground_with_movement(
        &mut self,
        below: bool,
        horizontal: bool,
        movement: Vec3,
    ) -> Result<()> {
        let position = *self.body.position;
        let bounds = self.bounds();
        let context = CollisionContext::position(bounds.min_position().y);
        let mut collisions = SceneCollisions {
            world: &self.game.world,
            binding: self.game.collision,
            subject: None,
        };
        self.body.moving.ground.set_with_movement(
            below,
            horizontal,
            bounds,
            Some(movement),
            |bounds| find_supporting_block(&mut collisions, context, position, bounds, 1_000_000),
        )?;
        Ok(())
    }
    fn minor_horizontal_collision(&mut self, _: Vec3) -> Result<bool> {
        Ok(false)
    }
    fn legacy_on_position(&mut self) -> Result<BlockPos> {
        self.support(movement_support::LEGACY_SUPPORT_OFFSET)
    }
    fn block_state(&mut self, p: BlockPos) -> Result<BlockStateId> {
        Ok(self.block(p))
    }
    fn on_ground(&mut self) -> Result<bool> {
        Ok(self.body.moving.ground.on_ground)
    }
    fn check_fall_damage(
        &mut self,
        dy: f64,
        grounded: bool,
        state: BlockStateId,
        position: BlockPos,
    ) -> Result<()> {
        if self.body.fall == FallRule::Boat {
            if !self.game.is_passenger(Ent::Id(self.body.id)) {
                if grounded {
                    self.body.moving.fall_distance = 0.0;
                } else if dy < 0.0 {
                    let below = {
                        let at = BlockPos::containing(*self.body.position);
                        BlockPos { y: at.y - 1, ..at }
                    };
                    let fluid = self.game.res.motion().fluids.fluid(self.block(below));
                    if !matches!(fluid.kind(), FluidKind::Water | FluidKind::FlowingWater) {
                        self.body.moving.fall_distance -= f64::from(dy as f32);
                    }
                }
            }
            return Ok(());
        }
        if self.body.fall == FallRule::Living
            && grounded
            && self.body.moving.fall_distance > 0.0
            && self.game.remote.is_none()
        {
            self.landing_crumbs(state, position);
        }
        if !self.touching_water() && dy < 0.0 {
            self.body.moving.fall_distance -= f64::from(dy as f32);
        }
        if grounded {
            let distance = self.body.moving.fall_distance;
            if distance > 0.0 {
                let res = self.game.res.clone();
                res.motion()
                    .landing
                    .fall_on(self, state, position, LandingEntityKind::Other, distance)
                    .map_err(|e| e.to_string())?;
                let support = self
                    .body
                    .moving
                    .ground
                    .supporting_block
                    .map(|p| self.block(p));
                self.game_event("minecraft:hit_ground", Some(support.unwrap_or(state)));
            }
            self.body.moving.fall_distance = 0.0;
        }
        Ok(())
    }
    fn removed(&mut self) -> Result<bool> {
        Ok(self.body.removed)
    }
    fn can_simulate(&mut self) -> Result<bool> {
        EntityMoveWorld::locally_authoritative(self)
    }
    fn restitute(&mut self, state: BlockStateId, x: bool, z: bool, resolved: Vec3) -> Result<()> {
        let input = RestitutionInput {
            block: state,
            collided_x: x,
            collided_z: z,
            vertical_collision: self.body.moving.collision.vertical,
            collision_below: self.body.moving.collision.below,
            living: false,
            resolved_movement: resolved,
            fall_distance: self.body.moving.fall_distance,
        };
        movement_restitution::restitute_movement(self, input)
    }
    fn client_side(&mut self) -> Result<bool> {
        Ok(self.game.remote.is_some())
    }
    fn movement_emission(&mut self) -> Result<MovementEmission> {
        Ok(self.body.emission)
    }
    fn passenger(&mut self) -> Result<bool> {
        Ok(self.game.is_passenger(Ent::Id(self.body.id)))
    }
    fn apply_movement_emission(
        &mut self,
        kind: MovementEmission,
        movement: Vec3,
        position: BlockPos,
        state: BlockStateId,
    ) -> Result<()> {
        emission::apply_movement_emission(self, kind, movement, position, state)
    }
    fn block_speed_factor(&mut self) -> Result<f32> {
        let res = self.game.res.clone();
        let location = MovementLocation {
            position: *self.body.position,
            block_position: BlockPos::containing(*self.body.position),
            supporting_block: self.body.moving.ground.supporting_block,
        };
        let mut blocks = Blocks(&self.game.world);
        Ok(movement_support::block_speed_factor(
            &mut RegistryMovementWorld {
                properties: &res.motion().blocks,
                world: &mut blocks,
            },
            location,
        )
        .map_err(|e| e.to_string())?)
    }
}

impl RestitutionWorld for KindMoveWorld<'_, '_, '_> {
    type Error = Box<dyn std::error::Error>;
    fn suppressing_bounce(&mut self) -> Result<bool> {
        Ok(false)
    }
    fn entity_bounciness(&mut self) -> Result<f64> {
        Ok(0.0)
    }
    fn velocity(&mut self) -> Result<Vec3> {
        Ok(*self.body.velocity)
    }
    fn effective_gravity(&mut self) -> Result<f64> {
        Ok(self.body.gravity)
    }
    fn block_suppresses_bounce(&mut self, state: BlockStateId) -> Result<bool> {
        Ok(self.game.res.motion().blocks.get(state)?.suppresses_bounce)
    }
    fn block_bounce_restitution(&mut self, state: BlockStateId) -> Result<f32> {
        Ok(self
            .game
            .res
            .motion()
            .blocks
            .get(state)?
            .coefficients
            .bounce_restitution)
    }
    fn air_drag(&mut self) -> Result<f32> {
        Ok(self.body.air_drag)
    }
    fn movement_support_position(&mut self) -> Result<BlockPos> {
        self.support(movement_support::MOVEMENT_SUPPORT_OFFSET)
    }
    fn bounce_on(&mut self, state: BlockStateId, position: BlockPos, _: f64) -> Result<()> {
        let res = self.game.res.clone();
        res.motion()
            .landing
            .bounce_on(self, state, position, LandingEntityKind::Other)
            .map_err(|e| e.to_string().into())
    }
    fn bounce_event(&mut self) -> Result<()> {
        self.game_event("minecraft:bounce", None);
        Ok(())
    }
    fn mark_position_for_sync(&mut self) {
        if let Some(sync) = self.body.sync.as_deref_mut() {
            *sync = true;
        }
    }
    fn set_velocity(&mut self, value: Vec3) -> Result<()> {
        *self.body.velocity = value;
        Ok(())
    }
}

impl MovementEmissionWorld for KindMoveWorld<'_, '_, '_> {
    type Error = Box<dyn std::error::Error>;
    type State = BlockStateId;
    fn distances(&self) -> MovementDistances {
        self.body.moving.steps
    }
    fn set_movement_distance(&mut self, value: f32) {
        self.body.moving.steps.movement = value;
    }
    fn set_flight_distance(&mut self, value: f32) {
        self.body.moving.steps.flight = value;
    }
    fn set_next_step(&mut self, value: f32) {
        self.body.moving.steps.next_step = value;
    }
    fn on_position(&mut self) -> Result<BlockPos> {
        self.support(movement_support::ON_SUPPORT_OFFSET)
    }
    fn block_state(&mut self, p: BlockPos) -> Result<BlockStateId> {
        Ok(self.block(p))
    }
    fn is_air(&mut self, s: BlockStateId) -> Result<bool> {
        Ok(self.game.world.is_air(s))
    }
    fn climbable_tag(&mut self, s: BlockStateId) -> Result<bool> {
        let name = &self.game.res.facts().get(s).name;
        Ok(self.game.res.motion().climbable.contains(&**name))
    }
    fn is_powder_snow(&mut self, s: BlockStateId) -> Result<bool> {
        Ok(&*self.game.res.facts().get(s).name == "minecraft:powder_snow")
    }
    fn on_ground(&mut self) -> Result<bool> {
        Ok(self.body.moving.ground.on_ground)
    }
    fn crouching(&mut self) -> Result<bool> {
        Ok(false)
    }
    fn on_rails(&mut self) -> Result<bool> {
        Ok(false)
    }
    fn swimming(&mut self) -> Result<bool> {
        Ok(false)
    }
    fn in_water(&mut self) -> Result<bool> {
        Ok(self.touching_water())
    }
    /// `Entity.walkingStepSound`'s step sound, without the amethyst chime.
    fn walking_step_sound(&mut self, position: BlockPos, state: BlockStateId) -> Result<()> {
        let muffled = self.touching_water();
        if muffled {
            self.water_swim_sound()?;
            self.step_sound(state, true);
            return Ok(());
        }
        let above = BlockPos {
            y: position.y + 1,
            ..position
        };
        let above_state = self.block(above);
        let res = self.game.res.clone();
        let name = &*self.game.res.facts().get(above_state).name;
        let combination = res.motion().combination_step_sound.contains(name);
        if combination || res.motion().inside_step_sound.contains(name) {
            self.step_sound(above_state, false);
            if combination {
                self.step_sound(state, true);
            }
        } else {
            self.step_sound(state, false);
        }
        Ok(())
    }
    fn step_event(&mut self, s: BlockStateId) -> Result<()> {
        self.game_event("minecraft:step", Some(s));
        Ok(())
    }
    fn next_step(&mut self) -> Result<f32> {
        Ok(self.body.moving.steps.next_step())
    }
    fn water_swim_sound(&mut self) -> Result<()> {
        let v = *self.body.velocity;
        let volume = 1.0_f32.min(
            (v.x * v.x * f64::from(0.2_f32) + v.y * v.y + v.z * v.z * f64::from(0.2_f32)).sqrt()
                as f32
                * 0.35,
        );
        let random = &mut self.game.world.random;
        let pitch = 1.0 + (random.next_float() - random.next_float()) * 0.4;
        let at = self.at();
        self.game
            .audio
            .play("minecraft:entity.generic.swim", Some(at), volume, pitch);
        Ok(())
    }
    fn entity_event(&mut self, event: MovementEvent) -> Result<()> {
        self.game_event(
            match event {
                MovementEvent::Step => "minecraft:step",
                MovementEvent::Swim => "minecraft:swim",
                MovementEvent::Flap => "minecraft:flap",
            },
            None,
        );
        Ok(())
    }
    fn flapping(&mut self) -> Result<bool> {
        Ok(false)
    }
    fn on_flap(&mut self) -> Result<()> {
        Ok(())
    }
    fn movement_emission(&mut self) -> Result<MovementEmission> {
        Ok(self.body.emission)
    }
}

impl BlockPushWorld for KindMoveWorld<'_, '_, '_> {
    type Error = Box<dyn std::error::Error>;
    fn collision_shape(&mut self, state: BlockStateId, p: BlockPos) -> Result<VoxelShape> {
        let mut world = SceneCollisions {
            world: &self.game.world,
            binding: self.game.collision,
            subject: None,
        };
        self.game
            .collision
            .collision_shape(&mut world, state, [p.x, p.y, p.z])
            .map_err(|e| e.to_string().into())
    }
    fn entities(&mut self, _: Aabb) -> Result<Vec<EntityIdentity>> {
        Ok(Vec::new())
    }
    fn entity_bounds(&self, _: EntityIdentity) -> Aabb {
        self.bounds()
    }
    fn teleport_relative(&mut self, _: EntityIdentity, movement: Vec3) -> Result<()> {
        *self.body.position = self.body.position.add_vector(movement);
        Ok(())
    }
}

fn landing_sound(sound: LandingSound) -> &'static str {
    match sound {
        LandingSound::HoneySlide => "minecraft:block.honey_block.slide",
        LandingSound::HoneyFall => "minecraft:block.honey_block.fall",
        LandingSound::TurtleEggBreak => "minecraft:block.turtle_egg.break",
        LandingSound::ShelfMushroomBounce => "minecraft:block.shelf_fungus.bounce",
    }
}

impl BlockLandingWorld for KindMoveWorld<'_, '_, '_> {
    type DamageSource = LandingDamage;
    type Sound = SceneSound;
    fn is_server_level(&self) -> bool {
        self.game.remote.is_none()
    }
    fn is_client_side(&mut self) -> Result<bool> {
        Ok(self.game.remote.is_some())
    }
    fn random_float(&mut self) -> Result<f32> {
        Ok(self.game.world.random.next_float())
    }
    fn random_int(&mut self, bound: i32) -> Result<i32> {
        Ok(self.game.world.random.next_int_bound(bound)?)
    }
    fn under_spawn_protection(&mut self, _: BlockPos) -> Result<bool> {
        Ok(false)
    }
    fn mob_griefing(&mut self) -> Result<bool> {
        Ok(true)
    }
    fn entity_dimensions(&self) -> (f32, f32) {
        (self.body.size[0] as f32, self.body.size[1] as f32)
    }
    fn suppressing_bounce(&mut self) -> Result<bool> {
        Ok(false)
    }
    fn damage_source(
        &mut self,
        _: DamageSourceOwner,
        kind: LandingDamage,
    ) -> Result<LandingDamage> {
        Ok(kind)
    }
    fn cause_fall_damage(&mut self, _: f64, _: f32, _: LandingDamage) -> Result<bool> {
        Ok(false)
    }
    fn known_sound(&self, s: LandingSound) -> SceneSound {
        SceneSound::Block(s)
    }
    fn living_fall_sound(&mut self, big: bool) -> Result<SceneSound> {
        Ok(if big {
            SceneSound::BigFall
        } else {
            SceneSound::SmallFall
        })
    }
    fn entity_sound(&mut self, sound: SceneSound, volume: f32, pitch: f32) -> Result<()> {
        if let SceneSound::Block(s) = sound {
            let at = self.at();
            self.game
                .audio
                .play(landing_sound(s), Some(at), volume, pitch);
        }
        Ok(())
    }
    fn block_sound(
        &mut self,
        position: BlockPos,
        sound: SceneSound,
        volume: f32,
        pitch: f32,
    ) -> Result<()> {
        if let SceneSound::Block(s) = sound {
            let at = [
                f64::from(position.x) + 0.5,
                f64::from(position.y) + 0.5,
                f64::from(position.z) + 0.5,
            ];
            self.game
                .audio
                .play(landing_sound(s), Some(at), volume, pitch);
        }
        Ok(())
    }
    fn broadcast_entity_event(&mut self, event: u8) -> Result<()> {
        let at = self.at();
        self.game
            .lan_entity_event(self.body.id as i32, event as i8, at);
        Ok(())
    }
    fn destroy_block(&mut self, position: BlockPos, drops: bool) -> Result<bool> {
        let _ = (position, drops);
        Err("landing breaks no block for an entity that is not living".into())
    }
    fn set_block(&mut self, position: BlockPos, state: BlockStateId, flags: u32) -> Result<bool> {
        let _ = (position, state, flags);
        Err("landing changes no block for an entity that is not living".into())
    }
    fn block_event(
        &mut self,
        _: LandingEvent,
        _: BlockPos,
        _: BlockStateId,
        _: bool,
    ) -> Result<()> {
        Ok(())
    }
    fn level_event(&mut self, event: i32, position: BlockPos, data: i32) -> Result<()> {
        let _ = (event, position, data);
        Ok(())
    }
}

impl Game<'_> {
    /// `Entity.move(moverType, delta)` of one native entity; false when it isn't in the level or its kind has no port.
    pub(super) fn entity_move(&mut self, target: Target, mover: MoverType, delta: Vec3) -> bool {
        match target {
            Target::Orb(id) => {
                let Some(i) = self.orbs.iter().position(|o| o.id == id) else {
                    return false;
                };
                let mut orb = self.orbs.remove(i);
                let body = BodyRef {
                    id,
                    size: BodyRef::size_of(orb.bounds()),
                    removed: orb.removed,
                    position: &mut orb.position,
                    velocity: &mut orb.velocity,
                    moving: &mut orb.moving,
                    sync: None,
                    no_physics: false,
                    step_height: 0.0,
                    gravity: 0.03,
                    air_drag: 0.98,
                    emission: MovementEmission::None,
                    fall: FallRule::Entity,
                    steered: false,
                };
                self.move_body(body, mover, delta);
                self.orbs.insert(i, orb);
                true
            }
            Target::Projectile(id) => {
                let Some(i) = self.creatures.projectiles.iter().position(|p| p.id == id) else {
                    return false;
                };
                let mut p = self.creatures.projectiles.remove(i);
                let arrow = p.arrow.is_some();
                let (gravity, air_drag) = (p.gravity(), p.air_drag());
                let body = BodyRef {
                    id,
                    size: BodyRef::size_of(p.bounds()),
                    removed: p.removed,
                    position: &mut p.position,
                    velocity: &mut p.velocity,
                    moving: &mut p.moving,
                    sync: Some(&mut p.needs_sync),
                    no_physics: matches!(p.kind, ProjectileKind::ShulkerBullet { .. })
                        || p.arrow.as_ref().is_some_and(|a| a.no_physics),
                    step_height: 0.0,
                    gravity,
                    air_drag,
                    emission: if arrow {
                        MovementEmission::None
                    } else {
                        MovementEmission::All
                    },
                    fall: FallRule::Entity,
                    steered: false,
                };
                self.move_body(body, mover, delta);
                if arrow && mover != MoverType::SelfMovement {
                    let rain = self.arrow_rain();
                    horizon_client::arrows::after_move(&mut p, &self.arrow_scene(&rain));
                }
                self.creatures.projectiles.insert(i, p);
                true
            }
            Target::Boat(id) => {
                let Some(i) = self.boats.iter().position(|b| b.id == id) else {
                    return false;
                };
                let mut boat = self.boats.remove(i);
                let body = BodyRef {
                    id,
                    size: BodyRef::size_of(boat.bounds()),
                    removed: false,
                    position: &mut boat.position,
                    velocity: &mut boat.velocity,
                    moving: &mut boat.moving,
                    sync: None,
                    no_physics: false,
                    step_height: 0.0,
                    gravity: 0.04,
                    air_drag: 1.0,
                    emission: MovementEmission::Events,
                    fall: FallRule::Boat,
                    steered: true,
                };
                self.move_body(body, mover, delta);
                self.boats.insert(i, boat);
                true
            }
            Target::Stand(id) => {
                let Some(i) = self.armor_stands.iter().position(|a| a.id == id) else {
                    return false;
                };
                let mut stand = self.armor_stands.remove(i);
                let body = BodyRef {
                    id,
                    size: BodyRef::size_of(stand.bounds()),
                    removed: stand.removed,
                    position: &mut stand.position,
                    velocity: &mut stand.velocity,
                    moving: &mut stand.moving,
                    sync: None,
                    no_physics: stand.marker,
                    step_height: 0.0,
                    gravity: 0.08,
                    air_drag: 0.98,
                    emission: MovementEmission::All,
                    fall: FallRule::Living,
                    steered: false,
                };
                self.move_body(body, mover, delta);
                self.armor_stands.insert(i, stand);
                true
            }
            Target::Crystal(id) => {
                let Some(i) = self.crystals.iter().position(|c| c.id == id) else {
                    return false;
                };
                let mut crystal = self.crystals.remove(i);
                // An end crystal holds no motion of its own.
                let mut velocity = Vec3::ZERO;
                let body = BodyRef {
                    id,
                    size: [2.0, 2.0],
                    removed: false,
                    position: &mut crystal.position,
                    velocity: &mut velocity,
                    moving: &mut crystal.moving,
                    sync: None,
                    no_physics: false,
                    step_height: 0.0,
                    gravity: 0.0,
                    air_drag: 0.98,
                    emission: MovementEmission::None,
                    fall: FallRule::Entity,
                    steered: false,
                };
                self.move_body(body, mover, delta);
                self.crystals.insert(i, crystal);
                true
            }
            Target::Bolt(id) => {
                let Some(i) = self.bolts.iter().position(|b| b.id == id) else {
                    return false;
                };
                let mut bolt = self.bolts.remove(i);
                // A lightning bolt holds no motion, and its box has no size.
                let mut position = Vec3::new(bolt.at[0], bolt.at[1], bolt.at[2]);
                let mut velocity = Vec3::ZERO;
                let body = BodyRef {
                    id,
                    size: [0.0, 0.0],
                    removed: bolt.removed,
                    position: &mut position,
                    velocity: &mut velocity,
                    moving: &mut bolt.moving,
                    sync: None,
                    no_physics: false,
                    step_height: 0.0,
                    gravity: 0.0,
                    air_drag: 0.98,
                    emission: MovementEmission::All,
                    fall: FallRule::Entity,
                    steered: false,
                };
                self.move_body(body, mover, delta);
                bolt.at = [position.x, position.y, position.z];
                self.bolts.insert(i, bolt);
                true
            }
            Target::Other(id) => {
                let Some(i) = self.others.iter().position(|o| o.id == id) else {
                    return false;
                };
                let mut other = self.others.remove(i);
                // Its motion lives in the saved compound.
                let mut velocity = self
                    .persist
                    .raw
                    .get(&id)
                    .map_or(Vec3::ZERO, horizon_client::entity_nbt::motion);
                let body = BodyRef {
                    id,
                    size: self.type_size(&other.kind),
                    removed: false,
                    position: &mut other.position,
                    velocity: &mut velocity,
                    moving: &mut other.moving,
                    sync: None,
                    no_physics: false,
                    step_height: 0.0,
                    gravity: 0.0,
                    air_drag: 0.98,
                    emission: MovementEmission::All,
                    fall: FallRule::Entity,
                    steered: false,
                };
                self.move_body(body, mover, delta);
                if let Some(Tag::Compound(raw)) = self.persist.raw.get_mut(&id) {
                    raw.insert(
                        "Motion".into(),
                        Tag::List(
                            [velocity.x, velocity.y, velocity.z]
                                .map(|v| Tag::Double(v.to_bits()))
                                .to_vec(),
                        ),
                    );
                }
                self.others.insert(i, other);
                true
            }
            _ => false,
        }
    }

    fn move_body(&mut self, body: BodyRef<'_>, mover: MoverType, delta: Vec3) {
        let mut world = KindMoveWorld { game: self, body };
        if let Err(e) = move_entity(&mut world, mover, delta) {
            log::warn!("Horizon: Entity.move failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::crossing_tests::flat_game;
    use super::*;
    use horizon_client::entities::{ArmorStand, ExperienceOrb};
    use horizon_client::projectiles::Projectile;

    fn orb(game: &mut Game<'_>, at: Vec3) -> u32 {
        game.next_entity += 1;
        let id = game.next_entity;
        game.orbs.push(ExperienceOrb {
            id,
            position: at,
            previous: at,
            velocity: Vec3::ZERO,
            value: 1,
            age: 0,
            moving: Default::default(),
            count: 1,
            ticks: 0,
            removed: false,
        });
        id
    }

    #[test]
    fn an_orb_moved_into_a_wall_stops_and_flags_horizontal_collision() {
        let Some(mut game) = flat_game() else { return };
        let stone = game
            .pack
            .assets
            .registry
            .default_state("minecraft:stone")
            .unwrap();
        for y in 61..64 {
            game.world.set(3, y, 0, stone).unwrap();
        }
        let id = orb(&mut game, Vec3::new(0.5, 61.0, 0.5));
        assert!(game.entity_move(
            Target::Orb(id),
            MoverType::SelfMovement,
            Vec3::new(5.0, 0.0, 0.0)
        ));
        let orb = game.orbs.iter().find(|o| o.id == id).unwrap();
        assert!((orb.position.x - 2.75).abs() < 1.0e-6, "{}", orb.position.x);
        assert!(orb.moving.collision.horizontal);
        assert!(!orb.moving.collision.vertical);
    }

    #[test]
    fn an_orb_moved_down_onto_the_floor_lands() {
        let Some(mut game) = flat_game() else { return };
        let id = orb(&mut game, Vec3::new(0.5, 63.0, 0.5));
        game.entity_move(
            Target::Orb(id),
            MoverType::SelfMovement,
            Vec3::new(0.0, -5.0, 0.0),
        );
        let orb = game.orbs.iter().find(|o| o.id == id).unwrap();
        assert_eq!(orb.position.y, 61.0);
        assert!(orb.moving.ground.on_ground && orb.moving.collision.below);
        assert_eq!(orb.moving.fall_distance, 0.0);
    }

    fn arrow_stuck_in_the_air(game: &mut Game<'_>) -> u32 {
        game.next_entity += 1;
        let id = game.next_entity;
        let mut p = Projectile::new(
            ProjectileKind::Arrow {
                base_damage: 2.0,
                critical: false,
                effect: None,
            },
            Vec3::new(0.5, 63.0, 0.5),
            None,
        );
        p.id = id;
        p.arrow.as_mut().unwrap().in_ground = true;
        p.stuck = true;
        game.creatures.projectiles.push(p);
        id
    }

    #[test]
    fn an_arrow_moved_by_a_piston_starts_falling() {
        let Some(mut game) = flat_game() else { return };
        let id = arrow_stuck_in_the_air(&mut game);
        assert!(game.entity_move(
            Target::Projectile(id),
            MoverType::Piston,
            Vec3::new(0.1, 0.0, 0.0)
        ));
        let arrow = game
            .creatures
            .projectiles
            .iter()
            .find(|p| p.id == id)
            .unwrap();
        assert!(!arrow.arrow.as_ref().unwrap().in_ground && !arrow.stuck);
        assert!((arrow.position.x - 0.6).abs() < 1.0e-9);
    }

    #[test]
    fn an_arrow_moved_by_itself_stays_in_its_block() {
        let Some(mut game) = flat_game() else { return };
        let id = arrow_stuck_in_the_air(&mut game);
        game.entity_move(
            Target::Projectile(id),
            MoverType::SelfMovement,
            Vec3::new(0.1, 0.0, 0.0),
        );
        let arrow = game
            .creatures
            .projectiles
            .iter()
            .find(|p| p.id == id)
            .unwrap();
        assert!(arrow.arrow.as_ref().unwrap().in_ground);
    }

    fn boat_at(game: &mut Game<'_>, at: Vec3) -> u32 {
        game.next_entity += 1;
        let id = game.next_entity;
        game.boats
            .push(super::super::boats::Boat::new(id, "oak".into(), at, 0.0));
        id
    }

    #[test]
    fn a_boat_moved_down_onto_slime_bounces() {
        let Some(mut game) = flat_game() else { return };
        let slime = game
            .pack
            .assets
            .registry
            .default_state("minecraft:slime_block")
            .unwrap();
        for x in -2..3 {
            for z in -2..3 {
                game.world.set(x, 60, z, slime).unwrap();
            }
        }
        let id = boat_at(&mut game, Vec3::new(0.5, 63.0, 0.5));
        game.boats.iter_mut().find(|b| b.id == id).unwrap().velocity = Vec3::new(0.0, -1.0, 0.0);
        game.entity_move(
            Target::Boat(id),
            MoverType::SelfMovement,
            Vec3::new(0.0, -3.0, 0.0),
        );
        let boat = game.boats.iter().find(|b| b.id == id).unwrap();
        assert!(boat.moving.collision.below && boat.moving.ground.on_ground);
        assert!(boat.velocity.y > 0.0, "{:?}", boat.velocity);
    }

    #[test]
    fn a_boat_pushed_into_a_wall_flags_horizontal_collision() {
        let Some(mut game) = flat_game() else { return };
        let stone = game
            .pack
            .assets
            .registry
            .default_state("minecraft:stone")
            .unwrap();
        for y in 61..64 {
            for z in -2..3 {
                game.world.set(4, y, z, stone).unwrap();
            }
        }
        let id = boat_at(&mut game, Vec3::new(0.5, 61.0, 0.5));
        game.boats
            .iter_mut()
            .find(|b| b.id == id)
            .unwrap()
            .moving
            .fall_distance = 5.0;
        game.entity_move(
            Target::Boat(id),
            MoverType::SelfMovement,
            Vec3::new(6.0, 0.0, 0.0),
        );
        let boat = game.boats.iter().find(|b| b.id == id).unwrap();
        assert!(boat.moving.collision.horizontal);
        assert!((boat.position.x - (4.0 - 1.375 / 2.0)).abs() < 1.0e-6);
    }

    #[test]
    fn an_armor_stand_moved_down_lands_and_a_marker_goes_through_the_floor() {
        let Some(mut game) = flat_game() else { return };
        game.next_entity += 1;
        let (a, b) = (game.next_entity, game.next_entity + 1);
        game.next_entity += 1;
        game.armor_stands
            .push(ArmorStand::new(a, Vec3::new(0.5, 63.0, 0.5), 0.0));
        let mut marker = ArmorStand::new(b, Vec3::new(3.5, 63.0, 0.5), 0.0);
        marker.marker = true;
        game.armor_stands.push(marker);
        for id in [a, b] {
            game.entity_move(
                Target::Stand(id),
                MoverType::SelfMovement,
                Vec3::new(0.0, -5.0, 0.0),
            );
        }
        let landed = game.armor_stands.iter().find(|s| s.id == a).unwrap();
        assert_eq!(landed.position.y, 61.0);
        assert!(landed.moving.ground.on_ground);
        let through = game.armor_stands.iter().find(|s| s.id == b).unwrap();
        assert_eq!(through.position.y, 58.0);
    }

    #[test]
    fn an_end_crystal_moves_by_the_delta_and_stops_at_a_wall() {
        let Some(mut game) = flat_game() else { return };
        let stone = game
            .pack
            .assets
            .registry
            .default_state("minecraft:stone")
            .unwrap();
        for y in 61..64 {
            for z in -2..3 {
                game.world.set(4, y, z, stone).unwrap();
            }
        }
        game.next_entity += 1;
        let id = game.next_entity;
        let at = Vec3::new(0.5, 61.0, 0.5);
        game.crystals.push(super::super::dragon::Crystal {
            id,
            position: at,
            show_bottom: true,
            time: 0,
            beam: None,
            invulnerable: false,
            moving: Default::default(),
        });
        game.entity_move(
            Target::Crystal(id),
            MoverType::SelfMovement,
            Vec3::new(1.0, 0.0, 0.0),
        );
        assert_eq!(game.crystals[0].position.x, 1.5);
        game.entity_move(
            Target::Crystal(id),
            MoverType::SelfMovement,
            Vec3::new(9.0, 0.0, 0.0),
        );
        let crystal = &game.crystals[0];
        assert_eq!(crystal.position.x, 3.0);
        assert!(crystal.moving.collision.horizontal);
    }

    #[test]
    fn a_lightning_bolt_moves_by_the_delta() {
        let Some(mut game) = flat_game() else { return };
        game.next_entity += 1;
        let id = game.next_entity;
        game.bolts.push(super::super::lightning::Bolt::new(
            id,
            [0.5, 62.0, 0.5],
            1,
            0,
            true,
            [0; 4],
        ));
        assert!(game.entity_move(
            Target::Bolt(id),
            MoverType::SelfMovement,
            Vec3::new(2.0, 0.0, 1.0)
        ));
        assert_eq!(game.bolts[0].at, [2.5, 62.0, 1.5]);
    }

    #[test]
    fn an_entity_of_a_type_native_does_not_simulate_moves_and_lands() {
        let Some(mut game) = flat_game() else { return };
        game.next_entity += 1;
        let id = game.next_entity;
        game.others.push(super::commands::OtherEntity {
            id,
            kind: "minecraft:item_display".into(),
            position: Vec3::new(0.5, 63.0, 0.5),
            yaw: 0.0,
            pitch: 0.0,
            moving: Default::default(),
        });
        assert!(game.entity_move(
            Target::Other(id),
            MoverType::SelfMovement,
            Vec3::new(0.0, -5.0, 0.0)
        ));
        let other = game.others.iter().find(|o| o.id == id).unwrap();
        assert!(other.moving.ground.on_ground);
        assert_eq!(other.position.y, 61.0);
    }
}
