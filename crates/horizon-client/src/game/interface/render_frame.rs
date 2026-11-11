//! The frame built for the renderer: world, entities, sky, hand and screen.
use super::*;

impl Game<'_> {
    /// Build the complete frame before handing its borrowed geometry and lighting to
    /// the consumer. Tests use the same boundary as GPU submission.
    pub(super) fn render_with(
        &mut self,
        consume: impl FnOnce(&mut Renderer, &Frame<'_>) -> Result<bool>,
    ) -> Result<bool> {
        self.prepared_model_draws.borrow_mut().clear();
        let prepare = spans::guard("prepare");
        let mut pr = spans::laps();
        // The window's GPU holds the atlas: its copy in memory is kept
        // compressed (a no-op once it is).
        if self.renderer.is_some() {
            self.gui.atlas.pack_pixels();
            self.pack.render.pixels.pack();
        }
        // Before any text of this frame: a full glyph region starts over.
        self.gui.atlas.font.begin_frame();
        if self.clock.still() {
            // Obfuscated text picks the same glyphs in a frame of the same world tick.
            self.gui.atlas.font.reseed(self.visual_ticks() as i64);
        }
        self.glint_frame = self.capture_glint_frame();
        if self.renderer.as_mut().is_some_and(|r| r.display_changed()) {
            self.apply_ui_settings();
        }
        self.follow_frame();
        self.observe_screen();
        // `Gui.setScreen` clears the focus of the screen it leaves: a menu
        // that comes back starts with no widget highlighted.
        self.drop_left_screen_focus();
        // Minecraft.doWorldLoad: the LevelLoadingScreen while single player's
        // server starts, with the server's chunk map and progress.
        if let Some(starting) = &self.integrated_starting {
            // WorldOpenFlows.upgradeAndOpenWorld: the server's file fixers run on
            // FileFixerProgressScreen, before the LevelLoadingScreen has progress.
            let upgrade = starting.handle.upgrade();
            if upgrade.is_file_fixing() {
                horizon_client::widgets::begin_screen(|| {
                    horizon_client::widgets::ScreenInfo::new(
                        "file_fixer",
                        screen_info::tr("upgradeWorld.title", &[]),
                    )
                });
                if let Some(renderer) = &mut self.renderer {
                    let mut hud = HudBatch::new(
                        [renderer.config.width, renderer.config.height],
                        self.settings.gui_scale_inputs(),
                    );
                    horizon_client::level_loading::draw_file_fixer(
                        &mut hud,
                        &self.gui.atlas,
                        upgrade,
                        self.cursor,
                    );
                    pr.lap("pr_loading");
                    drop(prepare);
                    return renderer.render(&loading_frame(&hud));
                }
                return Ok(false);
            }
            // The server's progress once it has some (`hasProgress`).
            let progress = Some(starting.handle.load_view())
                .filter(|v| v.has_progress())
                .map(|v| v.server_progress());
            horizon_client::widgets::begin_screen(|| {
                screen_info::level_loading(progress, &screen_info::tr)
            });
            if let Some(renderer) = &mut self.renderer {
                let mut hud = HudBatch::new(
                    [renderer.config.width, renderer.config.height],
                    self.settings.gui_scale_inputs(),
                );
                let (w, h) = (hud.width, hud.height);
                horizon_client::ui::menu_background(
                    &mut hud,
                    &self.gui.atlas,
                    0.0,
                    0.0,
                    w,
                    h,
                    false,
                );
                // The chunk map and progress are the server thread's.
                let view = self
                    .integrated_starting
                    .as_ref()
                    .map(|s| s.handle.load_view().clone());
                let status = |x: i32, z: i32| view.as_ref().and_then(|v| v.status(x, z));
                let progress = view
                    .as_ref()
                    .is_some_and(|v| v.has_progress())
                    .then_some(self.loading_progress);
                horizon_client::level_loading::draw(
                    &mut hud,
                    &self.gui.atlas,
                    Some(&status),
                    progress,
                );
                pr.lap("pr_loading");
                drop(prepare);
                return renderer.render(&loading_frame(&hud));
            }
            return Ok(false);
        }
        if self.pending_entry.is_some() {
            if let (Some(renderer), Some(loader)) = (&mut self.renderer, &self.rest_loader) {
                horizon_client::widgets::begin_screen(|| {
                    screen_info::level_loading(Some(loader.progress.fraction()), &screen_info::tr)
                });
                let mut hud = HudBatch::new(
                    [renderer.config.width, renderer.config.height],
                    self.settings.gui_scale_inputs(),
                );
                horizon_client::level_loading::draw_progress(
                    &mut hud,
                    &self.gui.atlas,
                    loader.progress.line(),
                    loader.progress.fraction(),
                );
                pr.lap("pr_loading");
                drop(prepare);
                return renderer.render(&loading_frame(&hud));
            }
            return Ok(false);
        }
        // A pack reload covers the frame with its progress
        // (`LoadingOverlay`) until `drive_pack_reload` applies it.
        if let Some(reload) = &self.pack_reload {
            if let Some(renderer) = &mut self.renderer {
                let mut hud = HudBatch::new(
                    [renderer.config.width, renderer.config.height],
                    self.settings.gui_scale_inputs(),
                );
                horizon_client::level_loading::draw_loading_overlay(
                    &mut hud,
                    &self.gui.atlas,
                    reload.progress.line(),
                    reload.progress.fraction(),
                    self.settings.file.bool("darkMojangStudiosBackground"),
                );
                pr.lap("pr_loading");
                drop(prepare);
                return renderer.render(&loading_frame(&hud));
            }
            return Ok(false);
        }
        let Some(size) = self
            .renderer
            .as_ref()
            .map(|r| [r.config.width, r.config.height])
        else {
            return Ok(false);
        };
        pr.lap("pr_setup");
        let alpha = self.clock.fraction();
        // Camera.getCameraEntityPartialTicks: the view, the hand and the
        // player's own model stay smooth while the level is frozen.
        let camera_alpha = self.clock.camera_fraction();
        self.apply_requested_camera_type();
        let vanilla = self.camera_view(camera_alpha as f32);
        let view = vanilla.map(|view| {
            if self.shows_panorama() || self.studio.flying() {
                view
            } else {
                self.native_camera_pose_now(camera_alpha as f32, view)
            }
        });
        // What faces the camera (particles, item sprites, block entity text) faces a mod's.
        // A spectated entity's view is not the local player's yaw and pitch
        // (`Camera.setup` reads the camera entity), so it is kept too.
        let spectating = self.camera_mob().is_some();
        self.posed_rotation = view
            .filter(|posed| spectating || Some(*posed) != vanilla)
            .map(|(_, yaw, pitch)| (yaw, pitch));
        let eye = view.map(|_| {
            (
                glam::DVec3::from_array(self.view.eye),
                self.view.eye_rotation,
            )
        });
        let (camera, underwater, view_yaw, view_pitch) = match view {
            Some((c, yaw, pitch)) => {
                let s =
                    self.world
                        .block(c.x.floor() as i32, c.y.floor() as i32, c.z.floor() as i32);
                (c, self.res.facts().get(s).fluid == Fluid::Water, yaw, pitch)
            }
            None => (
                glam::DVec3::new(0.5, 100.0, 0.5),
                false,
                self.yaw,
                self.pitch,
            ),
        };
        pr.lap("pr_camera");
        self.audio
            .set_listener([camera.x, camera.y, camera.z], view_yaw, view_pitch);
        pr.lap("pr_listener");
        let block = [
            camera.x.floor() as i32,
            camera.y.floor() as i32,
            camera.z.floor() as i32,
        ];
        // The camera inside an opaque block sees nothing through it: disable.
        let cave_culling = self.settings.horizon.cave_culling
            && !self
                .res
                .facts()
                .get(self.world.block(block[0], block[1], block[2]))
                .solid_render;
        // The cave culling search runs on its worker while the frame is prepared.
        if cave_culling
            && self.title.is_none()
            && let Some(renderer) = self.renderer.as_mut()
        {
            horizon_client::spans::time("occlusion", || {
                renderer.start_occlusion(camera, (self.streaming.radius * 16) as f32)
            });
        }
        pr.lap("pr_occlusion");
        let biome = self
            .world
            .chunks
            .get(&(block[0] >> 4, block[2] >> 4))
            .map(|c| c.biome(block[0], block[1], block[2]))
            .unwrap_or(0);
        let environment = self.view_environment();
        // EnvironmentAttributeProbe: the spatially interpolated biome colours,
        // Gaussian-weighted over the noise biomes around the camera (plains
        // where no chunk is loaded, as ClientLevel.getUncachedNoiseBiome).
        let biomes = world_biomes!(self);
        let weights = {
            let chunks = &self.world.chunks;
            let plains = biomes.id("minecraft:plains");
            horizon_client::biome::gaussian_weights(camera.to_array(), |qx, qy, qz| {
                chunks
                    .get(&(qx >> 2, qz >> 2))
                    .map_or(plains, |c| c.biome(qx * 4, qy * 4, qz * 4))
            })
        };
        // visual/water_fog_color: no dimension sets it, its default is #050533.
        let water_fog = horizon_client::biome::blend_rgb(&weights, |b| {
            horizon_client::biome::rgb(biomes.get(b).water_fog)
        });
        // The attribute stack, with the camera's biome mix in the probe's order; the biome
        // maps as the packs left them.
        let attributes_of = |b: u16| {
            let info = biomes.get(b);
            environment.biome(&info.name).unwrap_or(&info.attributes)
        };
        let mut blend = horizon_world::environment_system::BiomeBlend::default();
        for &(b, weight) in &weights {
            blend.accumulate(weight, attributes_of(b));
        }
        // The packs' biome post-process group, mixed the same way where they set one.
        let empty = horizon_world::environment_attributes::AttributeMap::default();
        let post_of = |b: u16| {
            environment
                .biome_post(&biomes.get(b).name)
                .unwrap_or(&empty)
        };
        let mut post_blend = horizon_world::environment_system::BiomeBlend::default();
        if environment.has_biome_post() {
            for &(b, weight) in &weights {
                post_blend.accumulate(weight, post_of(b));
            }
        }
        let (rain, thunder) = self.view_weather(alpha as f32);
        // What the packs' attribute expressions read (only a pack's stack has any).
        let mut polytone_frame = None;
        if self.pack.environments.is_some()
            || self.pack.polytone_particles.is_some()
            || self.pack.polytone_post.is_some()
        {
            let light = self.world.light(block[0], block[1], block[2]);
            let frame = horizon_client::environment::Frame {
                rain,
                thunder,
                game_time: self.world.game_time,
                day_time: self.view_day_time(),
                render_distance: self.streaming.radius,
                sky_type: match self.dim {
                    Dim::Overworld | Dim::Extra(_) => 1,
                    Dim::End => 2,
                    Dim::Nether => 0,
                },
                dimension: self.dim.id().to_owned(),
                player: self.player.as_ref().map_or(camera.to_array(), |p| {
                    [p.position.x, p.position.y, p.position.z]
                }),
                player_velocity: self
                    .player
                    .as_ref()
                    .map_or([0.0; 3], |p| [p.velocity.x, p.velocity.y, p.velocity.z]),
                player_on_ground: self.player.as_ref().is_some_and(|p| p.ground.on_ground),
                in_water: underwater,
                camera: camera.to_array(),
                sky_light: i32::from(light >> 4),
                block_light: i32::from(light & 15),
                biome: biomes.get(biome).name.clone(),
                temperature: biomes.get(biome).temperature,
                downfall: biomes.get(biome).downfall,
            };
            if let Some(particles) = &self.pack.polytone_particles
                && let Ok(mut f) = particles.frame.write()
            {
                f.clone_from(&frame);
            }
            polytone_frame = Some(frame.clone());
            environment.set_frame(frame);
        }
        let mut sky = environment.sample(&horizon_client::environment::Sample {
            day_time: self.view_day_time(),
            rain,
            thunder,
            blend: Some(&blend),
            biome: Some(attributes_of(biome)),
            post_blend: Some(&post_blend),
            post_biome: Some(post_of(biome)),
        });
        self.end_flash_sky(&mut sky, alpha);
        if self.flash > 0 && !self.settings.file.bool("hideLightningFlashes") {
            // ClientLevel's flash layer while skyFlashTime > 0: the sky colour
            // moves 22% of the way to (0.8, 0.8, 1.0).
            let flash = [0.8, 0.8, 1.0];
            sky.sky_color =
                std::array::from_fn(|i| sky.sky_color[i] + (flash[i] - sky.sky_color[i]) * 0.22);
            sky.sky_light_factor = 1.0;
        }
        pr.lap("pr_environment");
        // FishingHookRenderer's line, then DebugRenderer: chunk borders and
        // hitboxes (F3+G, F3+B).
        let mut lines = self.fishing_line(alpha, size);
        let mut g = self.debug_gizmos(alpha as f32, camera.to_array());
        self.studio_camera_path(&mut g);
        lines.extend(g.lines);
        let on_top = g.on_top;
        pr.lap("pr_debug");
        let visuals = if self.title.is_none() {
            self.native_frame_visuals(alpha, &mut lines)
        } else {
            native_visuals::FrameVisuals::default()
        };
        let emissive_layers = if self.title.is_none() {
            let mut requests = Vec::new();
            self.native_emissive(&mut requests);
            // Native mods' overlays first, then the packs' OptiFine emissive blocks.
            horizon_client::native_emissive::table(
                &self.pack.render.sprite_layers,
                requests,
                &self.pack.render.emissive,
            )
        } else {
            Vec::new()
        };
        pr.lap("pr_native_visuals");
        let view = if self.studio.flying() {
            glam::Mat4::from_quat(self.camera_orientation().conjugate())
        } else {
            horizon_client::camera::view_matrix(view_yaw, view_pitch)
        };
        let at = camera.to_array();
        // GameRenderer.render3dHud: the debug crosshair, first person with the HUD shown.
        let mut crosshair = horizon_client::gizmos::Gizmos::default();
        if debug_crosshair_drawn(
            self.debug_crosshair(),
            self.camera_type.is_first_person(),
            self.ui.hide_gui,
            self.shows_panorama(),
        ) {
            let scale = HudBatch::new(size, self.settings.gui_scale_inputs()).scale;
            crosshair.debug_crosshair(at, view_pitch, view_yaw, scale);
        }
        let debug_lines = horizon_client::gizmos::camera_relative(&lines, at, view);
        let debug_lines_on_top = horizon_client::gizmos::camera_relative(&on_top, at, view);
        let debug_crosshair = horizon_client::gizmos::camera_relative(&crosshair.on_top, at, view);
        // LevelExtractor.extractBlockOutline: model translucency controls draw order;
        // no outline when an entity is targeted (Minecraft.hitResult an EntityHitResult).
        let mut outline_after_terrain = false;
        // GameRenderer.shouldRenderBlockOutline: not with the HUD hidden (F1), and only for a
        // player who may build. Adventure players only get one with a CAN_BREAK or
        // CAN_PLACE_ON item matching the block, and spectators with a menu block; Horizon
        // does not open menus for spectators, so those never show it. An open
        // screen does not hide it: vanilla checks the HUD and the game mode, not the screen.
        let adventure = self.mode == horizon_client::command::args::GameMode::Adventure;
        let can_build = self.may_build() || adventure;
        let outline = if block_outline_drawn(
            self.ui.hide_gui,
            self.entity_targeted(),
            can_build,
            self.shows_panorama(),
        ) {
            self.pick()
                .filter(|h| {
                    self.may_build()
                        || !self.block_action_restricted(h.pos)
                        || self.can_place_on_in_adventure(h.pos)
                })
                .filter(|h| {
                    let state = self.world.block(h.pos[0], h.pos[1], h.pos[2]);
                    let mut canceled = false;
                    self.native_block_highlight(h.pos, state, alpha as f32, &mut canceled);
                    !canceled
                })
                .map(|h| {
                    let state = self.world.block(h.pos[0], h.pos[1], h.pos[2]);
                    let f = self.res.facts().get(state);
                    outline_after_terrain = self
                        .pack
                        .render
                        .model_catalog
                        .collect(state, BlockPos::new(h.pos[0], h.pos[1], h.pos[2]))
                        .is_ok_and(|models| {
                            models
                                .iter()
                                .any(|(_, model)| model.faces.iter().any(|f| f.translucent))
                        });
                    let o = if f.outline_offset {
                        self.pack
                            .context
                            .tables
                            .offset(state, h.pos[0], h.pos[2])
                            .unwrap_or([0.0; 3])
                    } else {
                        [0.0; 3]
                    };
                    let p = h.pos.map(f64::from);
                    if let Some(edges) = self
                        .collision
                        .cached_shapes()
                        .context_outline_edges(state, self.outline_context_kind())
                    {
                        return edges
                            .iter()
                            .map(|e| std::array::from_fn(|i| p[i % 3] + o[i % 3] + e[i]))
                            .collect();
                    }
                    if let Some(b) = self.context_outline(h.pos, state, true, &self.be_anims) {
                        return horizon_client::blocks::cube_edges()
                            .iter()
                            .map(|e| {
                                std::array::from_fn(|i| {
                                    let k = i % 3;
                                    p[k] + b[k] + e[i] * (b[k + 3] - b[k])
                                })
                            })
                            .collect();
                    }
                    f.outline_edges
                        .iter()
                        .map(|e| std::array::from_fn(|i| p[i % 3] + o[i % 3] + e[i]))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        pr.lap("pr_outline");
        let title = self.shows_panorama();
        let (yaw, pitch) = if title {
            // PanoramaRenderer: the title cube spins at 0.1 degrees per tick, pitched 10 degrees.
            (
                self.panorama_rotation.update(
                    self.started.elapsed().as_secs_f32(),
                    self.settings.file.double("panoramaScrollSpeed") as f32,
                ),
                10.0,
            )
        } else {
            (view_yaw, view_pitch)
        };
        let fov_degrees = if title {
            85.0
        } else {
            self.fov_degrees(camera_alpha as f32, camera.to_array())
        };
        let camera_effects = fov::camera_effects_enabled(title, self.studio.flying());
        let bob = if !camera_effects {
            glam::Mat4::IDENTITY
        } else {
            self.bob_matrix(camera_alpha as f32)
        };
        let spin = if !camera_effects {
            glam::Mat4::IDENTITY
        } else {
            self.spinning_matrix(camera_alpha as f32)
        };
        let terrain_distance = self.terrain_distance();
        let distance = (terrain_distance * 16) as f32;
        let depth_far =
            horizon_client::camera::depth_far(terrain_distance, self.settings.cloud_range);
        // Only native mods read it: no matrices or allocation per frame without one.
        let mods_read_camera = !mod_api::__private::mods().is_empty();
        self.screen_blur_requested.set(false);
        self.native_camera_frame = (mods_read_camera && !title).then(|| {
            let partial = camera_alpha as f32;
            mod_api::Camera {
                position: camera.to_array(),
                eye_height: self.view.eye_height_old
                    + (self.view.eye_height - self.view.eye_height_old) * partial,
                partial_tick: partial,
                yaw,
                pitch,
                fov_degrees,
                near: 0.05,
                far: depth_far,
                view: (bob * spin * horizon_client::camera::view_matrix(yaw, pitch))
                    .to_cols_array(),
                projection: glam::Mat4::perspective_rh(
                    fov_degrees.to_radians(),
                    size[0] as f32 / size[1].max(1) as f32,
                    depth_far,
                    0.05,
                )
                .to_cols_array(),
                dimension: self.dim.id().to_string(),
            }
        });
        pr.lap("pr_projection");
        // GUI hooks project against this frame's camera, including its bob/spin and FOV.
        let mut hud = horizon_client::spans::time("hud", || self.hud(size));
        self.flush_ime(&mut hud);
        self.native_gui_render_post(&mut hud, self.ui.hide_gui, self.focused);
        self.resolve_inline_profiles(&hud.inline_profiles);
        pr.lap("pr_hud");
        self.locator_camera =
            (!title).then_some((camera.to_array(), view_yaw, view_pitch, fov_degrees));
        let mut requested_post_effects = self.requested_post_effects();
        // The packs' Polytone post chains, after vanilla's (#11486).
        let game_ticks = self.world.game_time as f64 + alpha;
        let delta_ticks = (game_ticks - self.polytone_post_ticks).clamp(0.0, 20.0) as f32;
        self.polytone_post_ticks = game_ticks;
        let polytone_extras = match (&self.pack.polytone_post, &polytone_frame) {
            (Some(post), Some(frame)) => {
                let view = bob * spin * horizon_client::camera::view_matrix(yaw, pitch);
                let globals = horizon_client::polytone_post::PolyGlobals {
                    // 26.3's reverse-Z projection, as the world's (and its depth) is.
                    projection: glam::Mat4::perspective_rh(
                        fov_degrees.to_radians(),
                        size[0] as f32 / size[1].max(1) as f32,
                        depth_far,
                        0.05,
                    ),
                    model_view: view,
                    sun_angle: sky.sun_angle.to_radians() - std::f32::consts::FRAC_PI_2,
                    day_time: self.view_day_time().rem_euclid(24000) as f32,
                    delta_time: delta_ticks,
                    player: self.player.as_ref().map_or(camera.to_array(), |p| {
                        [p.position.x, p.position.y, p.position.z]
                    }),
                };
                let (effects, extras) = post.frame(frame, &globals);
                requested_post_effects.extend(effects);
                Some(extras)
            }
            _ => None,
        };
        self.view_frustum = self
            .renderer
            .as_ref()
            .map(|r| r.camera_frustum(fov_degrees, yaw, pitch, bob * spin, depth_far));
        let billboard_frustum = if self.settings.horizon.taa {
            self.renderer.as_ref().map(|r| {
                r.camera_frustum_with_jitter(fov_degrees, yaw, pitch, bob * spin, depth_far)
            })
        } else {
            self.view_frustum
        };
        // Keep the buffer local while builders need mutable access to the game.
        let mut entities = std::mem::take(&mut self.entity_vertices);
        entities.clear();
        let mut static_block_entities = std::sync::Arc::from([]);
        let mut visible_static_block_entity_sections = Vec::new();
        let mut static_block_entity_stats = StaticCacheFrameStats::default();
        self.see_through_vertices.clear();
        self.glowing_vertices.get_mut().clear();
        self.shadow_only_vertices.get_mut().clear();
        pr.lap("pr_entity_setup");
        // The pack is chosen before entity geometry, so a pack that starts this frame
        // keeps block-entity models in the CPU stream instead of dropping them.
        let mut pack_inputs = if title {
            Default::default()
        } else {
            self.shader_pack_inputs(camera, alpha as f32, &sky)
        };
        if !title {
            let (built, static_sections, visible_static_sections, static_stats) =
                spans::time("entity_geometry", || {
                    self.entity_geometry(camera, alpha, billboard_frustum, entities)
                });
            entities = built;
            static_block_entities = static_sections;
            visible_static_block_entity_sections = visible_static_sections;
            static_block_entity_stats = static_stats;
        } else {
            // No level behind the panorama, so no entity either: the GPU path's
            // draws are started by `begin_skinning`, which only `entity_geometry`
            // runs, and would otherwise stay those of the last world frame and
            // be drawn over the title (#8192). Its models stay stored.
            self.skin_frame.get_mut().clear_draws();
        }
        pr.lap("pr_entities");
        let bolts = if title {
            Vec::new()
        } else {
            self.bolt_instances(camera)
        };
        // Particles blended additively (resource packs' Polytone ones, #11486), from the
        // render that built `entities`.
        let mut additive = self.particles.take_additive();
        // The dragon is drawn in every dimension, as any entity: a dragon
        // summoned outside the End (the server's AddEntity) is shown too.
        if !title {
            let mut dragon = Vec::new();
            self.dragon_geometry(camera, alpha, &mut dragon, &mut additive);
            self.dragon_fireball_geometry(camera, alpha, &mut dragon);
            entities.extend(dragon);
        }
        pr.lap("pr_dragon_bolts");
        let (celestial, stars) = if title {
            (Vec::new(), None)
        } else if self.dim == Dim::Overworld
            && self.potions.effects.get("darkness").is_none()
            && self.potions.effects.get("blindness").is_none()
        {
            // Camera.doesMobEffectBlockSky: no sun, moon or stars.
            self.celestial_geometry(&sky, rain)
        } else if self.dim == Dim::End && self.settings.horizon.sky {
            (self.end_flash_geometry(), None)
        } else {
            (Vec::new(), None)
        };
        self.custom_sky_frame([camera.x, camera.y, camera.z], &sky, (rain, thunder), title);
        pr.lap("pr_celestial");
        let (mut hand, hand_items) = if title || self.scoping() {
            Default::default()
        } else {
            self.hand_geometry(camera_alpha)
        };
        // GameRenderer.render3dHud: the screen effects after the hand, in
        // its pass (the hand pass draws with `bob * view`).
        if !title {
            let view = bob * horizon_client::camera::view_matrix(yaw, pitch);
            let aspect = size[0] as f32 / size[1].max(1) as f32;
            self.item_activation_geometry(camera_alpha as f32, view, aspect, &mut hand);
        }
        pr.lap("pr_hand");
        let hud_fov = self.hud_fov_degrees(camera_alpha as f32, camera.to_array());
        let screen = if title {
            Vec::new()
        } else {
            let scale = self
                .player
                .as_mut()
                .and_then(|p| p.attribute("minecraft:scale").ok())
                .unwrap_or(1.0) as f32;
            let hand_projection = horizon_client::camera::hand_projection(
                hud_fov,
                size[0] as f32,
                size[1].max(1) as f32,
                depth_far,
            );
            self.screen_quads(alpha as f32, size, scale, hand_projection)
        };
        if !title {
            self.end_geometry(camera, &mut entities);
        }
        self.outline_fill(&outline, camera, &mut entities);
        pr.lap("pr_screen_end");
        let weather = if !title && self.can_have_weather() && self.settings.horizon.weather {
            horizon_client::spans::time("weather", || self.weather_geometry(camera, alpha))
        } else {
            Vec::new()
        };
        let border = (!title)
            .then(|| self.border_draw(camera.to_array(), alpha as f32))
            .flatten();
        let see_through = std::mem::take(&mut self.see_through_vertices);
        pr.lap("pr_weather_border");
        if title {
            self.dynamic_lights.clear();
        } else {
            self.gather_dynamic_lights(camera, alpha);
        }
        pr.lap("pr_lights");
        pack_inputs.frame_timing = self
            .screenshot_sequence
            .as_ref()
            .map(screenshot_sequence::Sequence::timing);
        pr.lap("pr_shader_inputs");
        // Fog hooks may clear particles too. They must run before the final clear
        // boundary, not while constructing Frame after geometry has been extracted.
        let fog = self.frame_fog(camera, &sky, biome, water_fog);
        if !title {
            self.client_particle_geometry(camera, alpha, fov_degrees, &mut entities);
        }
        let mut world_geometry = self.world_geometry.take().expect("view storage");
        self.fill_world_geometry(
            title,
            camera.to_array(),
            view,
            alpha,
            &mut world_geometry,
            &mut entities,
        );
        let colored_lights = if self.title.is_none() {
            visuals.finish(self, camera)
        } else {
            Vec::new()
        };
        // Map textures and font glyphs changed while building this frame's
        // geometry (game/blocks/maps/mod.rs, world text) upload with this batch; draining
        // map_patches any earlier leaves them a frame behind.
        hud.patches.append(&mut self.map_patches);
        let entity_spans = std::mem::take(self.entity_spans.get_mut());
        let entity_motion = std::mem::take(self.entity_motion.get_mut());
        let entity_motion_posed = std::mem::take(self.entity_motion_posed.get_mut());
        let glowing = std::mem::take(self.glowing_vertices.get_mut());
        let shadow_only = std::mem::take(self.shadow_only_vertices.get_mut());
        let skin = std::mem::take(self.skin_frame.get_mut());
        let frame = Frame {
            weather: &weather,
            weather_time: horizon_client::weather::Time::new(self.world.game_time, alpha as f32),
            nether_lighting: self.dim == Dim::Nether,
            camera,
            yaw,
            pitch,
            fov_degrees,
            eye: eye.filter(|_| !title).map_or(camera, |e| e.0),
            eye_rotation: eye.filter(|_| !title).map_or([yaw, pitch], |e| e.1),
            first_person: eye.is_none() || title || !self.view.detached,
            bob,
            spin,
            sky,
            shadow_scene_revision: (self.world.revision, self.world.chunks.membership_revision()),
            shadow_cache: self.settings.horizon.shadow_cache,
            distance,
            detail_distance: (self.streaming.radius * 16) as f32,
            depth_far,
            underwater,
            water_plane: if title
                || underwater
                || !self
                    .renderer
                    .as_ref()
                    .is_some_and(|r| r.wants_water_plane())
            {
                None
            } else {
                self.water_plane(camera)
            },
            title,
            sky_blocked: horizon_client::fog::sky_pass_skipped(
                horizon_client::fog::fog_type(
                    &self.world,
                    self.res.facts(),
                    [camera.x, camera.y, camera.z],
                    &self.near_plane(),
                ),
                self.potions.effects.get("blindness").is_some()
                    || self.potions.effects.get("darkness").is_some(),
                self.boss_overlay.should_create_world_fog(),
            ),
            skybox: self.dim == Dim::Overworld && self.settings.horizon.sky,
            end_sky: self.dim == Dim::End && self.settings.horizon.sky,
            end_sky_sprite: self
                .gui
                .atlas
                .sprites
                .get("environment/end_sky")
                .map(|r| [r.u0, r.v0, r.u1, r.v1]),
            boss_world_fog: self.boss_overlay.should_create_world_fog(),
            environment_fog: (self.dim == Dim::Nether).then_some((sky.fog_start, sky.fog_end)),
            cave_culling,
            water_fog,
            fog: Some(fog),
            dark_disc: self.dark_disc(camera),
            outline,
            outline_after_terrain,
            outline_high_contrast: self.settings.file.bool("highContrastBlockOutline"),
            pvp: self.settings.extras.pvp.frame(),
            debug_lines: &debug_lines,
            debug_lines_on_top: &debug_lines_on_top,
            debug_crosshair: &debug_crosshair,
            hud: &hud,
            glint_alpha: self.glint_frame.alpha,
            glint_ticks: self.glint_frame.scroll_ticks(),
            clouds: (!title
                && self.dim == Dim::Overworld
                && horizon_client::clouds::visible(&self.settings.clouds, sky.cloud_color[3]))
            .then(|| self.gui.atlas.sprites.get("horizon/white"))
            .flatten()
            .map(|white| horizon_client::clouds::Input {
                texture: &self.pack.clouds,
                uv: [(white.u0 + white.u1) / 2.0, (white.v0 + white.v1) / 2.0],
                camera,
                time: self.world.game_time,
                partial_tick: alpha,
                range: self.settings.cloud_range,
                fast: self.settings.clouds == "fast",
                color: sky.cloud_color,
            }),
            entities: &entities,
            static_block_entities: &static_block_entities,
            visible_static_block_entity_sections: &visible_static_block_entity_sections,
            static_block_entity_stats,
            entity_motion: &entity_motion,
            entity_motion_posed: &entity_motion_posed,
            entity_spans: &entity_spans,
            skin: skin.pending().then_some(&skin),
            glowing: &glowing,
            dynamic_lights: &self.dynamic_lights,
            colored_lights: &colored_lights,
            emissive_layers: &emissive_layers,
            lightning: horizon_client::gfx::lightning::Input {
                bolts: &bolts,
                uv: horizon_client::gfx::lightning::white_uv(&self.gui.atlas.font.white()),
            },
            additive: &additive,
            see_through: &see_through,
            world_primitives: world_geometry.batch(),
            shadow_only: &shadow_only,
            celestial: if self.settings.horizon.sky || title {
                &celestial
            } else {
                &[]
            },
            stars: stars.filter(|_| self.settings.horizon.sky || title),
            hand: &hand,
            hand_items: &hand_items,
            hand_fov_degrees: hud_fov,
            screen: &screen,
            pack: pack_inputs,
            border,
            game_time: self.world.game_time as f32,
            shader_game_time: ((self.world.game_time % 24000) as f32 + alpha as f32) / 24000.0,
            crumbling_sprite: self
                .gui
                .atlas
                .sprites
                .get("block/destroy_stage_0")
                .map_or([0.0; 4], |r| [r.u0, r.v0, r.u1 - r.u0, r.v1 - r.v0]),
            end_portal_sprites: ["environment/end_sky", "entity/end_portal/end_portal"].map(|n| {
                self.gui
                    .atlas
                    .sprites
                    .get(n)
                    .map_or([0.0; 4], |r| [r.u0, r.v0, r.u1 - r.u0, r.v1 - r.v0])
            }),
            ticks: self.texture_clock(),
            darkness: self.darkness_lighting(alpha as f32),
            night_vision: self.night_vision_intensity(alpha as f32),
            block_flicker: self.fog.block_light_flicker,
            boss_darken: self
                .boss_overlay
                .darken_amount(self.clock.fraction() as f32),
            menu_blur: self.menu_blur(),
            post_effects: horizon_client::post_chain::Request {
                effects: &requested_post_effects,
                root: &self.pack.source,
                extras: polytone_extras.as_ref(),
                menu_blur_radius: self.settings.menu_blur as f32,
                // GlobalSettingsUniform.update.
                globals: horizon_client::post_chain::Globals {
                    camera_block_pos: [
                        camera.x.floor() as i32,
                        camera.y.floor() as i32,
                        camera.z.floor() as i32,
                    ],
                    camera_offset: [
                        (camera.x.floor() - camera.x) as f32,
                        (camera.y.floor() - camera.y) as f32,
                        (camera.z.floor() - camera.z) as f32,
                    ],
                    game_time: ((self.world.game_time % 24000) as f32 + alpha as f32) / 24000.0,
                    ..Default::default()
                },
            },
            chunk_fade: self.settings.chunk_fade,
        };
        pr.lap("pr_frame");
        // Renderer encoding and GPU waiting belong to render, not prepare.
        drop(prepare);
        let debug_passes = self.debug_lists_gpu_passes();
        let mut prepared_draws = std::mem::take(&mut *self.prepared_model_draws.borrow_mut());
        let result = match self.renderer.as_mut() {
            Some(renderer) => {
                renderer.set_pass_timing(debug_passes || self.gpu_passes);
                renderer.set_pass_timing_gap(if self.gpu_passes {
                    Duration::ZERO
                } else {
                    horizon_client::debug_overlay::GPU_TIMING_GAP
                });
                renderer.stage_prepared_draws(&prepared_draws);
                consume(renderer, &frame)
            }
            None => Err("renderer".into()),
        };
        prepared_draws.clear();
        *self.prepared_model_draws.borrow_mut() = prepared_draws;
        let _cleanup = spans::guard("pr_cleanup");
        // A frame the surface refused (occluded, lost, timed out) is `Ok(false)`.
        self.frame_drawn = matches!(result, Ok(true));
        // Restore storage even when rendering fails (for example, a lost surface).
        self.entity_vertices = entities;
        self.world_geometry = Some(world_geometry);
        *self.entity_spans.get_mut() = entity_spans;
        *self.entity_motion.get_mut() = entity_motion;
        *self.entity_motion_posed.get_mut() = entity_motion_posed;
        *self.glowing_vertices.get_mut() = glowing;
        *self.shadow_only_vertices.get_mut() = shadow_only;
        let mut skin = skin;
        // The renderer took the new models when its store grew by them; otherwise they stay
        // pending, or the stores restart (`SkinModels::begin`).
        if self
            .renderer
            .as_ref()
            .and_then(|r| r.skinned_models())
            .is_some_and(|stored| stored == skin.base + skin.new_model.len() as u32)
        {
            skin.handed();
        }
        *self.skin_frame.get_mut() = skin;
        self.see_through_vertices = see_through;
        result
    }
}
