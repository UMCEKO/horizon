//! Experience points a player gains or loses (`Player.giveExperiencePoints`).
use super::Game;

impl Game<'_> {
    /// `Player.giveExperiencePoints`: `add_experience_levels` runs once for each
    /// level gained or lost, and the mods hear of the level change once.
    pub(super) fn give_experience_points(&mut self, points: i32) {
        let Some(points) = self.xp_change(points) else {
            return;
        };
        let old_level = self.experience.level;
        self.experience.add_points(points);
        while self.experience.progress < 0.0 {
            let xp = &self.experience;
            let lost = xp.progress * horizon_client::entities::Experience::needed(xp.level) as f32;
            let had_level = xp.level > 0;
            self.add_experience_levels(-1);
            let xp = &mut self.experience;
            xp.progress = if had_level {
                1.0 + lost / horizon_client::entities::Experience::needed(xp.level) as f32
            } else {
                0.0
            };
        }
        while self.experience.progress >= 1.0 {
            let xp = &mut self.experience;
            xp.progress =
                (xp.progress - 1.0) * horizon_client::entities::Experience::needed(xp.level) as f32;
            self.add_experience_levels(1);
            let xp = &mut self.experience;
            xp.progress /= horizon_client::entities::Experience::needed(xp.level) as f32;
        }
        self.native_player_level_changed(old_level);
    }
}
