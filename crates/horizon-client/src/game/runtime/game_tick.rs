//! The game's tick: the client's own update, then the world for a server.
use super::*;

impl Game<'_> {
    pub(super) fn tick(&mut self) -> Result<()> {
        if self.agent.hold_server {
            return Ok(());
        }
        self.abort_open_transactions();
        // `Player.entityInteractionRange`/`blockInteractionRange` for the tick's
        // picks: the attribute carries creative's and a server's modifiers.
        self.entity_reach = self.interaction_range("minecraft:entity_interaction_range", 3.0, 5.0);
        self.block_reach = self.interaction_range("minecraft:block_interaction_range", 4.5, 5.0);
        self.tick_debug_keys()?;
        self.tick_debug_charts();
        self.tick_tutorial();
        if self.screen == Screen::Creative {
            self.refresh_creative_tabs();
            // CreativeModeInventoryScreen.containerTick: a player without
            // infinite materials gets the survival inventory instead.
            if !self.creative {
                if let Some(c) = self.carried.take() {
                    let _ = self.add_stack_to_inventory(c);
                }
                self.set_screen(Screen::Inventory);
            }
        }
        let _lockstep = self.java_hook_tick_begin();
        #[cfg(feature = "java-mods")]
        self.java_creative_tabs_take();
        if self.remote.is_some() {
            let result = self.with_residency_tick(Self::tick_remote);
            self.java_client_tick_send();
            return result;
        }
        // Only a server ticks its own world from here: its players are guests, swapped in for
        // their turns (`as_guest`), and the window's player is a client of it.
        self.ticks += 1;
        self.repro_tick();
        self.track_experience_display();
        self.tick_enchanting_book();
        // `/tick freeze` stops the world but not the player.
        let running = match self.level_pass.running {
            Some(running) => running,
            None => self.clock.world_runs(),
        };
        // GameRenderer.tick: a wither's bar darkens the world, then fades;
        // the activated item counts down.
        self.boss_overlay.tick_darken(running);
        self.tick_item_activation();
        self.native_inventory_ticks();
        if running {
            // `MinecraftServer.tickChildren`: `commandFunctions` runs before
            // any level ticks, so a tick function reads the game time and the
            // day clock from before this tick. Vanilla's SavedData is loaded
            // long before this point; the lazy scoreboard/storage read has to
            // be done by then too, or a load tag writes over what it restores.
            self.load_scoreboard();
            self.tick_functions();
            self.world.game_time += 1;
            if self
                .ui
                .rules
                .bool_at(horizon_client::gamerules::vanilla::ADVANCE_WEATHER)
            {
                self.world.weather.tick_timers(&mut self.world.random);
            }
            self.world.weather.tick_levels();
        } else if self.clock.still() {
            // Under `HORIZON_STILL` a frozen world's rain and thunder sit at their levels, as a
            // shader pack's smoothed values do, so `/weather rain` shows full rain without
            // stepping the world (and its mobs) through the fade.
            self.world.weather.settle_levels();
        }
        // ClientLevel.tick: the sky flash fades in a frozen level too.
        self.tick_weather_effects();
        let _ = self.finish_player_autosaves(false);
        // MinecraftServer.tickServer's countdown, then autoSave: three
        // hundred seconds of ticks at the rate the server runs at now.
        self.ticks_until_autosave -= 1;
        self.autosaving = self.ticks_until_autosave <= 0;
        if let Some(serving) = &self.serving {
            serving.set_saving(self.autosaving);
        }
        if self.autosaving {
            self.save_game_with(false)?;
            self.ticks_until_autosave = self.next_autosave_interval();
            if let Some(serving) = &self.serving {
                serving.set_saving(false);
            }
        }
        if running {
            self.advance_clock();
            // `ServerLevel.tickTime`, once the clock has moved on.
            self.run_scheduled_functions();
        }
        if self.use_delay > 0 {
            self.use_delay -= 1;
        }
        self.tick_totem();
        self.update_totem_ready();
        self.tick_command_extras();
        self.tick_world(running)
    }
}
