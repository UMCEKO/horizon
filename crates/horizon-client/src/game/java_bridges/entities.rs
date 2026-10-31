//! Entities, on views: real Java entities read from Horizon's saved form, and what a call changed applied back.

use super::*;

horizon_java_host::java_bridges! { table for Game<'static> {
    "dev/horizonlab/javamods/boot/EntityViews".query
    "(DDDDDD)[B" =>
    fn entities_query(game, x1: f64, y1: f64, z1: f64, x2: f64, y2: f64, z2: f64) -> Bytes {
        Bytes(game.java_entities_in([x1, y1, z1], [x2, y2, z2]))
    }

    "dev/horizonlab/javamods/boot/EntityViews".queryId
    "(I)[B" =>
    fn entities_query_id(game, id: i32) -> Bytes {
        Bytes(game.java_entity_by_id(id as u32))
    }
}}
