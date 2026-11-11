// game-module-cfg: feature java-mods
//! The server, the connection, commands.

use super::*;

impl Game<'_> {
    fn java_player_uuid(player: &str) -> u128 {
        u128::from_str_radix(&player.replace('-', ""), 16).unwrap_or_default()
    }

    /// `MinecraftServer.getProfilePermissions`: the acting player's level is swapped into the game, the others' are the hub's.
    fn java_permission(&self, player: &str) -> i32 {
        let uuid = Self::java_player_uuid(player);
        if horizon_client::lan::playerdata::uuid_ints(uuid) == self.acting_uuid() {
            return i32::from(
                self.lan_acting_permission
                    .unwrap_or(self.ui.registries.permission),
            );
        }
        self.lan
            .as_ref()
            .map_or(0, |host| i32::from(host.hub.permission_of(uuid)))
    }

    /// `PlayerList.isOp`: the operators' list, and the single-player owner with commands allowed.
    fn java_is_op(&self, player: &str) -> bool {
        let uuid = Self::java_player_uuid(player);
        if horizon_client::lan::playerdata::uuid_ints(uuid) == self.acting_uuid() {
            return self.acting_player_is_op();
        }
        self.lan.as_ref().is_some_and(|host| host.hub.is_op(uuid))
    }
}

horizon_java_host::java_bridges! { table for Game<'static> {
    "net/minecraft/server/players/PlayerList".isOp
    "(Lnet/minecraft/server/players/NameAndId;)Z" =>
    fn player_list_is_op(game, player: Text) -> bool {
        game.java_is_op(&player.0)
    }

    "net/minecraft/server/MinecraftServer".getProfilePermissions
    "(Lnet/minecraft/server/players/NameAndId;)Lnet/minecraft/server/permissions/LevelBasedPermissionSet;" =>
    fn server_profile_permissions(game, player: Text) -> i32 {
        game.java_permission(&player.0)
    }

    "net/minecraft/client/server/IntegratedServer".getProfilePermissions
    "(Lnet/minecraft/server/players/NameAndId;)Lnet/minecraft/server/permissions/LevelBasedPermissionSet;" =>
    fn integrated_profile_permissions(game, player: Text) -> i32 {
        game.java_permission(&player.0)
    }

    "dev/horizonlab/javamods/boot/Wire".wire
    "(Ljava/lang/String;[B)V" =>
    fn wire_send(_game, player: Text, frame: Bytes) -> () {
        super::super::java_mods::java_wire_push(&player.0, frame.0);
    }

    "net/minecraft/world/level/storage/PrimaryLevelData".isHardcore
    "()Z" =>
    fn world_data_hardcore(game) -> bool {
        game.level.hardcore
    }

    "net/minecraft/world/level/storage/PrimaryLevelData".getDifficulty
    "()Lnet/minecraft/world/Difficulty;" =>
    fn world_data_difficulty(game) -> i32 {
        i32::from(game.level.difficulty_id())
    }

    "net/minecraft/world/level/storage/PrimaryLevelData".isDifficultyLocked
    "()Z" =>
    fn world_data_difficulty_locked(game) -> bool {
        game.level.difficulty_locked
    }

    "net/minecraft/world/level/storage/PrimaryLevelData".isAllowCommands
    "()Z" =>
    fn world_data_allow_commands(game) -> bool {
        game.allow_commands()
    }

    "net/minecraft/world/level/storage/PrimaryLevelData".getGameType
    "()Lnet/minecraft/world/level/GameType;" =>
    fn world_data_game_type(game) -> i32 {
        game.level.mode()
    }

    "net/minecraft/world/level/storage/PrimaryLevelData".getLevelName
    "()Ljava/lang/String;" =>
    fn world_data_level_name(game) -> Text {
        Text(game.level.name.clone())
    }
}}
