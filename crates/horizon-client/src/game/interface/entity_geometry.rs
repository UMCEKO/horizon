//! The frame's entity and block-entity geometry.
use super::*;

impl Game<'_> {
    /// ItemEntityRenderer: items bob and spin; blocks render as small cubes.
    pub(super) fn entity_geometry(
        &mut self,
        camera: glam::DVec3,
        alpha: f64,
        billboard_frustum: Option<horizon_client::gfx::Frustum>,
        mut out: Vec<Vertex>,
    ) -> (
        Vec<Vertex>,
        Arc<[StaticSectionSnapshot]>,
        Vec<[i32; 3]>,
        StaticCacheFrameStats,
    ) {
        // Its steps as spans (HORIZON_SPIKES): each lap names the step that just ended.
        let mut laps = horizon_client::spans::laps();
        let mut static_block_entities = std::sync::Arc::from([]);
        let mut visible_static_block_entity_sections = Vec::new();
        let mut static_block_entity_stats = StaticCacheFrameStats::default();
        self.entity_spans.borrow_mut().clear();
        self.entity_motion.borrow_mut().clear();
        self.entity_motion_posed.borrow_mut().clear();
        self.maps.slots.next_frame();
        // LevelExtractor.extract: the transient blocks queued since the last frame.
        self.drain_transient_block_queue();
        self.prepare_entity_culling();
        self.prepare_occlusion_culling(camera);
        self.prepare_trims();
        self.begin_skinning();
        self.mark_entity(out.len(), "minecraft:entity_flame");
        // One scan for both passes that draw them (block entities below and the
        // overlay near the end of this function).
        let cracks = self.cracks(camera);
        let fire_yaw = if self.camera_type == horizon_client::camera::CameraType::ThirdPersonFront {
            self.yaw + 180.0
        } else {
            self.yaw
        };
        // Packs may move vertices or draw from another camera. The guarded
        // billboard frustum covers TAA's jitter without changing model culling.
        let fire_view = billboard_frustum
            .as_ref()
            .filter(|_| self.pack_ids.is_none() && self.native_shaders.picked.is_empty());
        let fire_visible = |position, width, height| {
            fire_view.is_none_or(|view| {
                horizon_client::fire_render::in_view(view, camera, position, width, height)
            })
        };
        laps.lap("eg_prep");
        for m in &self.mobs {
            // Blaze.isOnFire: a charged blaze burns.
            let blazing = m.species() == horizon_client::mobs::Species::Blaze && m.extra.flag;
            if (m.is_on_fire() || blazing) && !m.removed {
                let at = m.previous.lerp(m.position, alpha);
                if !fire_visible([at.x, at.y, at.z], m.width(), m.height()) {
                    continue;
                }
                horizon_client::fire_render::flames(
                    [at.x, at.y, at.z],
                    m.width(),
                    m.height(),
                    camera,
                    fire_yaw,
                    &self.gui.atlas.sprites,
                    self.world.light(
                        at.x.floor() as i32,
                        at.y.floor() as i32,
                        at.z.floor() as i32,
                    ),
                    &mut out,
                );
            }
        }
        if self.own_body_in_view()
            && let Some(p) = &self.player
            && p.fire_ticks > 0
            && !p.spectator
        {
            let feet = self
                .previous_position
                .lerp(p.position, self.clock.camera_fraction());
            if fire_visible(
                [feet.x, feet.y, feet.z],
                p.dimensions.width,
                p.dimensions.height,
            ) {
                horizon_client::fire_render::flames(
                    [feet.x, feet.y, feet.z],
                    p.dimensions.width,
                    p.dimensions.height,
                    camera,
                    fire_yaw,
                    &self.gui.atlas.sprites,
                    self.world.light(
                        feet.x.floor() as i32,
                        feet.y.floor() as i32,
                        feet.z.floor() as i32,
                    ),
                    &mut out,
                );
            }
        }
        laps.lap("eg_fire_mobs");
        for item in &self.dropped {
            if item.fire_ticks > 0
                && !self
                    .res
                    .items()
                    .resists_fire(&item.stack, "minecraft:in_fire")
            {
                let at = item.previous.lerp(item.position, alpha);
                if !fire_visible([at.x, at.y, at.z], 0.25, 0.25) {
                    continue;
                }
                horizon_client::fire_render::flames(
                    [at.x, at.y, at.z],
                    0.25,
                    0.25,
                    camera,
                    fire_yaw,
                    &self.gui.atlas.sprites,
                    self.world.light(
                        at.x.floor() as i32,
                        at.y.floor() as i32,
                        at.z.floor() as i32,
                    ),
                    &mut out,
                );
            }
        }
        laps.lap("eg_fire_items");
        for stand in &self.armor_stands {
            if stand.fire_ticks > 0 && !stand.marker {
                let scale = if stand.small { 0.5 } else { 1.0 };
                let at = stand.render_position(alpha);
                if !fire_visible([at.x, at.y, at.z], 0.5 * scale, 1.975 * scale) {
                    continue;
                }
                horizon_client::fire_render::flames(
                    [at.x, at.y, at.z],
                    0.5 * scale,
                    1.975 * scale,
                    camera,
                    fire_yaw,
                    &self.gui.atlas.sprites,
                    self.world.light(
                        at.x.floor() as i32,
                        at.y.floor() as i32,
                        at.z.floor() as i32,
                    ),
                    &mut out,
                );
            }
        }
        laps.lap("eg_fire_stands");
        // `ItemEntityRenderer`.
        self.mark_entity(out.len(), "minecraft:item");
        self.dropped_item_geometry(camera, alpha, &mut out);
        self.mark_motion(out.len(), 0);
        // `VaultRenderer`.
        self.mark_entity(out.len(), "");
        self.vault_geometry(camera, alpha, &mut out);
        // `CampfireRenderer`.
        self.mark_entity(out.len(), "");
        self.campfire_geometry(camera, &mut out);
        self.brushable_geometry(camera, &mut out);
        self.shelf_geometry(camera, &mut out);
        self.mark_entity(out.len(), "minecraft:ominous_item_spawner");
        self.ominous_item_spawner_geometry(camera, alpha, &mut out);
        laps.lap("eg_items_misc");
        // Mobs and their projectiles (a span each).
        self.mob_geometry(camera, alpha, &mut out);
        self.mark_motion(out.len(), 0);
        laps.lap("eg_mobs");
        // In first person only a shader pack's shadow pass draws the player.
        self.mark_player(out.len(), !self.own_body_in_view());
        let player_start = out.len();
        self.mark_motion(player_start, Self::MOTION_OWN_PLAYER);
        let player_alpha = self.own_player_alpha();
        self.player_model_geometry(camera, player_alpha, &mut out);
        // A native pack's shadow pass alone gets the first-person model (no per-entity draws).
        if self.pack_ids.is_none() && !self.own_body_in_view() {
            self.shadow_only_vertices
                .borrow_mut()
                .extend(out.drain(player_start..));
        }
        self.mark_motion(out.len(), 0);
        // What crates draw around players: first person, see-through (game/player_hooks.rs).
        self.players_after_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "minecraft:evoker_fangs");
        self.fangs_geometry(camera, alpha, &mut out);
        self.block_item_display_geometry(camera, alpha, &mut out);
        laps.lap("eg_players");
        let mut see_through = std::mem::take(&mut self.see_through_vertices);
        see_through.clear();
        self.name_tag_geometry(camera, alpha, &mut out, &mut see_through);
        self.test_instance_marker_geometry(camera, &mut out, &mut see_through);
        self.debug_renderer_geometry(camera, &mut out);
        self.text_display_geometry(camera, alpha, &mut out, &mut see_through);
        self.see_through_vertices = see_through;
        laps.lap("eg_name_tags");
        // Entity shadows (EntityRenderer.extractShadow); a native pack's sun shadows replace them.
        self.mark_translucent_entity(out.len(), "minecraft:entity_shadow");
        let pack_shadows = self.renderer.as_ref().is_some_and(|r| {
            r.pack_entity_shadows() || (self.dim == Dim::Overworld && r.native_shadow())
        });
        let shadows = self.settings.entity_shadows && self.pack.polytone_colors.entity_shadows();
        let shadow_sprite = (shadows && !pack_shadows)
            .then(|| self.gui.atlas.sprites.get("misc/shadow").copied())
            .flatten();
        if let Some(sprite) = shadow_sprite {
            // DimensionType.ambientLight: the_nether 0.1, the_end 0.25.
            let ambient = match self.dim {
                Dim::Nether => 0.1,
                Dim::End => 0.25,
                Dim::Overworld | Dim::Extra(_) => 0.0,
            };
            let darken = self.sky_darken();
            let a = alpha;
            let lerp = |p: Vec3, q: Vec3| {
                glam::DVec3::new(
                    p.x + (q.x - p.x) * a,
                    p.y + (q.y - p.y) * a,
                    p.z + (q.z - p.z) * a,
                )
            };
            let mut casters: Vec<(glam::DVec3, f32, f32)> =
                horizon_client::shadows::mob_casters(&self.mobs, a).collect();
            let item = horizon_client::shadows::shadow_params("minecraft:item");
            casters.extend(
                self.dropped
                    .iter()
                    .map(|i| (lerp(i.previous, i.position), item.0, item.1)),
            );
            let orb = horizon_client::shadows::shadow_params("minecraft:experience_orb");
            casters.extend(
                self.orbs
                    .iter()
                    .map(|o| (lerp(o.previous, o.position), orb.0, orb.1)),
            );
            // Text, block and item displays (DisplayRenderer's shadow radius).
            casters.extend(self.text_displays.values().filter_map(|d| d.shadow(a)));
            // Packs may move vertices or draw from another view.
            let shadow_view = self
                .view_frustum
                .as_ref()
                .filter(|_| self.pack_ids.is_none() && self.native_shaders.picked.is_empty());
            for (pos, radius, strength) in casters {
                if let Some(view) = shadow_view
                    && !horizon_client::shadows::in_view(view, camera, pos, radius, strength)
                {
                    continue;
                }
                horizon_client::shadows::shadow(
                    &self.world,
                    self.res.facts(),
                    &|state| {
                        self.res
                            .collision()
                            .cached_shapes()
                            .cached_shape(state)
                            .ok()
                            .flatten()
                            .and_then(|shape| shape.bounds())
                    },
                    sprite,
                    camera,
                    pos,
                    radius,
                    strength,
                    ambient,
                    darken,
                    &mut out,
                );
            }
        }
        laps.lap("eg_shadows");
        // Block entities (BlockEntityRenderers): chests, shulker boxes, banners, skulls...
        if let Some(r) = &self.renderer {
            // Block entities outside the view frustum are skipped (with a
            // block of margin for models larger than their block).
            let view = self.view_frustum;
            // Beams and structure outlines reach far outside their block
            // (BlockEntityRenderer.shouldRenderOffScreen): never culled.
            let off_screen = |state: u32| {
                let name = &*self.res.facts().get(BlockStateId(state)).name;
                matches!(
                    name,
                    "minecraft:beacon"
                        | "minecraft:end_gateway"
                        | "minecraft:structure_block"
                        | "minecraft:test_instance_block"
                )
            };
            let long_range = |state: u32| {
                &*self.res.facts().get(BlockStateId(state)).name == "minecraft:end_gateway"
            };
            let in_view = |p: &[i32; 3]| {
                view.is_none_or(|v| {
                    let o = (glam::DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
                        - camera)
                        .as_vec3();
                    v.intersects(o - glam::Vec3::ONE, o + glam::Vec3::splat(2.0))
                })
            };
            let camera_list: Vec<&([i32; 3], u32)> = r
                .visible_block_entities_in_frustum(
                    self.settings.chunk_fade,
                    camera,
                    view,
                    &off_screen,
                    long_range,
                )
                .filter(|(block, section_inside)| {
                    let (pos, state) = **block;
                    self.res.block_entity_render().renders(state)
                        && (off_screen(state) || *section_inside || in_view(&pos))
                })
                .map(|(block, _)| block)
                .collect();
            // A shader pack's shadow pass draws the block entities of every section it draws,
            // in the view or not (`Renderer::shadow_pass_block_entities`, #8464): those the
            // view leaves out go in hidden from the camera.
            let shadow_only: Vec<&([i32; 3], u32)> =
                if self.pack_ids.is_some() && self.pack_shadow_casters.0 {
                    let listed: HashSet<[i32; 3]> = camera_list.iter().map(|b| b.0).collect();
                    r.shadow_pass_block_entities(camera)
                        .filter(|(pos, state)| {
                            !listed.contains(pos) && self.res.block_entity_render().renders(*state)
                        })
                        .collect()
                } else {
                    Vec::new()
                };
            let shadow_only_at: HashSet<[i32; 3]> = shadow_only.iter().map(|b| b.0).collect();
            let dynamic_visible = |pos: [i32; 3], state: u32, lo: glam::DVec3, hi: glam::DVec3| {
                if off_screen(state) || shadow_only_at.contains(&pos) {
                    return true;
                }
                if let Some(view) = view
                    && !view.intersects(
                        (lo - camera - glam::DVec3::splat(0.5)).as_vec3(),
                        (hi - camera + glam::DVec3::splat(0.5)).as_vec3(),
                    )
                {
                    return false;
                }
                let name = &*self.res.facts().get(BlockStateId(state)).name;
                if self.settings.horizon.entity_culling == EntityCullingMode::Off
                    || self.pack_shadow_casters.0
                    || name == "minecraft:end_portal"
                    || name == "minecraft:end_gateway"
                    || hi.y - lo.y > 2.0
                {
                    return true;
                }
                // Culler::aabb_bounds adds 0.05 on each side. Keep its final
                // query just inside the conservative rendered-model envelope.
                let bounds = Aabb::new(
                    Vec3::new(lo.x + 0.06, lo.y + 0.06, lo.z + 0.06),
                    Vec3::new(hi.x - 0.06, hi.y - 0.06, hi.z - 0.06),
                );
                !self.block_entity_occluded(bounds)
            };
            let lid = |p: [i32; 3]| self.chest_lid(p, alpha as f32);
            let anim = |p: [i32; 3]| self.block_anim(p, alpha as f32);
            let scene = BlockEntityScene {
                camera,
                orientation: self.camera_orientation(),
                anim: &anim,
                texts: None,
                partial: alpha as f32,
                game_time: self.world.game_time,
                max_y: self.world.shape.max_y() - 1,
                world: &self.world,
                sprites: &self.gui.atlas.sprites,
                emissive: &self.gui.atlas.emissive,
                translucent_sprites: &self.gui.atlas.translucent,
                atlas_revision: self.gui.atlas.revision,
                nether: self.dim == Dim::Nether,
                lid: &lid,
                sign: &|p| match self.world.block_entities.get(&(p[0], p[1], p[2])) {
                    Some(BlockEntity::Sign(sign)) => Some(sign),
                    _ => None,
                },
                banner: &|p| self.banner_layers_at(p),
                skull: &|p| self.skull_skin_at(p),
                glyph: &|c| self.gui.atlas.glyph_cell(c),
                glyphs: self.gui.atlas.font.epoch(),
                font: Some(&self.gui.atlas.font),
                dynamic_visible: Some(&dynamic_visible),
            };
            let visible = camera_list.into_iter().chain(shadow_only.iter().copied());
            let block_entity_cache = CacheOptions {
                shulker_boxes: self.settings.horizon.cache_shulker_boxes,
                signs: self.settings.horizon.cache_signs,
                decorated_pots: self.settings.horizon.cache_decorated_pots,
                bells: self.settings.horizon.cache_bells,
                chests: self.settings.horizon.cache_chests,
                banners: self.settings.horizon.cache_banners,
                copper_golem_statues: self.settings.horizon.cache_copper_golem_statues,
            };
            // Entity occlusion is for ordinary entity meshes. Block entities
            // already have section and per-block frustum culling; routing them
            // through the per-vertex occlusion callback both repeats that work
            // and disables their per-section GPU cache. Shader packs still need
            // per-entity span marks, so they keep the callback path.
            if self.pack_ids.is_some() {
                let section_revision = |pos| r.block_entity_section_revision(pos);
                static_block_entity_stats = self
                    .res
                    .block_entity_render()
                    .render_with_cache_options_and_callbacks(
                        &scene,
                        visible,
                        block_entity_cache,
                        r.block_entity_revision(),
                        Some(&section_revision),
                        &mut out,
                        |pos, state, start| {
                            self.mark_block_entity(start, state);
                            self.mark_motion(start, motion_block_entity(pos));
                        },
                        |pos, _, start, out| {
                            if shadow_only_at.contains(&pos) {
                                self.hide_spans_from(start, out.len());
                            }
                        },
                    );
            } else {
                let cache_candidates = r
                    .visible_block_entities_in_frustum(
                        self.settings.chunk_fade,
                        camera,
                        None,
                        |_| false,
                        |_| false,
                    )
                    .filter(|(block, _)| self.res.block_entity_render().renders(block.1))
                    .map(|(block, _)| block);
                (
                    static_block_entities,
                    visible_static_block_entity_sections,
                    static_block_entity_stats,
                ) = self
                    .res
                    .block_entity_render()
                    .render_with_gpu_cache_options(
                        &scene,
                        visible,
                        cache_candidates,
                        block_entity_cache,
                        r.block_entity_revision(),
                        |pos| r.block_entity_section_revision(pos),
                        &mut out,
                    );
            }
            // Mod block entities: their mods draw them (`native_block_entity_submit`),
            // culled by the bounds they give. A world without any (`has_mods`) has none
            // to find among its other unrendered block entities (furnaces, hoppers...).
            let mods = self
                .world
                .block_entities
                .has_mods()
                .then(|| {
                    r.visible_block_entities(self.settings.chunk_fade)
                        .filter(|(_, state)| !self.res.block_entity_render().renders(*state))
                })
                .into_iter()
                .flatten();
            let untemplated = self.untemplated_mod_block_entities(camera);
            self.mod_block_entity_geometry(
                mods.chain(untemplated.iter()),
                camera,
                alpha as f32,
                &mut out,
            );
            self.recorded_block_entity_geometry(camera, alpha as f32, &mut out);
            self.mark_stage(out.len(), "DESTROY");
            for &(pos, stage) in &cracks {
                let state = self.world.block(pos[0], pos[1], pos[2]);
                self.res
                    .block_entity_render()
                    .render_breaking(&scene, pos, state.0, stage, &mut out);
            }
        }
        laps.lap("eg_block_entities");
        self.mark_entity(out.len(), "minecraft:experience_orb");
        let orb_view = billboard_frustum
            .as_ref()
            .filter(|_| self.pack_ids.is_none() && self.native_shaders.picked.is_empty());
        let live = self.orbs.iter().map(|o| {
            let at = [
                o.previous.x + (o.position.x - o.previous.x) * alpha,
                o.previous.y + (o.position.y - o.previous.y) * alpha,
                o.previous.z + (o.position.z - o.previous.z) * alpha,
            ];
            (o, at, alpha as f32)
        });
        self.orb_geometry(camera, live, orb_view, &mut out);
        laps.lap("eg_orbs");
        // Falling blocks use their baked model, including non-cubic shapes.
        self.mark_entity(out.len(), "minecraft:falling_block");
        // Packs can move vertices or draw another view; otherwise only depth
        // planes are used, which TAA's X/Y projection jitter cannot change.
        let falling_view = self
            .view_frustum
            .as_ref()
            .filter(|_| self.pack_ids.is_none() && self.native_shaders.picked.is_empty());
        let tables = &self.pack.context.tables;
        let falling_forward = self.camera_orientation() * -glam::Vec3::Z;
        for f in &self.falling {
            let p = f.model_origin(alpha, |state, x, z| tables.offset(state, x, z)) - camera;
            let o = p.as_vec3();
            // Only rear-origin models pay for the envelope/depth check. The
            // full posed-camera test still decides rejection, including models
            // whose origin is behind but whose geometry reaches into view.
            if falling_forward.dot(o) < 0.0
                && falling_view.is_some_and(|view| {
                    self.pack
                        .render
                        .model_catalog
                        .render_bounds(f.state)
                        .is_some_and(|[lo, hi]| {
                            let lo = glam::Vec3::from_array(lo) + o;
                            let hi = glam::Vec3::from_array(hi) + o;
                            let magnitude = o
                                .abs()
                                .max_element()
                                .max(lo.abs().max_element())
                                .max(hi.abs().max_element());
                            let padding =
                                glam::Vec3::splat(1.0e-4 + magnitude * f32::EPSILON * 4.0);
                            !view.intersects_depth(lo - padding, hi + padding)
                        })
                })
            {
                continue;
            }
            // `FallingBlockEntity.blockPosition()`: the cell its feet are in.
            let (x, y, z) = (
                f.position.x.floor() as i32,
                f.position.y.floor() as i32,
                f.position.z.floor() as i32,
            );
            if !falling_block_visible(f.state, self.world.block(x, y, z)) {
                continue;
            }
            let Some((_, tint)) = self.falling_block_face_sprite(f.state) else {
                continue;
            };
            let light = self.world.light(x, y + 1, z);
            let packed = (u32::from(light >> 4) * 16) | ((u32::from(light & 15) * 16) << 8);
            let start = out.len();
            self.mark_motion(start, Self::MOTION_OBJECT | u64::from(f.id));
            self.block_model_vertices(
                f.state,
                horizon_core::random::position_seed(f.start_pos),
                glam::Mat4::from_translation(o),
                packed,
                false,
                &mut out,
            );
            let tint = horizon_client::mobs::render::abgr(tint);
            for vertex in &mut out[start..] {
                vertex.color = horizon_client::hud_layout::multiply(vertex.color, tint);
            }
        }
        self.mark_motion(out.len(), 0);
        self.mark_entity(out.len(), "");
        self.moving_piston_geometry(camera, alpha, &mut out);
        // LevelRenderer.submitTransientBlocks.
        self.transient_block_geometry(camera, &mut out);
        self.mark_entity(out.len(), "minecraft:tnt");
        self.tnt_geometry(camera, alpha, &mut out);
        self.mark_motion(out.len(), 0);
        self.mark_entity(out.len(), "minecraft:painting");
        self.painting_geometry(camera, &mut out);
        self.mark_motion(out.len(), 0);
        self.mark_entity(out.len(), "minecraft:item_frame");
        self.frame_geometry(camera, &mut out);
        self.mark_motion(out.len(), 0);
        self.mark_entity(out.len(), "minecraft:armor_stand");
        self.armor_stand_geometry(camera, alpha, &mut out);
        self.mark_motion(out.len(), 0);
        self.cushion_geometry(camera, &mut out);
        self.mark_entity(out.len(), "minecraft:oak_boat");
        self.boat_geometry(camera, alpha, &mut out);
        self.mark_motion(out.len(), 0);
        self.mark_entity(out.len(), "minecraft:minecart");
        self.cart_geometry(camera, alpha, &mut out);
        self.mark_motion(out.len(), 0);
        // Beams: `gbuffers_beaconbeam`, not a block entity's program (5.4 has no stage for them).
        self.mark_stage(out.len(), "NONE");
        self.beacon_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "minecraft:fishing_bobber");
        self.fishing_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "minecraft:leash_knot");
        self.leash_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "minecraft:trident");
        self.trident_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "minecraft:firework_rocket");
        self.rocket_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "minecraft:splash_potion");
        self.thrown_potion_geometry(camera, alpha, &mut out);
        laps.lap("eg_objects");
        // Breaking progress overlays (destroy_stage_0..9): ours and the other players'.
        self.mark_stage(out.len(), "DESTROY");
        for &(pos, stage) in &cracks {
            let o = (glam::DVec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]))
                - camera)
                .as_vec3();
            let state = self.world.block(pos[0], pos[1], pos[2]);
            if self.res.facts().get(state).render_shape
                == horizon_client::blocks::RenderShape::Model
                && let Ok(models) = self
                    .pack
                    .render
                    .model_catalog
                    .collect(state, BlockPos::new(pos[0], pos[1], pos[2]))
            {
                let offset = self
                    .pack
                    .context
                    .tables
                    .offset(state, pos[0], pos[2])
                    .unwrap_or([0.0; 3]);
                let origin = o + glam::DVec3::from(offset).as_vec3();
                let translucent = models
                    .iter()
                    .any(|(_, model)| model.faces.iter().any(|f| f.translucent));
                for (_, model) in models {
                    horizon_client::breaking::geometry(model, origin, translucent, stage, &mut out);
                }
            }
        }
        laps.lap("eg_cracks");
        self.finish_async_occlusion_culling();
        laps.lap("eg_culling_finish");
        (
            out,
            static_block_entities,
            visible_static_block_entity_sections,
            static_block_entity_stats,
        )
    }
}
