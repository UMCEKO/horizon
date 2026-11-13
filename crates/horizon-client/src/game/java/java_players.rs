// game-module-cfg: feature java-mods
//! A server player joining a level and teleporting, as mods' mixins patch them.

use super::java_routing::{Recv, TransitionArg, route};
use super::native_entities::Removal;
use super::*;
use horizon_java_host::wire::In;

/// Where a `Flat.raw(TeleportTransition)` sends a player: the feet, the turn and the level.
pub(in crate::game) fn parse_transition(bytes: &[u8]) -> Option<([f64; 3], [f32; 2], Dim)> {
    let mut r = In(bytes);
    let at = [r.double()?, r.double()?, r.double()?];
    let turn = [r.float()?, r.float()?];
    r.double()?;
    r.double()?;
    r.double()?;
    Some((at, turn, Dim::from_id(r.text()?)))
}

impl Game<'_> {
    /// `ServerLevel.addPlayer` for the acting player, at native's sites of a player joining a level.
    pub(in crate::game) fn level_add_player(&mut self, dim: Dim) {
        route!(
            self,
            "net/minecraft/server/level/ServerLevel#addPlayer(Lnet/minecraft/server/level/ServerPlayer;)V",
            Recv::Level(dim),
            (self.java_player_arg())
        );
        self.level_add_player_body(dim);
    }

    /// `ServerLevel.addPlayer`'s body: the player's view joins the level.
    pub(in crate::game) fn level_add_player_body(&mut self, dim: Dim) {
        let [a, b, c, d] = self.acting_uuid().map(|part| u128::from(part as u32));
        let uuid = a << 96 | b << 64 | c << 32 | d;
        self.java_hook_player_level(uuid, dim.id(), true, Removal::Discarded);
    }

    /// `ServerPlayer.teleport` of the guest `conn`, who is swapped in for the call.
    pub(in crate::game) fn player_teleport_of(
        &mut self,
        conn: u32,
        level: Dim,
        at: [f64; 3],
        rotation: Option<[f32; 2]>,
    ) -> bool {
        if self.lan_acting == Some(conn) {
            return self.player_teleport(level, at, rotation);
        }
        match self.as_guest(conn, |g| g.player_teleport(level, at, rotation)) {
            Some(done) => done,
            None => self.queue_player_teleport(conn, level, at, rotation),
        }
    }

    /// `ServerPlayer.teleport` for the acting player: whether it happened.
    pub(in crate::game) fn player_teleport(
        &mut self,
        level: Dim,
        at: [f64; 3],
        rotation: Option<[f32; 2]>,
    ) -> bool {
        route!(
            self,
            "net/minecraft/server/level/ServerPlayer#teleport(Lnet/minecraft/world/level/portal/TeleportTransition;)Lnet/minecraft/server/level/ServerPlayer;",
            Recv::Player,
            (self.transition_arg(level, at, rotation))
        );
        self.player_teleport_body(level, at, rotation)
    }

    /// `ServerPlayer.teleport`'s body: native's move of the acting player.
    pub(in crate::game) fn player_teleport_body(
        &mut self,
        level: Dim,
        at: [f64; 3],
        rotation: Option<[f32; 2]>,
    ) -> bool {
        let Some(conn) = self.lan_acting else {
            return route::early(false);
        };
        if !self.queue_player_teleport(conn, level, at, rotation) {
            return route::early(false);
        }
        true
    }

    /// The `TeleportTransition` a teleport to `at` in `level` is, with no motion.
    fn transition_arg(
        &mut self,
        level: Dim,
        at: [f64; 3],
        rotation: Option<[f32; 2]>,
    ) -> TransitionArg {
        self.java_register_level(level);
        let [yaw, pitch] = rotation.unwrap_or([self.yaw, self.pitch]);
        let mut bytes = Vec::new();
        for value in at {
            bytes.extend(value.to_be_bytes());
        }
        bytes.extend(yaw.to_be_bytes());
        bytes.extend(pitch.to_be_bytes());
        bytes.extend([0u8; 24]);
        let id = level.id();
        bytes.extend((id.len() as i32).to_be_bytes());
        bytes.extend(id.as_bytes());
        TransitionArg(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transition_names_where_the_player_goes() {
        let mut bytes = Vec::new();
        for value in [1.5f64, 100.0, -7.25] {
            bytes.extend(value.to_be_bytes());
        }
        bytes.extend(90.0f32.to_be_bytes());
        bytes.extend((-10.0f32).to_be_bytes());
        bytes.extend([0u8; 24]);
        let id = "minecraft:the_nether";
        bytes.extend((id.len() as i32).to_be_bytes());
        bytes.extend(id.as_bytes());
        let (at, turn, level) = parse_transition(&bytes).expect("a whole transition parses");
        assert_eq!(at, [1.5, 100.0, -7.25]);
        assert_eq!(turn, [90.0, -10.0]);
        assert_eq!(level, Dim::Nether);
    }
}
