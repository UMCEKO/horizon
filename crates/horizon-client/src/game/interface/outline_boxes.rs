//! A block's outline boxes in world space, as the hit outline and picks walk them.
use super::*;

impl Game<'_> {
    /// The same outline traversal without allocating a box list.
    pub(super) fn outline_boxes_iter(
        &self,
        world: &World,
        anims: &horizon_client::block_entity_anim::BlockEntityAnims,
        pos: [i32; 3],
        player_context: bool,
    ) -> impl Iterator<Item = [f64; 6]> + '_ {
        let state = world.block(pos[0], pos[1], pos[2]);
        let declared = self.collision.cached_shapes().context_outline_shape(
            state,
            if player_context {
                self.outline_context_kind()
            } else {
                horizon_world::collision_context::EntityContextKind::Other
            },
        );
        let context = declared
            .is_none()
            .then(|| self.context_outline(pos, state, player_context, anims))
            .flatten();
        #[cfg(feature = "java-mods")]
        let java = (context.is_none() && declared.is_none())
            .then(|| self.java_outline(pos, state))
            .flatten();
        #[cfg(not(feature = "java-mods"))]
        let java: Option<Vec<[f64; 6]>> = None;
        let f = self.res.facts().get(state);
        let o = perspective::outline_offset(
            &self.pack.context.tables,
            state,
            pos[0],
            pos[2],
            f.outline_offset,
        );
        let base = [
            f64::from(pos[0]) + o[0],
            f64::from(pos[1]) + o[1],
            f64::from(pos[2]) + o[2],
        ];
        let boxes = if context.is_some() || declared.is_some() || java.is_some() {
            &[][..]
        } else {
            &f.outline[..]
        };
        context
            .map(|b| std::array::from_fn(|i| f64::from(pos[i % 3]) + b[i]))
            .into_iter()
            .chain(
                declared
                    .into_iter()
                    .flat_map(|shape| shape.boxes())
                    .map(move |b| {
                        let min = b.min_position();
                        let max = b.max_position();
                        let coords = [min.x, min.y, min.z, max.x, max.y, max.z];
                        std::array::from_fn(|i| base[i % 3] + coords[i])
                    }),
            )
            .chain(
                java.into_iter()
                    .flatten()
                    .map(move |b| std::array::from_fn(|i| f64::from(pos[i % 3]) + b[i])),
            )
            .chain(
                boxes
                    .iter()
                    .map(move |b| std::array::from_fn(|i| base[i % 3] + b[i])),
            )
    }
}
