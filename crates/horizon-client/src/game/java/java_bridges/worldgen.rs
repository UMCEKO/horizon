// game-module-cfg: feature java-mods
//! Java's generation context reads native's chunks: `NativeChunkScan`, the `ChunkScanAccess` of each level's `StructureCheck`.

use super::*;
use horizon_client::chunk::Chunk;
use horizon_core::nbt::{self, Compound, Limits, NamedTag, Tag};

/// What `NativeChunkScan` visits for a column: its data version and `structures`, from the loaded column or else its save; none when native never had it.
fn scanned_structures(
    loaded: Option<&Chunk>,
    saved: impl FnOnce() -> Option<NamedTag>,
) -> Option<NamedTag> {
    let structures = match loaded {
        Some(chunk) => horizon_client::save::structures_tag(
            chunk,
            chunk
                .saved
                .as_ref()
                .and_then(|extras| extras.root.get(&"structures".into())),
        ),
        None => saved()?.tag.get("structures")?.clone(),
    };
    let mut root = Compound::new();
    root.insert(
        "DataVersion".into(),
        Tag::Int(horizon_world::DATA_VERSION),
    );
    root.insert("structures".into(), structures);
    Some(NamedTag {
        name: Default::default(),
        tag: Tag::Compound(root),
    })
}

impl Game<'_> {
    /// The structures of column `x`, `z` of the level named `level`, in place or parked, as `NativeChunkScan` reads them.
    fn java_chunk_structures(&self, level: &str, x: i32, z: i32) -> Option<NamedTag> {
        if self.dim.id() == level {
            return scanned_structures(self.world.chunks.get(&(x, z)).map(|c| &**c), || {
                self.save.raw_chunk(x, z)
            });
        }
        let world = self.levels.parked_world_of(level)?;
        let save = self.levels.parked_save_of(level)?;
        scanned_structures(world.chunks.get(&(x, z)).map(|c| &**c), || {
            save.raw_chunk(x, z)
        })
    }
}

horizon_java_host::java_bridges! { table for Game<'static> {
    "dev/horizonlab/javamods/boot/NativeChunkScan".structures
    "(Ljava/lang/String;J)[B" =>
    fn chunk_scan_structures(game, level: Text, chunk: i64) -> Bytes {
        let (x, z) = (chunk as i32, (chunk >> 32) as i32);
        let tag = game.java_chunk_structures(&level.0, x, z);
        Bytes(tag.and_then(|tag| nbt::write_named(&tag, Limits::saved()).ok()).unwrap_or_default())
    }
}}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(tag: &NamedTag) -> Option<&Tag> {
        tag.tag.get("structures")?.get("References")?.get("minecraft:village_plains")
    }

    #[test]
    fn a_loaded_column_answers_from_memory_and_a_missing_one_from_its_save() {
        let mut chunk = Chunk::empty(3, -2);
        chunk
            .structure_refs
            .push(("minecraft:village_plains".into(), (2, -1)));
        let from_memory = scanned_structures(Some(&chunk), || panic!("a loaded column read its save")).unwrap();
        assert_eq!(
            from_memory.tag.get("DataVersion"),
            Some(&Tag::Int(horizon_world::DATA_VERSION))
        );
        // `ChunkPos.pack`: x in the low half, z in the high half.
        let packed = i64::from(2u32) | i64::from(-1i32) << 32;
        assert_eq!(refs(&from_memory), Some(&Tag::LongArray(vec![packed])));

        let saved = NamedTag {
            name: Default::default(),
            tag: Tag::compound([(
                "structures",
                Tag::compound([("References", Tag::compound([("minecraft:village_plains", Tag::LongArray(vec![7]))]))]),
            )]),
        };
        let from_save = scanned_structures(None, || Some(saved)).unwrap();
        assert_eq!(refs(&from_save), Some(&Tag::LongArray(vec![7])));
        assert!(scanned_structures(None, || None).is_none());
    }

    #[test]
    fn a_loaded_column_keeps_the_starts_its_save_had() {
        let mut chunk = Chunk::empty(0, 0);
        let mut root = Compound::new();
        root.insert(
            "structures".into(),
            Tag::compound([("starts", Tag::compound([("mymod:tower", Tag::compound([("id", Tag::String("mymod:tower".into()))]))]))]),
        );
        chunk.saved = Some(std::sync::Arc::new(horizon_client::chunk::ChunkExtras {
            root,
            ..Default::default()
        }));
        let scanned = scanned_structures(Some(&chunk), || None).unwrap();
        assert!(scanned.tag.get("structures").and_then(|s| s.get("starts")).and_then(|s| s.get("mymod:tower")).is_some());
    }
}
