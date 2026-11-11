//! The LAN host's tick: guest packets, tracking, and the flushes to the guests.
use super::{Game, lan::LanHost};
use horizon_client::lan::{self, hub::TickCtx};

impl Game<'_> {
    /// The end of the level's tick: the entities added since its states were
    /// taken for the guests (a falling block's drop, a spawner's mob, the
    /// host's snowball) are paired without another tracking tick
    /// (`Hub::note_joined`). Only they are
    /// described (their ids are above the counter the states were taken at),
    /// and nothing when the counter has not moved or no guest is in the level.
    pub(super) fn lan_note_joined(&mut self) {
        let Some(host) = self.lan.take() else {
            return;
        };
        let mut host = self.lan_flush_tracking(host);
        host.states_next_entity = None;
        self.lan = Some(host);
        self.flush_attachment_sync();
    }

    /// Deliver start/stop callbacks before sounds and attachment snapshots.
    /// A callback can spawn another entity, whose pairing is part of the same
    /// operation, as a nested ChunkMap.addEntity would be in vanilla.
    fn lan_flush_tracking(&mut self, mut host: LanHost) -> LanHost {
        loop {
            self.lan_pair_joined(&mut host);
            host.hub.collect_tracking();
            if host.hub.native_tracking.is_empty() {
                return host;
            }
            self.lan = Some(host);
            self.java_hook_tracking();
            self.run_native_tracking();
            host = self
                .lan
                .take()
                .expect("the hub is back after the tracking events");
        }
    }

    pub(super) fn tick_lan(&mut self, paused: bool) {
        // A frozen level does not reach tick_lan_packets through tick_world.
        // Apply its packets before taking the snapshot too.
        if !paused {
            self.tick_lan_packets();
        }
        if self.lan.is_none() {
            if self.level_pass.running.is_none() {
                self.level_pass.packets_drained = false;
            }
            return;
        }
        // What the tick sends a guest goes to its connection at the end.
        let holding = lan::Holding::start();
        self.register_native_commands();
        // The host's own commands' (and command blocks') players moving
        // to another level, and their broadcastToAdmins lines.
        if !self.level_teleports.is_empty() {
            self.run_level_teleports();
        }
        if !self.admin_feedback.is_empty() {
            self.send_admin_feedback(None);
        }
        // An animal whose rider left the game (or the level) stops taking
        // its input.
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        for m in &mut self.mobs {
            if m.rider.is_some()
                && self.riding != Some(m.id)
                && !host.hub.guests.iter().any(|g| g.vehicle == Some(m.id))
            {
                m.rider = None;
            }
        }
        // A guest whose file names the vehicle it rode rides it again.
        self.restore_guest_vehicles();
        // Finish the players' mining, item use and pickups before describing
        // this tick to their clients. Otherwise a newly broken block and its
        // drops remain absent from the snapshot until the following tick.
        if !paused {
            horizon_client::spans::time("lan_guests", || self.tick_guests());
            // The vehicles the guests just moved enter their portals now.
            let Some(host) = self.lan.as_ref() else {
                return;
            };
            let ridden: Vec<u32> = host.hub.guests.iter().filter_map(|g| g.vehicle).collect();
            self.find_ridden_vehicle_portals(&ridden);
        }
        // A guest's mount fired its `started_riding` with that guest alone in
        // place (the hub held by its action): now every player on the vehicle
        // hears it (`Entity.startRiding`).
        if !self.started_riding.is_empty() {
            self.flush_started_riding();
        }
        {
            let Some(host) = self.lan.as_mut() else {
                return;
            };
            let has_guests = !host.hub.guests.is_empty();
            let mut states = std::mem::take(&mut host.states);
            if has_guests {
                self.lan_assign_ids();
                let Some(host) = self.lan.as_mut() else {
                    return;
                };
                let mut guests: Vec<(i64, u32, i32)> = host
                    .hub
                    .guests
                    .iter()
                    .filter_map(|g| Some((g.vehicle_order, g.vehicle?, g.entity_id)))
                    .collect();
                guests.sort_by_key(|&(order, ..)| order);
                let guests: Vec<(u32, i32)> = guests
                    .into_iter()
                    .map(|(_, vehicle, id)| (vehicle, id))
                    .collect();
                let mut riding = std::mem::take(&mut host.hub.riding);
                self.lan_riding(&guests, &mut riding);
                let Some(host) = self.lan.as_mut() else {
                    return;
                };
                host.hub.riding = riding;
                // Idle states are kept only when the players' moves are in:
                // the clients that could pair are known. The range scale is
                // the one the tracker gets below.
                let mut idle_states = std::mem::take(&mut host.idle_states);
                let mut kept = if self.level_pass.packets_drained {
                    let percent = if host.hub.config.integrated {
                        (self.settings.entity_distance * 100.0).round() as i32
                    } else {
                        host.hub.entity_range_percent
                    };
                    idle_states.pass(&host.hub, percent)
                } else {
                    None
                };
                horizon_client::spans::time("lan_entity_states", || {
                    self.lan_entities(&mut states, true, 0, kept.as_mut());
                    if let Some(kept) = kept {
                        kept.finish();
                    }
                    self.lan_hook_states(&mut states);
                    self.lan_entity_uuids(&mut states);
                });
                let Some(host) = self.lan.as_mut() else {
                    return;
                };
                host.idle_states = idle_states;
                host.states_next_entity = Some(self.next_entity);
            } else {
                let Some(host) = self.lan.as_mut() else {
                    return;
                };
                host.states_next_entity = None;
                states.clear();
                // Nobody can receive past movement. A returning viewer pairs
                // from the cart's current position/rotation, as AddEntity does.
                self.carts.for_each_mut(|cart| cart.lerp_steps.clear());
            }
            // Persist is swapped per level, so parked dimensions retain their marks.
            self.spend_forced_meta();
            // ServerEntity.sendChanges clears needsSync once it has looked.
            // SteppedInterpolationTracker.updateTracking clears syncPosition.
            for m in &mut self.mobs {
                m.needs_sync = false;
                m.sync_position = false;
            }
            self.creatures
                .projectiles
                .for_each_mut(|p| p.needs_sync = false);
            for d in &mut self.dropped {
                d.needs_sync = false;
            }
            self.tridents.iter_mut().for_each(|t| t.needs_sync = false);

            // The step command reaches clients before the events its tick
            // generated, as ServerTickRateManager.updateStepTicks does.
            let Some(host) = self.lan.as_mut() else {
                return;
            };
            host.hub.sync_ticking(
                self.clock.rate(),
                self.clock.frozen(),
                self.clock.step_state(),
            );
            // `runBlockEvents`' broadcast precedes what the events change (held
            // for the next tick, `tick_world`), and follows the blocks changed
            // before them (`lan_broadcast_changes`): the guest re-runs a
            // piston's `triggerEvent` on the blocks as they were before the
            // move, a base the last tick restored included.
            for (pos, b0, b1, state) in std::mem::take(&mut self.lan_block_events) {
                host.hub.block_event(pos, b0, b1, state);
            }
            let entities = states.as_slice();
            self.lan_prepare_captured_sounds(entities);
            let Some(host) = self.lan.as_mut() else {
                return;
            };
            // An entity that left the level since the last tick (a snowball
            // that hit) is dropped by the tracker below: its event goes out
            // first, as `Level.broadcastEntityEvent` precedes the discard.
            let (gone, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.lan_entity_events)
                .into_iter()
                .partition(|(id, ..)| !entities.iter().any(|e| e.id == *id));
            self.lan_entity_events = later;
            for (id, event) in gone {
                host.hub.entity_event(id, event);
            }
            host.hub.far_level(&self.save);
            let spawn = self.world_spawn_position();
            let events = {
                let clock_rate = self.network_clock_rate();
                let Some(host) = self.lan.as_mut() else {
                    return;
                };
                let mut ctx = TickCtx {
                    sea_level: self.end.locator().map_or(63, |terrain| terrain.sea_level()),
                    world: &mut self.world,
                    entities,
                    next_entity: &mut self.next_entity,
                    spawn,
                    spawn_angle: self.level.world_spawn_angle,
                    difficulty: i32::from(self.ui.difficulty),
                    difficulty_locked: self.level.difficulty_locked,
                    clock_rate,
                    tick_rate: self.clock.rate(),
                    tick_frozen: self.clock.frozen(),
                    tick_steps: self.clock.step_state(),
                    collision: Some(self.collision),
                    border: &self.border,
                    streaming: Some(&self.streaming),
                };
                host.hub.movement_checks = (
                    self.ui
                        .rules
                        .bool_at(horizon_client::gamerules::vanilla::PLAYER_MOVEMENT_CHECK),
                    self.ui
                        .rules
                        .bool_at(horizon_client::gamerules::vanilla::ELYTRA_MOVEMENT_CHECK),
                );
                // IntegratedServer.getScaledTrackingDistance: the Entity Distance
                // option scales what an integrated server or a LAN host tracks (a
                // dedicated one keeps its entity-broadcast-range-percentage).
                if host.hub.config.integrated {
                    host.hub.entity_range_percent =
                        (self.settings.entity_distance * 100.0).round() as i32;
                }
                host.hub.set_immediate_respawn(
                    self.ui
                        .rules
                        .bool_at(horizon_client::gamerules::vanilla::IMMEDIATE_RESPAWN),
                );
                host.hub.set_limited_crafting(
                    self.ui
                        .rules
                        .bool_at(horizon_client::gamerules::vanilla::LIMITED_CRAFTING),
                );
                // A server with several levels runs a pass for each
                // (`tick_server`): the network is read once, before the first.
                // Player packets may already have been drained before
                // scheduled block ticks (`tick_lan_packets`).
                host.hub.mods_loaded = !mod_api::__private::mods().is_empty();
                let pass = self.level_pass;
                let mut events = if pass.first && !pass.packets_drained {
                    host.hub.poll(&mut ctx)
                } else {
                    Vec::new()
                };
                if !pass.packets_drained {
                    events.extend(host.hub.drain_packets(&mut ctx));
                }
                events.extend(horizon_client::spans::time("lan_tick_level", || {
                    host.hub.tick_level(&mut ctx)
                }));
                if pass.last {
                    horizon_client::spans::time("lan_end_tick", || host.hub.end_tick());
                }
                events
            };
            self.wake_woken_guests();
            let Some(host) = self.lan.take() else {
                return;
            };
            let processed = horizon_client::spans::time("lan_process_events", || {
                self.lan_process_events(host, events, entities, spawn)
            });
            let Some(back) = processed else {
                return;
            };
            self.lan = Some(back);
            let Some(host) = self.lan.as_mut() else {
                return;
            };
            host.states = states;
        }
        if self.lan.as_ref().is_some_and(|host| !host.quiet.is_empty()) {
            self.run_quiet_commands();
        }
        if self.native_net_waiting() {
            self.run_native_net();
        }
        if self
            .lan
            .as_ref()
            .is_some_and(|host| !host.hub.native_advancement_grants.is_empty())
        {
            self.run_native_advancement_grants();
        }
        if self
            .lan
            .as_ref()
            .is_some_and(|host| !host.hub.native_menu_events.is_empty())
        {
            self.run_native_menu_events();
        }
        self.java_hook_tracking();
        if self
            .lan
            .as_ref()
            .is_some_and(|host| !host.hub.chunk_watches.is_empty())
        {
            self.run_chunk_watches();
        }
        if let Some(host) = self.lan.as_mut() {
            host.hub.collect_tracking();
        }
        if self
            .lan
            .as_ref()
            .is_some_and(|host| !host.hub.native_tracking.is_empty())
        {
            self.run_native_tracking();
        }
        // A blast from a guest's attack, and a mace's wind burst.
        self.flush_lan_explosions();
        let _ = self.flush_wind_bursts();
        // A sweep attack's reach to the other players from a guest's attack.
        self.flush_sweeps();
        // A reeled-in hook pulling a player.
        self.flush_hook_pulls();
        // Particles a guest's turn sent (sweep attack, damage indicator, a hook's splashes).
        self.flush_lan_particles();
        self.lan_flush_effects();
        self.lan_living_flags();
        self.lan_stuck(paused);
        self.tick_server_travel();
        self.lan_swings();
        self.lan_update_streaming();
        self.lan_guest_poses();
        self.lan_update_waypoints();
        self.lan_sync_scoreboard();
        self.lan_sync_border();
        if let Some(host) = self.lan.as_mut() {
            host.hub.anticheat_ping();
        }
        drop(holding);
        self.flush_attachment_sync();
        self.flush_native_net();
        self.flush_far_terrain();
        if let Some(host) = self.lan.as_ref() {
            lan::flush_all(host.hub.guests.iter().map(|g| &g.sender));
        }
        self.flush_native_sounds();
        // `tick_server` resets the pass after all dimensions; outside a pass
        // the packet gate is per-tick.
        if self.level_pass.running.is_none() {
            self.level_pass.packets_drained = false;
        }
    }
}
