// game-module-cfg: feature java-mods
//! Shapes of mods' blocks that depend on their block entity: the server's collisions from the server's Java block, the window's from the Java client's copy.

use super::java_screens::live::{java_client_running, java_shape_generation};
use super::*;
use horizon_core::geometry::{Aabb, ShapeOp, Vec3, VoxelShape};
use horizon_java_host::wire::{In, Out};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

const CACHE_LIMIT: usize = 4096;

type Boxes = Vec<[f64; 6]>;
type Cached = (BlockStateId, i32, Boxes, Boxes);

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

fn read_boxes(reply: &mut In) -> Option<Boxes> {
    let mut boxes = Vec::new();
    for _ in 0..reply.int()? {
        let mut b = [0.0; 6];
        for v in &mut b {
            *v = reply.double()?;
        }
        boxes.push(b);
    }
    Some(boxes)
}

/// A `client_shape` reply: the boxes, then the edges; an old reply without the edge list has none.
fn parse_shape_reply(reply: &[u8]) -> Option<(Boxes, Boxes)> {
    let mut reply = In(reply);
    reply.byte()?;
    let boxes = read_boxes(&mut reply)?;
    let edges = read_boxes(&mut reply).unwrap_or_default();
    Some((boxes, edges))
}

impl Game<'_> {
    /// The block-local boxes the Java client outlines this block with; None when Horizon's own facts apply.
    pub(super) fn java_outline(&self, pos: [i32; 3], state: BlockStateId) -> Option<Boxes> {
        self.java_outline_shape(pos, state).map(|(boxes, _)| boxes)
    }

    /// The block-local edges vanilla outlines this block with, from Java's shape; None when Horizon's own facts apply.
    pub(super) fn java_outline_edges(&self, pos: [i32; 3], state: BlockStateId) -> Option<Boxes> {
        self.java_outline_shape(pos, state)
            .map(|(_, edges)| edges)
            .filter(|edges| !edges.is_empty())
    }

    fn java_outline_shape(&self, pos: [i32; 3], state: BlockStateId) -> Option<(Boxes, Boxes)> {
        self.remote.as_ref()?;
        if !shaped_by_java(state) {
            return None;
        }
        let host = horizon_java_host::get()?;
        let generation = java_shape_generation();
        // While the Java client ticks in the background, the last answer stands rather than the window waiting.
        let busy = java_client_running();
        let cached = CACHE.with_borrow(|c| {
            c.get(&pos)
                .filter(|(s, g, ..)| *s == state && (*g == generation || busy))
                .map(|(_, _, boxes, edges)| (boxes.clone(), edges.clone()))
        });
        if cached.is_some() || busy {
            return cached;
        }
        let mut out = Out::default();
        out.long(BlockPos::new(pos[0], pos[1], pos[2]).pack());
        let reply = host.request("client_shape", &out.0)?;
        let (boxes, edges) = parse_shape_reply(&reply)?;
        CACHE.with_borrow_mut(|c| {
            if c.len() >= CACHE_LIMIT {
                c.clear();
            }
            c.insert(pos, (state, generation, boxes.clone(), edges.clone()));
        });
        Some((boxes, edges))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(boxes: &[[f64; 6]], edges: Option<&[[f64; 6]]>) -> Vec<u8> {
        let mut out = Out::default();
        out.0.push(1);
        for list in std::iter::once(boxes).chain(edges) {
            out.int(list.len() as i32);
            for v in list.iter().flatten() {
                out.double(*v);
            }
        }
        out.0
    }

    #[test]
    fn reply_carries_boxes_and_edges() {
        let slab = [0.0, 0.0, 0.0, 1.0, 0.5, 1.0];
        let edges: Vec<[f64; 6]> = (0..12)
            .map(|i| [f64::from(i), 0.0, 0.0, f64::from(i), 1.0, 0.0])
            .collect();
        let (b, e) = parse_shape_reply(&reply(&[slab], Some(&edges))).unwrap();
        assert_eq!(b, vec![slab]);
        assert_eq!(e, edges);
    }

    #[test]
    fn reply_without_edge_list_has_no_edges() {
        let slab = [0.0, 0.0, 0.0, 1.0, 0.5, 1.0];
        let (b, e) = parse_shape_reply(&reply(&[slab], None)).unwrap();
        assert_eq!(b, vec![slab]);
        assert!(e.is_empty());
    }
}
