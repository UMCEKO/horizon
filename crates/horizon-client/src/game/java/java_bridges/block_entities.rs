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
    "(JLjava/lang/String;)V" =>
    fn block_entity_adopt(game, pos: Pos, id: Text) -> () {
        game.java_adopt_block_entity(pos.0, id.0);
    }

    "dev/horizonlab/javamods/boot/BlockEntities".forget
    "(J)V" =>
    fn block_entity_forget(game, pos: Pos) -> () {
        let key = (pos.0.x, pos.0.y, pos.0.z);
        game.world.block_entities.release_for_java(&key);
        game.world.block_entities.remove(&key);
    }

    "dev/horizonlab/javamods/boot/BlockEntities".store
    "(J[B)V" =>
    fn block_entity_store(game, pos: Pos, tag: Bytes) -> () {
        game.java_store_block_entity(pos.0, &tag.0);
    }

    "dev/horizonlab/javamods/boot/Clients".collisionShape
    "(JLnet/minecraft/world/level/block/state/BlockState;[D)V" =>
    fn block_collision_shape(game, pos: Pos, state: State, boxes: Doubles) -> () {
        game.java_collision_shape([pos.0.x, pos.0.y, pos.0.z], state.0, &boxes.0);
    }
}}
