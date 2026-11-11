//! Chunks queued for the writer thread, some saved as the record a mod wrote for them.
use super::{
    Arc, Biomes, BlockRegistry, Chunk, Encoding, Folder, NamedTag, Record, Result, SaveDir,
    put_calculated_blending,
};
use std::collections::HashMap;

impl SaveDir {
    /// `save_later`, except that a chunk with a record in `written` is saved as that record (`ChunkDataEvent.Save`'s tag).
    pub fn save_later_written(
        &self,
        chunks: &[Arc<Chunk>],
        registry: &'static BlockRegistry,
        biomes: &Arc<Biomes>,
        written: Vec<((i32, i32), NamedTag)>,
    ) -> Result<usize> {
        let mut written: HashMap<(i32, i32), NamedTag> = written.into_iter().collect();
        let encoding = Arc::new(Encoding {
            registry,
            biomes: biomes.clone(),
            tables: self.tables.get().cloned(),
            shape: self.shape,
        });
        let records = chunks
            .iter()
            .map(|c| {
                let key = (c.x, c.z);
                let blending = self.calculated_blending(c.x, c.z);
                let record = match written.remove(&key) {
                    Some(mut tag) => {
                        if let Some(data) = &blending {
                            put_calculated_blending(&mut tag, data);
                        }
                        Record::Tag(Arc::new(tag))
                    }
                    None => Record::Chunk(c.clone(), encoding.clone(), blending),
                };
                (key, record)
            })
            .collect();
        self.queue(Folder::Region, records, false)?;
        self.save_poi(chunks, false)?;
        Ok(chunks.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::nbt::Tag;
    use crate::chunk::LevelShape;

    #[test]
    fn a_written_record_is_saved_in_place_of_its_chunk() {
        let Some(root) = std::env::var_os("HORIZON_TEST_RESOURCES") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        let assets = crate::assets::Assets::load(&root).expect("assets");
        let biomes = Arc::new(
            Biomes::load(&horizon_assets::resource_source::ResourceRoot::directory(root))
                .expect("biomes"),
        );
        let dir = std::env::temp_dir().join(format!("horizon-written-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let save = SaveDir::new(dir.clone(), LevelShape::OVERWORLD);
        let chunks = [Arc::new(Chunk::empty(0, 0)), Arc::new(Chunk::empty(1, 0))];
        let mut tag = super::super::encode(&chunks[0], assets.registry, &biomes);
        if let Tag::Compound(map) = &mut tag.tag {
            map.insert("mymod:data".into(), Tag::Int(7));
        }
        save.save_later_written(&chunks, assets.registry, &biomes, vec![((0, 0), tag)])
            .unwrap();
        save.flush().unwrap();
        let mod_data = |x| save.raw_chunk(x, 0).unwrap().tag.get("mymod:data").cloned();
        assert_eq!(mod_data(0), Some(Tag::Int(7)));
        assert_eq!(mod_data(1), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
