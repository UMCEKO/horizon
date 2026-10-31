//! Block entities and mods' blocks: Java holds the objects, Horizon their saved form and their turn to tick.

use super::*;

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
        game.world.block_entities.remove(&(pos.0.x, pos.0.y, pos.0.z));
    }
}}
