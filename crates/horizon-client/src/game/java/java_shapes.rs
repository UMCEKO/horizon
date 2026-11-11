// game-module-cfg: feature java-mods
//! Shapes of mods' blocks that depend on their block entity: the server's collisions from the server's Java block, the window's from the Java client's copy.
#![allow(dead_code)]

use super::*;
use horizon_core::geometry::{Aabb, ShapeOp, Vec3, VoxelShape};

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
}
