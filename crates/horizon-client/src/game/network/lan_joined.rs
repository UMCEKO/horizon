//! A guest joining the server: its saved data loaded, then placed in its level.
use super::Game;
use horizon_client::lan::{
    entity::{EntityState, States},
    hub::TickCtx,
    playerdata,
};

impl Game<'_> {
    pub(super) fn lan_guest_joined(
        &mut self,
        conn: u32,
        entities: &[EntityState],
        spawn: [f64; 3],
    ) {
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let default_mode = host.hub.config.default_game_mode;
        let force = host.hub.config.force_game_mode;
        let root = self.world_root.clone();
        let in_place = host.hub.config.dimension.clone();
        let levels = host.hub.config.levels;
        let owner = host.owner.as_mut();
        let Some((known, uuid, name)) = host.hub.update_guest(conn, |g| {
            // PlayerList.loadPlayerData: the owner gets the world's player file,
            // everyone else their own.
            let known = match owner.filter(|o| o.uuid == g.uuid) {
                Some(o) => playerdata::load_owner(g, &root, o.saved.take().as_deref()),
                None => playerdata::load(g, &root),
            };
            // `PlayerList.placeNewPlayer`: a saved level the world no longer has is the Overworld.
            if !levels.has(&g.dimension) {
                g.dimension = "minecraft:overworld".to_owned();
            }
            // PlayerEvent.LoadFromFile: the player's data is read and applied,
            // the player not yet placed.
            (known, g.uuid, g.name.clone())
        }) else {
            return;
        };
        let java_client = host.hub.java_guests().any(|u| u == uuid);
        self.java_hook_remote_join(uuid, java_client);
        self.initialize_server_player(uuid, conn);
        self.native_player_load_from_file(uuid, &name, &playerdata::directory(&root));
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        if host
            .hub
            .update_guest(conn, |g| {
                // PrepareSpawnTask: with no saved rotation the player faces the world
                // spawn's angle (`/setworldspawn ... <angle>`).
                playerdata::face_world_spawn(g, known, self.level.world_spawn_angle);
                if !known || force {
                    g.game_mode = default_mode;
                    // ServerPlayerGameMode.setGameModeForPlayer: the abilities of the mode chosen.
                    g.abilities.update_for_mode(default_mode);
                }
            })
            .is_none()
        {
            return;
        }
        if !known {
            // ServerPlayer.adjustSpawnLocation: a player new to the world
            // stands where PlayerSpawnFinder puts them, not all on one spot
            // (it reads the loaded level, which holds the spawn when it is
            // the overworld).
            let feet = if self.dim == super::Dim::Overworld {
                self.server_player_spawn(spawn)
            } else {
                spawn
            };
            let Some(host) = self.lan.as_mut() else {
                return;
            };
            if host
                .hub
                .update_guest(conn, |g| {
                    g.position = feet;
                    g.previous_position = feet;
                    // Ready.spawn uses the world spawn's yaw and pitch.
                    [g.yaw, g.pitch] = self.level.world_spawn_angle;
                    g.head_yaw = g.yaw;
                    g.health = 20.0;
                    g.food = 20;
                    g.dimension = super::Dim::Overworld.id().to_owned();
                })
                .is_none()
            {
                return;
            }
        }
        // A server places each player in their own level (`PlayerList.placeNewPlayer`).
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let back = self.dim;
        let Some((level, mut elsewhere)) = host.hub.update_guest(conn, |g| {
            let level = super::Dim::from_id(&g.dimension);
            let elsewhere = level != back;
            if !elsewhere {
                g.dimension = in_place;
            }
            (level, elsewhere)
        }) else {
            return;
        };
        self.load_guest_records(conn);
        // The mods say which commands the newcomer's tree keeps.
        self.native_filter_commands(Some(conn));
        // A level that will not open, its generator gone with a mod, is one the
        // world no longer has: left in it, the player would fail the next tick.
        if elsewhere && let Err(e) = self.enter_level(level) {
            log::error!("could not open {}: {e}", level.id());
            let overworld = super::Dim::Overworld;
            let moved = self.lan.as_mut().and_then(|host| {
                host.hub
                    .update_guest(conn, |g| g.dimension = overworld.id().to_owned())
            });
            elsewhere = back != overworld;
            if moved.is_none() || (elsewhere && self.enter_level(overworld).is_err()) {
                return;
            }
        }
        let mut theirs = States::default();
        let entities = if elsewhere {
            // A dedicated server has no host player.
            self.lan_entities(&mut theirs, false, 0, None);
            self.lan_hook_states(&mut theirs);
            self.lan_entity_uuids(&mut theirs);
            theirs.as_slice()
        } else {
            entities
        };
        let clock_rate = self.network_clock_rate();
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
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        host.hub.place_new_player(conn, &mut ctx);
        self.bind_player_attachments(conn, false);
        if elsewhere && let Err(e) = self.enter_level(back) {
            log::error!("could not reopen {}: {e}", back.id());
        }
    }
}
