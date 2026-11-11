//! `Entity.move` for the kinds without a port of their own, in vanilla's order through `move_entity`.
#![cfg_attr(
    not(feature = "java-mods"),
    expect(dead_code, reason = "only the Java layer's Entity#move bridge calls it")
)]
use super::commands::Target;
use super::entity::Entity as _;
use super::*;
use horizon_client::entity_rules::EntityRules as _;
use horizon_client::{
    Result, collisions::SceneCollisions, landing::SceneSound, motion::ModBlockFactors,
    projectiles::ProjectileKind,
};
use horizon_core::{
    geometry::{Aabb, Direction, VoxelShape, traverse_blocks},
    position::BlockPos,
};
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
    /// The type's id, for the block parts it touches.
    pub kind: &'b str,
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

/// The parts of a [`BodyRef`] its kind fixes.
#[derive(Clone, Copy)]
struct Physics {
    no_physics: bool,
    gravity: f64,
    air_drag: f32,
    emission: MovementEmission,
    fall: FallRule,
    steered: bool,
}

impl Physics {
    /// A plain `Entity`'s: physics, its own `checkFallDamage` and no one steering.
    fn entity(gravity: f64, air_drag: f32, emission: MovementEmission) -> Self {
        Self {
            no_physics: false,
            gravity,
            air_drag,
            emission,
            fall: FallRule::Entity,
            steered: false,
        }
    }
}

/// A copy of one entity's moving fields, lent to [`Game::move_body`] while the entity stays in its list.
struct Lent {
    position: Vec3,
    velocity: Vec3,
    moving: MoveState,
    sync: bool,
}

impl Lent {
    fn of(position: Vec3, velocity: Vec3, moving: &MoveState) -> Self {
        Self {
            position,
            velocity,
            moving: moving.clone(),
            sync: false,
        }
    }
    fn restore(&self, position: &mut Vec3, velocity: &mut Vec3, moving: &mut MoveState) {
        *position = self.position;
        *velocity = self.velocity;
        moving.clone_from(&self.moving);
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
        let context = self.game.loop_context.clone();
        let mut blocks = Blocks(&self.game.world);
        Ok(movement_support::support_position(
            &mut RegistryMovementWorld {
                properties: &res.motion().blocks,
                world: &mut blocks,
                edits: &ModBlockFactors(&context),
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
                .seated_passengers(self.body.id)
                .first()
                .is_some_and(|&(_, _, player)| player)
    }
    /// `Entity.isPassenger`: only players and mobs ride here, and no kind moved through a body is either.
    fn rides(&self) -> bool {
        false
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
        self.game
            .landing_dust(state, Vec3::new(x, at.y, z), count, f64::from(0.15_f32));
    }
    fn game_event(&mut self, event: &'static str, state: Option<BlockStateId>) {
        let ctx = self.game.entity_event_context(self.body.id, state);
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
            self.body.steered && self.game.controls_boat(self.body.id)
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
            if !self.rides() {
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
        Ok(self.rides())
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
        let context = self.game.loop_context.clone();
        let mut blocks = Blocks(&self.game.world);
        Ok(movement_support::block_speed_factor(
            &mut RegistryMovementWorld {
                properties: &res.motion().blocks,
                world: &mut blocks,
                edits: &ModBlockFactors(&context),
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
    fn bounce_on(
        &mut self,
        state: BlockStateId,
        position: BlockPos,
        fall_distance: f64,
        movement: Vec3,
    ) -> Result<Vec3> {
        let res = self.game.res.clone();
        res.motion()
            .landing
            .bounce_on(self, state, position, LandingEntityKind::Other)
            .map_err(|e| e.to_string())?;
        let game = &mut *self.game;
        Ok(horizon_client::block_contact::bounce(
            super::entity::damage_level_of!(game),
            horizon_client::block_contact::Contact {
                entity: self.body.id as i32,
                kind: self.body.kind,
                state,
                position,
            },
            fall_distance,
            movement,
        ))
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
            Target::Mob(id) => self.mob_move(id, delta),
            Target::Item(id) => self.item_move(id, delta),
            Target::Orb(id) => self.orb_move(id, mover, delta),
            Target::Stand(id) => self.stand_move(id, mover, delta),
            Target::Projectile(id) => self.projectile_move(id, mover, delta),
            Target::Boat(id) => self.boat_move(id, mover, delta),
            Target::Crystal(id) => self.crystal_move(id, mover, delta),
            Target::Bolt(id) => self.bolt_move(id, mover, delta),
            Target::Dragon(id) => self.dragon_move(id, mover, delta),
            Target::Tnt(id) => self.tnt_move(id, delta),
            Target::Cart(id) => self.cart_move(id, delta),
            Target::Frame(_) | Target::Painting(_) => self.block_attached_move(target, delta),
            _ => false,
        }
    }

    /// `Entity.move` of the copy in `lent`, which the caller writes back.
    fn move_lent(
        &mut self,
        id: u32,
        kind: &str,
        size: [f64; 2],
        lent: &mut Lent,
        physics: Physics,
        mover: MoverType,
        delta: Vec3,
    ) {
        let body = BodyRef {
            id,
            kind,
            position: &mut lent.position,
            velocity: &mut lent.velocity,
            moving: &mut lent.moving,
            sync: Some(&mut lent.sync),
            size,
            removed: false,
            no_physics: physics.no_physics,
            step_height: 0.0,
            gravity: physics.gravity,
            air_drag: physics.air_drag,
            emission: physics.emission,
            fall: physics.fall,
            steered: physics.steered,
        };
        self.move_body(body, mover, delta);
    }

    fn orb_move(&mut self, id: u32, mover: MoverType, delta: Vec3) -> bool {
        let Some(orb) = self.orbs.iter().find(|o| o.id == id) else {
            return false;
        };
        let (size, kind) = (BodyRef::size_of(orb.bounds()), orb.native_type());
        let mut lent = Lent::of(orb.position, orb.velocity, &orb.moving);
        let physics = Physics::entity(orb.get_gravity(), 0.98, MovementEmission::None);
        self.move_lent(id, &kind, size, &mut lent, physics, mover, delta);
        if let Some(i) = self.orbs.iter().position(|o| o.id == id) {
            self.orbs.update(i, |o| lent.restore(&mut o.position, &mut o.velocity, &mut o.moving));
        }
        true
    }

    fn stand_move(&mut self, id: u32, mover: MoverType, delta: Vec3) -> bool {
        let Some(stand) = self.armor_stands.iter().find(|s| s.id == id) else {
            return false;
        };
        let (size, kind) = (BodyRef::size_of(stand.bounds()), stand.native_type());
        let mut lent = Lent::of(stand.position, stand.velocity, &stand.moving);
        let physics = Physics {
            no_physics: stand.marker || stand.no_gravity,
            fall: FallRule::Living,
            ..Physics::entity(stand.get_gravity(), 0.98, MovementEmission::All)
        };
        self.move_lent(id, &kind, size, &mut lent, physics, mover, delta);
        if let Some(i) = self.armor_stands.iter().position(|s| s.id == id) {
            self.armor_stands
                .update(i, |s| lent.restore(&mut s.position, &mut s.velocity, &mut s.moving));
        }
        true
    }

    /// `Entity.move` of a projectile, then `AbstractArrow.move`'s fall for an arrow pushed out of its block.
    fn projectile_move(&mut self, id: u32, mover: MoverType, delta: Vec3) -> bool {
        let Some(p) = self.creatures.projectiles.iter().find(|p| p.id == id) else {
            return false;
        };
        let (size, kind) = (BodyRef::size_of(p.bounds()), p.native_type());
        let arrow = p.arrow.is_some();
        let mut lent = Lent::of(p.position, p.velocity, &p.moving);
        lent.sync = p.needs_sync;
        let physics = Physics {
            no_physics: matches!(p.kind, ProjectileKind::ShulkerBullet { .. })
                || p.arrow.as_ref().is_some_and(|a| a.no_physics),
            ..Physics::entity(
                p.get_gravity(),
                p.air_drag(),
                if arrow {
                    MovementEmission::None
                } else {
                    MovementEmission::All
                },
            )
        };
        self.move_lent(id, &kind, size, &mut lent, physics, mover, delta);
        let Some(i) = self.creatures.projectiles.iter().position(|p| p.id == id) else {
            return true;
        };
        let mut p = self.creatures.projectiles[i].clone();
        lent.restore(&mut p.position, &mut p.velocity, &mut p.moving);
        p.needs_sync = lent.sync;
        if arrow && mover != MoverType::SelfMovement {
            let rain = self.arrow_rain();
            horizon_client::arrows::after_move(&mut p, &self.arrow_scene(&rain));
        }
        self.creatures.projectiles.update(i, |slot| *slot = p);
        true
    }

    fn boat_move(&mut self, id: u32, mover: MoverType, delta: Vec3) -> bool {
        let Some(boat) = self.boats.iter().find(|b| b.id == id) else {
            return false;
        };
        let (size, kind) = (BodyRef::size_of(boat.bounds()), boat.native_type());
        let mut lent = Lent::of(boat.position, boat.velocity, &boat.moving);
        let physics = Physics {
            fall: FallRule::Boat,
            steered: true,
            ..Physics::entity(boat.get_gravity(), 1.0, MovementEmission::Events)
        };
        self.move_lent(id, &kind, size, &mut lent, physics, mover, delta);
        if let Some(i) = self.boats.iter().position(|b| b.id == id) {
            self.boats
                .update(i, |b| lent.restore(&mut b.position, &mut b.velocity, &mut b.moving));
        }
        true
    }

    /// `Entity.move` of an end crystal, which holds no motion and keeps no move state between moves.
    fn crystal_move(&mut self, id: u32, mover: MoverType, delta: Vec3) -> bool {
        let Some(crystal) = self.crystals.iter().find(|c| c.id == id) else {
            return false;
        };
        let size = BodyRef::size_of(crystal.bounds());
        let mut lent = Lent::of(crystal.position, Vec3::ZERO, &MoveState::default());
        let physics = Physics::entity(0.0, 0.98, MovementEmission::None);
        self.move_lent(id, "minecraft:end_crystal", size, &mut lent, physics, mover, delta);
        if let Some(i) = self.crystals.iter().position(|c| c.id == id) {
            self.crystals.update(i, |c| c.position = lent.position);
        }
        true
    }

    /// `Entity.move` of a lightning bolt, whose box has no size.
    fn bolt_move(&mut self, id: u32, mover: MoverType, delta: Vec3) -> bool {
        let Some(bolt) = self.bolts.iter().find(|b| b.id == id) else {
            return false;
        };
        let at = Vec3::new(bolt.at[0], bolt.at[1], bolt.at[2]);
        let mut lent = Lent::of(at, Vec3::ZERO, &MoveState::default());
        let physics = Physics::entity(0.0, 0.98, MovementEmission::All);
        let kind = "minecraft:lightning_bolt";
        self.move_lent(id, kind, [0.0; 2], &mut lent, physics, mover, delta);
        if let Some(i) = self.bolts.iter().position(|b| b.id == id) {
            let p = lent.position;
            self.bolts.update(i, |b| b.at = [p.x, p.y, p.z]);
        }
        true
    }

    /// `Entity.move` of the dragon, which has `noPhysics`: it goes the whole way through blocks.
    fn dragon_move(&mut self, id: u32, mover: MoverType, delta: Vec3) -> bool {
        let Some(dragon) = self.dragon.as_ref().filter(|d| d.id == id) else {
            return false;
        };
        // `EntityType.ENDER_DRAGON`'s `sized(16.0F, 8.0F)`.
        let size = [16.0, 8.0];
        let mut lent = Lent::of(dragon.position, Vec3::ZERO, &MoveState::default());
        let physics = Physics {
            no_physics: true,
            fall: FallRule::Living,
            ..Physics::entity(0.08, 0.98, MovementEmission::All)
        };
        let kind = "minecraft:ender_dragon";
        self.move_lent(id, kind, size, &mut lent, physics, mover, delta);
        if let Some(dragon) = self.dragon.as_mut().filter(|d| d.id == id) {
            dragon.position = lent.position;
        }
        true
    }

    /// `Entity.move` of a primed TNT: [`PrimedTnt::move_by`], its block effects left to its tick.
    fn tnt_move(&mut self, id: u32, delta: Vec3) -> bool {
        let Some(i) = self.tnts.iter().position(|t| t.id == id) else {
            return false;
        };
        let mut tnt = self.tnts[i].clone();
        tnt.move_by(
            &self.world,
            self.collision,
            self.res.facts(),
            &self.loop_context,
            Some(self.border.geometry()),
            Some(&self.res.motion().blocks),
            &[],
            delta,
        );
        self.tnts.update(i, |slot| *slot = tnt);
        true
    }

    /// `Entity.move` of a minecart: `move_cart`, as its own tick moves it.
    fn cart_move(&mut self, id: u32, delta: Vec3) -> bool {
        let Some(i) = self.carts.iter().position(|c| c.id == id) else {
            return false;
        };
        let mut cart = self.carts[i].clone();
        self.move_cart(&mut cart, delta);
        self.carts.update(i, |slot| *slot = cart);
        true
    }

    /// `BlockAttachedEntity.move`: on a server, any movement kills the entity and drops its item with no breaker.
    fn block_attached_move(&mut self, target: Target, delta: Vec3) -> bool {
        let index = match target {
            Target::Frame(id) => self.item_frames.iter().position(|f| f.id == id),
            Target::Painting(id) => self.paintings.iter().position(|p| p.id == id),
            _ => None,
        };
        let Some(index) = index else {
            return false;
        };
        if self.remote.is_some() || delta.length_squared() <= 0.0 {
            return true;
        }
        match target {
            Target::Frame(_) => self.kill_frame_and_drop(index),
            _ => {
                self.break_painting(index, false);
            }
        }
        true
    }

    /// `Entity.move` of a mob: [`Mob::do_move_by`] with the mob lent out of its list.
    fn mob_move(&mut self, id: u32, delta: Vec3) -> bool {
        let Some(i) = self.mobs.iter().position(|m| m.id == id) else {
            return false;
        };
        let _save_lease = horizon_client::join_seq::visibility::PassLease::enter();
        let mut mob = self.mobs.extract(self.mobs[i].join).unwrap();
        let mut events = Vec::new();
        let mut lent = horizon_client::health::LentLevel::take(&mut self.world);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let world = self.mob_world_base(&[]);
            let mut level = horizon_client::health::DamageLevel::lent(
                &self.world,
                self.pack.assets.registry,
                &mut lent,
                Some(world_items!(self)),
                Some(&self.pack.block_registries),
                &self.damage_hooks,
            );
            mob.do_move_by(&world, &mut level, delta, &mut events);
        }));
        lent.give_back(&mut self.world);
        self.mobs.put_back(i, mob);
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
        if let Err(e) = self.handle_mob_events(events) {
            log::warn!("Horizon: a mob's move events failed: {e}");
        }
        true
    }

    /// `Entity.move` of a dropped item: [`ItemEntity::move_by`], then its landing and block effects as its tick runs them.
    fn item_move(&mut self, id: u32, delta: Vec3) -> bool {
        let Some(i) = self.dropped.iter().position(|item| item.id == id) else {
            return false;
        };
        // `getEntityCollisions` along the move: what `canBeCollidedWith` the item.
        let colliders = self.item_entity_collisions();
        let _save_lease = horizon_client::join_seq::visibility::PassLease::enter();
        let mut item = self.dropped.extract(self.dropped[i].join).unwrap();
        let mut step = item.move_by(
            &self.world,
            self.res.facts(),
            self.collision,
            delta,
            false,
            &colliders,
            Some(self.border.geometry()),
            Some(&self.res.motion().blocks),
        );
        let client = self.remote.is_some();
        if self.item_moved(&mut item, &mut step, client) {
            self.dropped.retire_extracted(item.id, item.join);
            return true;
        }
        item.finish_move_block_effects(
            &self.world,
            self.res.facts(),
            false,
            Some(&self.res.motion().blocks),
            step,
            &self.loop_context,
        );
        self.dropped.put_back(i, item);
        true
    }

    fn move_body(&mut self, body: BodyRef<'_>, mover: MoverType, delta: Vec3) {
        let mut world = KindMoveWorld { game: self, body };
        if let Err(e) = move_entity(&mut world, mover, delta) {
            log::warn!("Horizon: Entity.move failed: {e}");
        }
    }
}
