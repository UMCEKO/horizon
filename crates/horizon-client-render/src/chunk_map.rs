//! Resident chunk membership, with a token for consumers that cache its keys.
use crate::{chunk::Chunk, fast_hash::FxHashMap};
use std::{
    collections::hash_map::{Iter, ValuesMut},
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

type Key = (i32, i32);
type Columns = FxHashMap<Key, Arc<Chunk>>;
static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);
fn next_revision() -> u64 {
    NEXT_REVISION.fetch_add(1, Ordering::Relaxed)
}

/// Read access behaves like a map. Structural writes are explicit so edits to
/// blocks or light don't invalidate consumers of the loaded chunk key set.
/// Tokens are process-wide: replacing the entire map also invalidates caches.
pub struct ChunkMap {
    columns: Columns,
    revision: u64,
    watch: Option<Watch>,
}

/// The keys written since a watcher last took them, or that everything was.
#[derive(Default)]
struct Watch {
    touched: Vec<Key>,
    all: bool,
}

/// What a watcher must re-read: every column, or only these keys.
pub enum Touched {
    All,
    Keys(Vec<Key>),
}
impl Default for ChunkMap {
    fn default() -> Self {
        Self {
            columns: Columns::default(),
            revision: next_revision(),
            watch: None,
        }
    }
}
impl ChunkMap {
    fn touch(&mut self, key: Key) {
        if let Some(watch) = &mut self.watch
            && !watch.all
            && watch.touched.last() != Some(&key)
        {
            watch.touched.push(key);
        }
    }
    /// Starts logging writes on the first call (all columns are touched); later calls return what was written since.
    pub fn take_touched(&mut self) -> Touched {
        match &mut self.watch {
            None => {
                self.watch = Some(Watch::default());
                Touched::All
            }
            Some(watch) if watch.all => {
                watch.all = false;
                watch.touched.clear();
                Touched::All
            }
            Some(watch) => Touched::Keys(std::mem::take(&mut watch.touched)),
        }
    }
    pub fn membership_revision(&self) -> u64 {
        self.revision
    }
    pub fn insert(&mut self, key: Key, chunk: Arc<Chunk>) -> Option<Arc<Chunk>> {
        let previous = self.columns.insert(key, chunk);
        self.touch(key);
        if previous.is_none() {
            self.revision = next_revision();
        }
        previous
    }
    pub fn remove(&mut self, key: &Key) -> Option<Arc<Chunk>> {
        let previous = self.columns.remove(key);
        self.touch(*key);
        if previous.is_some() {
            self.revision = next_revision();
        }
        previous
    }
    pub fn clear(&mut self) {
        if let Some(watch) = &mut self.watch {
            watch.all = true;
        }
        if !self.columns.is_empty() {
            self.columns.clear();
            self.revision = next_revision();
        }
    }
    pub fn get_mut(&mut self, key: &Key) -> Option<&mut Arc<Chunk>> {
        self.touch(*key);
        self.columns.get_mut(key)
    }
    pub fn values_mut(&mut self) -> ValuesMut<'_, Key, Arc<Chunk>> {
        if let Some(watch) = &mut self.watch {
            watch.all = true;
        }
        self.columns.values_mut()
    }
}
impl Deref for ChunkMap {
    type Target = Columns;
    fn deref(&self) -> &Columns {
        &self.columns
    }
}
impl<'a> IntoIterator for &'a ChunkMap {
    type Item = (&'a Key, &'a Arc<Chunk>);
    type IntoIter = Iter<'a, Key, Arc<Chunk>>;
    fn into_iter(self) -> Self::IntoIter {
        self.columns.iter()
    }
}
impl crate::light::LightWorld for ChunkMap {
    fn section(&self, cx: i32, s: usize, cz: i32) -> Option<&Arc<crate::chunk::Section>> {
        self.columns.section(cx, s, cz)
    }
    fn set_section_light(&mut self, cx: i32, s: usize, cz: i32, light: crate::chunk::Light) {
        self.touch((cx, cz));
        self.columns.set_section_light(cx, s, cz, light);
    }
    fn layout(&self) -> crate::chunk::ColumnLayout {
        self.columns.layout()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_watcher_takes_only_the_columns_written_since_its_last_take() {
        let mut map = ChunkMap::default();
        map.insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        assert!(matches!(map.take_touched(), Touched::All));
        assert!(matches!(map.take_touched(), Touched::Keys(k) if k.is_empty()));
        map.insert((1, 0), Arc::new(Chunk::empty(1, 0)));
        Arc::make_mut(map.get_mut(&(0, 0)).unwrap()).lit = true;
        map.remove(&(1, 0));
        assert!(matches!(map.take_touched(), Touched::Keys(k) if k == [(1, 0), (0, 0), (1, 0)]));
        map.clear();
        assert!(matches!(map.take_touched(), Touched::All));
        assert!(matches!(map.take_touched(), Touched::Keys(k) if k.is_empty()));
    }
    #[test]
    fn membership_tokens_track_keys_and_map_replacement_only() {
        let mut map = ChunkMap::default();
        let empty = map.membership_revision();
        map.clear();
        map.remove(&(0, 0));
        assert_eq!(empty, map.membership_revision());
        let chunk = Arc::new(Chunk::empty(0, 0));
        map.insert((0, 0), chunk.clone());
        let inserted = map.membership_revision();
        assert_ne!(empty, inserted);
        map.insert((0, 0), chunk.clone());
        Arc::make_mut(map.get_mut(&(0, 0)).unwrap()).lit = true;
        for column in map.values_mut() {
            Arc::make_mut(column).light_loaded = true;
        }
        crate::light::LightWorld::set_section_light(
            &mut map,
            0,
            0,
            0,
            crate::chunk::Light::Uniform(0),
        );
        assert_eq!(inserted, map.membership_revision());
        map.remove(&(0, 0));
        map.insert((1, 0), chunk);
        assert_eq!(map.len(), 1);
        assert_ne!(inserted, map.membership_revision());
        let replaced = map.membership_revision();
        map.clear();
        assert_ne!(replaced, map.membership_revision());
        let cleared = map.membership_revision();
        map = ChunkMap::default();
        assert_ne!(cleared, map.membership_revision());
    }
}
