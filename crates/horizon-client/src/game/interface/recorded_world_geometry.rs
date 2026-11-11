//! What mods draw in the level's frame (`world_renders`), as the Java client recorded it: split into the frame's native passes.
use super::*;
use horizon_client::world_renders::{self, Outputs, WorldLine};

impl Game<'_> {
    /// The recorded pieces in view: the entity pass's go to `out`, the other passes' stay in `world` for this frame.
    pub(super) fn recorded_world_geometry(
        &self,
        camera: glam::DVec3,
        world: &mut Outputs,
        out: &mut Vec<Vertex>,
    ) {
        world.clear();
        if world_renders::is_empty() {
            return;
        }
        let view = self.view_frustum;
        let light = |p: [i32; 3]| self.world.light(p[0], p[1], p[2]);
        let visible = |p: [i32; 3]| {
            view.is_none_or(|v| {
                let o = (glam::DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
                    - camera)
                    .as_vec3();
                v.intersects(o - glam::Vec3::ONE, o + glam::Vec3::splat(2.0))
            })
        };
        let frame = world_renders::Frame {
            camera,
            sprites: &self.gui.atlas.sprites,
            light: &light,
            visible: &visible,
        };
        world_renders::draw(&frame, world);
        out.append(&mut world.entities);
    }
}

/// The recorded lines, camera-relative for the gizmo passes: depth-tested ones, then those on top.
pub(super) fn recorded_world_lines(
    world: &mut Outputs,
    at: [f64; 3],
    view: glam::Mat4,
) -> (Vec<[f32; 8]>, Vec<[f32; 8]>) {
    let relative = |lines: &[WorldLine]| {
        let lines: Vec<_> = lines
            .iter()
            .map(|l| horizon_client::gizmos::Line {
                start: l.start,
                end: l.end,
                color: l.color,
                width: l.width,
            })
            .collect();
        horizon_client::gizmos::camera_relative(&lines, at, view)
    };
    let lines = (relative(&world.lines), relative(&world.on_top));
    world.lines.clear();
    world.on_top.clear();
    lines
}
