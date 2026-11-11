// game-module-cfg: feature java-mods
//! Shapes of mods' blocks that depend on their block entity: the server's collisions from the server's Java block, the window's from the Java client's copy.
#![allow(dead_code)]

use super::java_screens::live::{java_client_running, java_shape_generation};
use super::*;
use horizon_core::geometry::{Aabb, ShapeOp, Vec3, VoxelShape};
use horizon_java_host::wire::{In, Out};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

const CACHE_LIMIT: usize = 4096;

type Cached = (BlockStateId, i32, Vec<[f64; 6]>);

thread_local! {
    static CACHE: RefCell<HashMap<[i32; 3], Cached>> = RefCell::default();
    static COLLISIONS_TAKEN: Cell<Option<i32>> = const { Cell::new(None) };
}

/// Java's `VoxelShape.toAabbs`, six doubles a box, as one shape.
fn boxes_shape(boxes: &[f64]) -> Option<VoxelShape> {
    let mut shape = VoxelShape::empty();
    for b in boxes.as_chunks::<6>().0 {
        let part = VoxelShape::from_box(Aabb::new(
            Vec3::new(b[0], b[1], b[2]),
            Vec3::new(b[3], b[4], b[5]),
        ))
        .ok()?;
        shape = shape.join_unoptimized(&part, ShapeOp::OR).ok()?;
    }
    Some(shape)
}

/// Whether this state is a mod's with a block entity, whose shape its code may take from that entity.
fn shaped_by_java(state: BlockStateId) -> bool {
    horizon_java_host::bridge::STATES.get().is_some_and(|map| {
        let at = state.0 as usize;
        map.owned.get(at).copied().unwrap_or(false) && map.entity.get(at).copied().unwrap_or(false)
    })
}

impl Game<'_> {
    /// The block-local boxes the Java client outlines this block with; None when Horizon's own facts apply.
    pub(super) fn java_outline(&self, pos: [i32; 3], state: BlockStateId) -> Option<Vec<[f64; 6]>> {
        self.remote.as_ref()?;
        let map = horizon_java_host::bridge::STATES.get()?;
        let at = state.0 as usize;
        if !map.owned.get(at).copied().unwrap_or(false)
            || !map.entity.get(at).copied().unwrap_or(false)
        {
            return None;
        }
        let host = horizon_java_host::get()?;
        let generation = java_shape_generation();
        // While the Java client ticks in the background, the last answer stands rather than the window waiting.
        let busy = java_client_running();
        let cached = CACHE.with_borrow(|c| {
            c.get(&pos)
                .filter(|(s, g, _)| *s == state && (*g == generation || busy))
                .map(|(_, _, boxes)| boxes.clone())
        });
        if cached.is_some() || busy {
            return cached;
        }
        let mut out = Out::default();
        out.long(BlockPos::new(pos[0], pos[1], pos[2]).pack());
        let reply = host.request("client_shape", &out.0)?;
        let mut reply = In(&reply);
        reply.byte()?;
        let mut boxes = Vec::new();
        for _ in 0..reply.int()? {
            let mut b = [0.0; 6];
            for v in &mut b {
                *v = reply.double()?;
            }
            boxes.push(b);
        }
        CACHE.with_borrow_mut(|c| {
            if c.len() >= CACHE_LIMIT {
                c.clear();
            }
            c.insert(pos, (state, generation, boxes.clone()));
        });
        Some(boxes)
    }

    /// A mod's block's collision shape as Java's `getCollisionShape(level, pos)` answered it for `state`.
    pub(super) fn java_collision_shape(
        &mut self,
        pos: [i32; 3],
        state: Option<BlockStateId>,
        boxes: &[f64],
    ) {
        let shapes = &mut self.world.block_entity_shapes;
        match (state.filter(|s| shaped_by_java(*s)), boxes_shape(boxes)) {
            (Some(state), Some(shape)) => {
                shapes.insert(pos, (state, shape));
            }
            _ => {
                shapes.remove(&pos);
            }
        }
    }

    /// The window's collisions take the Java client's shapes of blocks whose client copy changed since the last take.
    pub(super) fn java_client_collisions(&mut self) {
        let generation = java_shape_generation();
        if self.remote.is_none()
            || java_client_running()
            || COLLISIONS_TAKEN.get() == Some(generation)
        {
            return;
        }
        let Some(host) = horizon_java_host::get() else {
            return;
        };
        let Some(reply) = host.request("client_collisions_take", &[]) else {
            return;
        };
        COLLISIONS_TAKEN.set(Some(generation));
        let map = horizon_java_host::bridge::STATES.get();
        let mut reply = In(&reply);
        let Some(count) = reply.int() else {
            return;
        };
        for _ in 0..count {
            let (Some(pos), Some(java)) = (reply.long(), reply.int()) else {
                return;
            };
            let mut boxes = Vec::new();
            for _ in 0..reply.int().unwrap_or(0) * 6 {
                boxes.push(reply.double().unwrap_or(0.0));
            }
            let state = map.and_then(|m| m.to_horizon.get(java as usize).copied().flatten());
            let pos = BlockPos::unpack(pos);
            self.java_collision_shape([pos.x, pos.y, pos.z], state, &boxes);
        }
    }
}
