// game-module-cfg: feature java-mods
//! Block entities and mods' blocks: Java holds the objects, Horizon their saved form and their turn to tick.

use super::*;
use horizon_java_host::bridge::Doubles;

horizon_java_host::java_bridges! { table for Game<'static> {
    "dev/horizonlab/javamods/boot/BlockEntities".fetch
    "(J)V" =>
    fn block_entity_fetch(game, pos: Pos) -> () {
        game.java_send_block_entity(pos.0);
    }

    "dev/horizonlab/javamods/boot/BlockEntities".adopt
    "(JLjava/lang/String;)J" =>
    fn block_entity_adopt(game, pos: Pos, id: Text) -> i64 {
        game.java_adopt_block_entity(pos.0, id.0)
    }

    "dev/horizonlab/javamods/boot/BlockEntities".forget
    "(JJ)V" =>
    fn block_entity_forget(game, pos: Pos, generation: i64) -> () {
        let key = (pos.0.x, pos.0.y, pos.0.z);
        if game.java_generation_current(key, generation) {
            game.world.block_entities.release_for_java(&key);
            game.world.block_entities.remove(&key);
        }
    }

    "dev/horizonlab/javamods/boot/BlockEntities".store
    "(JJ[B)J" =>
    fn block_entity_store(game, pos: Pos, generation: i64, tag: Bytes) -> i64 {
        game.java_store_block_entity(pos.0, generation, &tag.0)
    }

    "dev/horizonlab/javamods/boot/Clients".collisionShape
    "(JLnet/minecraft/world/level/block/state/BlockState;[D)V" =>
    fn block_collision_shape(game, pos: Pos, state: State, boxes: Doubles) -> () {
        game.java_collision_shape([pos.0.x, pos.0.y, pos.0.z], state.0, &boxes.0);
    }
}}
