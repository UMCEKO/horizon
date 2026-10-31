//! Heavy pure work, where Rust measurably beats the JIT (docs/java-mods.md); a table for `()` needs no game.

use super::*;
use horizon_core::geometry::Vec3;
use horizon_java_host::bridge::{Doubles, Shape};

horizon_java_host::java_bridges! { table for () {
    "net/minecraft/world/phys/shapes/VoxelShape".clip
    "(Lnet/minecraft/world/phys/Vec3;Lnet/minecraft/world/phys/Vec3;Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/phys/BlockHitResult;" =>
    fn voxel_shape_clip(_game, shape: Shape, from: Point, to: Point, pos: Pos) -> Doubles {
        let [fx, fy, fz] = from.0;
        let [tx, ty, tz] = to.0;
        match shape.0.clip(Vec3::new(fx, fy, fz), Vec3::new(tx, ty, tz), pos.0) {
            Some(hit) => Doubles(vec![
                hit.location.x,
                hit.location.y,
                hit.location.z,
                f64::from(hit.direction as u8),
                f64::from(u8::from(hit.inside)),
                f64::from(pos.0.x),
                f64::from(pos.0.y),
                f64::from(pos.0.z),
            ]),
            None => Doubles(Vec::new()),
        }
    }
}}
