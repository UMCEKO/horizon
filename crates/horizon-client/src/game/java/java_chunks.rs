// game-module-cfg: feature java-mods
//! `ChunkMap`'s chunk events for Java mods, driven by Horizon's chunk loading, saving and unloading, with the
//! chunk's full vanilla tag as mods read and write it (`ChunkEvents` in Java).

use super::*;
use horizon_client::chunk::{Chunk, ChunkExtras};
use horizon_client::save::kept_root;
use horizon_core::nbt::{self, Limits, NamedTag};
use horizon_java_host::wire::{In, Out};

const DATA_LOAD: i32 = 0;
const LOAD: i32 = 1;
const UNLOAD: i32 = 2;
const SAVE: i32 = 3;
const UNSAVED: i32 = 4;
const LEVEL_SAVE: i32 = 5;
const UNLOADED: i32 = 6;

thread_local! {
    // Columns left the level and Java holds their views until their save.
    static LEAVING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn tag_bytes(tag: &NamedTag) -> Option<Vec<u8>> {
    nbt::write_named(tag, Limits::saved()).ok()
}

fn bytes_tag(bytes: &[u8]) -> Option<NamedTag> {
    nbt::read_named(bytes, Limits::saved())
        .ok()
        .map(|(tag, _)| tag)
}

/// What a chunk saved as `written` keeps in memory: the root keys Horizon writes back as loaded.
fn with_written_root(chunk: &mut Arc<Chunk>, written: &NamedTag) {
    let root = kept_root(&written.tag);
    if chunk
        .saved
        .as_ref()
        .map_or(root.is_empty(), |s| s.root == root)
    {
        return;
    }
    let extras = Arc::make_mut(chunk)
        .saved
        .get_or_insert_with(|| Arc::new(ChunkExtras::default()));
    Arc::make_mut(extras).root = root;
}

impl Game<'_> {
    /// One `chunk_events` call for this level; the reply past the mods' effects is `read`'s.
    fn java_chunk_events<T>(
        &mut self,
        kind: i32,
        body: impl FnOnce(&mut Out),
        read: impl FnOnce(&mut In) -> Option<T>,
    ) -> Option<T> {
        let key = self.java_level_key();
        let mut out = Out::default();
        out.int(key as i32);
        out.long(self.world.game_time);
        out.int(kind);
        body(&mut out);
        let reply = self.java_request("chunk_events", &out.0)?;
        let mut reply = In(&reply);
        let answer = read(&mut reply);
        self.java_apply_effects(&mut reply);
        self.java_drain_block_events();
        answer
    }

    /// Columns read from the save: their data reaches Java before their block entities, as `ChunkMap.scheduleChunkLoad` reads it.
    pub(super) fn java_chunks_read(
        &mut self,
        joined: &horizon_client::fast_hash::FxHashSet<(i32, i32)>,
    ) {
        let tags: Vec<(i32, i32, Vec<u8>)> = joined
            .iter()
            .filter_map(|key| self.world.chunks.get(key))
            .filter(|c| c.saved.is_some())
            .filter_map(|c| {
                let tag = self.save.raw_chunk(c.x, c.z)?;
                Some((c.x, c.z, tag_bytes(&tag)?))
            })
            .collect();
        if tags.is_empty() {
            return;
        }
        self.java_chunk_events(
            DATA_LOAD,
            |out| {
                out.int(tags.len() as i32);
                for (x, z, tag) in &tags {
                    out.int(*x);
                    out.int(*z);
                    out.bytes(tag);
                }
            },
            |_| Some(()),
        );
    }

    /// Columns now full in the level: `ChunkEvent.Load`, new for a generated one.
    pub(super) fn java_chunks_full(
        &mut self,
        joined: &horizon_client::fast_hash::FxHashSet<(i32, i32)>,
    ) {
        let columns: Vec<(i32, i32, bool)> = joined
            .iter()
            .filter_map(|key| self.world.chunks.get(key))
            .map(|c| (c.x, c.z, c.saved.is_none()))
            .collect();
        if columns.is_empty() {
            return;
        }
        self.java_chunk_events(
            LOAD,
            |out| {
                out.int(columns.len() as i32);
                for (x, z, generated) in &columns {
                    out.int(*x);
                    out.int(*z);
                    out.bool(*generated);
                }
            },
            |_| Some(()),
        );
    }

    fn java_positions(reply: &mut In) -> Option<Vec<(i32, i32)>> {
        (0..reply.int()?)
            .map(|_| {
                let pos = reply.long()?;
                Some((pos as i32, (pos >> 32) as i32))
            })
            .collect()
    }

    /// Columns leaving the level stop being loaded; one a mod marked unsaved is saved with the changed ones.
    pub(super) fn java_chunks_leaving(&mut self, unloaded: &[Arc<Chunk>]) {
        if unloaded.is_empty() {
            return;
        }
        LEAVING.set(true);
        let unsaved = self.java_chunk_events(
            UNLOAD,
            |out| {
                out.int(unloaded.len() as i32);
                for chunk in unloaded {
                    out.int(chunk.x);
                    out.int(chunk.z);
                }
            },
            Self::java_positions,
        );
        self.world.modified.extend(unsaved.unwrap_or_default());
    }

    /// Loaded columns a mod marked unsaved, which `ChunkMap.saveAllChunks` saves with the changed ones.
    pub(super) fn java_chunks_unsaved(&mut self) {
        if !self.java_level_running() {
            return;
        }
        let keys: Vec<(i32, i32)> = self.world.chunks.keys().copied().collect();
        let unsaved = self.java_chunk_events(
            UNSAVED,
            |out| {
                out.int(keys.len() as i32);
                for (x, z) in &keys {
                    out.int(*x);
                    out.int(*z);
                }
            },
            Self::java_positions,
        );
        self.world.modified.extend(unsaved.unwrap_or_default());
    }

    /// `ChunkMap.save` for each chunk about to be written: `ChunkDataEvent.Save` on its full tag; what Java's `write` returns is the record to save. Chunks that left the level get `ChunkEvent.Unload` after that, as `scheduleUnload` does.
    pub(super) fn java_chunks_saving(
        &mut self,
        chunks: &[Arc<Chunk>],
    ) -> Vec<((i32, i32), NamedTag)> {
        if !self.java_level_running() {
            return Vec::new();
        }
        let leaving = LEAVING.replace(false);
        let written = if chunks.is_empty() {
            Vec::new()
        } else {
            self.java_chunks_written(chunks)
        };
        if leaving {
            self.java_chunk_events(UNLOADED, |_| {}, |_| Some(()));
        }
        written
    }

    fn java_chunks_written(&mut self, chunks: &[Arc<Chunk>]) -> Vec<((i32, i32), NamedTag)> {
        let (registry, biomes) = (self.pack.assets.registry, self.res.biomes());
        let tables = self.save.tables.get().map(|t| &**t);
        let tags: Vec<(&Arc<Chunk>, Vec<u8>)> = chunks
            .iter()
            .filter_map(|c| {
                let tag =
                    horizon_client::save::encode_with(c, registry, biomes, tables, self.save.shape);
                Some((c, tag_bytes(&tag)?))
            })
            .collect();
        let written = self.java_chunk_events(
            SAVE,
            |out| {
                out.int(tags.len() as i32);
                for (chunk, tag) in &tags {
                    out.int(chunk.x);
                    out.int(chunk.z);
                    out.bytes(tag);
                }
            },
            |reply| {
                (0..reply.int()?)
                    .map(|_| bytes_tag(reply.bytes()?))
                    .collect::<Option<Vec<NamedTag>>>()
            },
        );
        let Some(written) = written else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for ((chunk, _), tag) in tags.iter().zip(written) {
            let key = (chunk.x, chunk.z);
            if let Some(live) = self.world.chunks.get_mut(&key) {
                with_written_root(live, &tag);
            }
            out.push((key, tag));
        }
        out
    }

    /// `ServerLevel.save`'s `LevelEvent.Save`, after the level's chunks.
    pub(super) fn java_level_saved(&mut self) {
        if self.java_level_running() {
            self.java_chunk_events(LEVEL_SAVE, |_| {}, |_| Some(()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::nbt::{Compound, Tag};

    fn tag(entries: impl IntoIterator<Item = (&'static str, Tag)>) -> NamedTag {
        NamedTag {
            name: "".into(),
            tag: Tag::compound(entries),
        }
    }

    #[test]
    fn the_full_chunk_tag_survives_the_wire() {
        let chunk = tag([
            ("DataVersion", Tag::Int(7)),
            (
                "sections",
                Tag::List(vec![Tag::compound([("Y", Tag::Byte(-4))])]),
            ),
            ("block_entities", Tag::List(Vec::new())),
            (
                "Heightmaps",
                Tag::compound([("WORLD_SURFACE", Tag::LongArray(vec![1, -2]))]),
            ),
            (
                "neoforge:attachments",
                Tag::compound([("a:b", Tag::Int(3))]),
            ),
        ]);
        let back = bytes_tag(&tag_bytes(&chunk).unwrap()).unwrap();
        assert_eq!(back, chunk);
    }

    #[test]
    fn what_java_wrote_replaces_the_kept_keys_and_leaves_the_rest_to_the_tag() {
        let mut live = Arc::new(Chunk::empty(0, 0));
        let extras = Arc::make_mut(&mut live)
            .saved
            .get_or_insert_with(|| Arc::new(ChunkExtras::default()));
        Arc::make_mut(extras).root = [
            ("structures".into(), Tag::Compound(Compound::new())),
            ("mymod:gone".into(), Tag::Int(2)),
        ]
        .into_iter()
        .collect();
        let written = tag([
            ("sections", Tag::List(Vec::new())),
            ("InhabitedTime", Tag::Long(5)),
            ("neoforge:attachments", Tag::Int(3)),
        ]);
        with_written_root(&mut live, &written);
        let root = &live.saved.as_ref().unwrap().root;
        let keys: Vec<String> = root.keys().map(|k| k.to_string_lossy()).collect();
        assert_eq!(keys, ["InhabitedTime", "neoforge:attachments"]);
        assert_eq!(root.get(&"neoforge:attachments".into()), Some(&Tag::Int(3)));
    }
}
