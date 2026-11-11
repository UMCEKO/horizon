//! The level's chunk saves: `ServerLevel.save`, and the columns streaming loads and unloads each update.
use super::{Game, java_hooks};
use horizon_client::Result;
use horizon_client::chunk::Chunk;
use horizon_client::mesher::SectionMesh;
use std::sync::Arc;
use std::time::{Duration, Instant};

type Uploads = Vec<((i32, i32, i32), SectionMesh)>;

impl Game<'_> {
    /// Save the level in place: its changed chunks, its entities and its raids.
    pub(super) fn save_level(&mut self, wait: bool) -> Result<()> {
        self.java_hook_flush_block_entities();
        Self::admit_entity_save()?;
        self.save_border();
        self.save_chunk_tickets()?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match self.streaming.prepare_save(&mut self.world, wait, deadline) {
                horizon_client::streaming::SaveStep::Full(chunk) => {
                    self.native_late_full_chunk_snapshot(&chunk);
                    self.streaming.save_finished(chunk);
                }
                horizon_client::streaming::SaveStep::Protos(protos) => {
                    self.native_proto_snapshots(&protos);
                    self.streaming.finish_save(protos, wait);
                    break;
                }
                horizon_client::streaming::SaveStep::Pending => unreachable!(),
            }
        }
        self.java_hook_chunks_unsaved();
        let keys: Vec<_> = self.world.modified.iter().copied().collect();
        self.save.save_attachments(&self.world.attachments)?;
        for key in &keys {
            self.world.modified.remove(key);
        }
        let chunks = self.with_entities(keys.iter().filter_map(|k| self.world.chunks.get(k)));
        self.native_full_chunk_snapshots(&chunks);
        let written = self.java_hook_chunks_saving(&chunks);
        self.save.save_later_written(
            &chunks,
            self.pack.assets.registry,
            world_biomes!(self),
            written,
        )?;
        // Prepare entities while the chunk writer runs, then use one barrier.
        if !wait {
            self.world.modified.autosaved(&keys, Instant::now());
        }
        self.save_entities(false)?;
        self.save_raids();
        if wait {
            self.save.flush()?;
        }
        self.java_hook_level_saved();
        if mod_api::__private::listened(mod_api::HookId::LevelSave) {
            self.native_level_save(self.dim.id());
        }
        Ok(())
    }

    /// `update_world`'s streaming: columns planned, loaded and unloaded, the unloaded changed ones saved, then the mods told.
    pub(super) fn stream_chunks(&mut self) -> (Uploads, Vec<Arc<Chunk>>) {
        let areas = self.streaming.extra.len();
        let waiting = std::mem::take(&mut self.spawn_wait);
        self.streaming
            .extra
            .extend(waiting.into_iter().map(|chunk| (chunk, 0)));
        let plan = horizon_client::spans::time("streaming_plan", || {
            let mut step = self.streaming.begin_plan_update(&mut self.world);
            loop {
                match step {
                    horizon_client::streaming::PlanStep::Ready(plan) => break plan,
                    horizon_client::streaming::PlanStep::Save { chunk, resume } => {
                        self.native_late_full_chunk_snapshot(&chunk);
                        self.streaming.save_finished(chunk);
                        step = self.streaming.resume_plan_update(&mut self.world, resume);
                    }
                }
            }
        });
        let (commit, protos) = self.streaming.prepare_commit(&mut self.world, plan);
        self.native_proto_snapshots(&protos);
        self.streaming.submit_protos(protos);
        let (uploads, mut unloaded) = horizon_client::spans::time("streaming", || {
            self.streaming.complete_commit(&mut self.world, commit)
        });
        unloaded.extend(java_hooks::java_hook_pumped_unloads());
        self.streaming.extra.truncate(areas);
        // Java's unload marks what it hadn't saved before the save; its `ChunkEvent.Unload` follows the save.
        let loaded: Vec<(i32, i32)> = self.world.loaded_chunks.iter().map(|&(x, z, _)| (x, z)).collect();
        self.java_hook_chunks_changed(&loaded, &unloaded);
        let dirty: Vec<_> = unloaded
            .iter()
            .filter(|c| self.world.modified.remove(&(c.x, c.z)))
            .cloned()
            .collect();
        let modified = horizon_client::spans::time("with_entities", || self.with_entities(&dirty));
        self.native_full_chunk_snapshots(&modified);
        let written = self.java_hook_chunks_saving(&modified);
        if !modified.is_empty()
            && let Err(e) = horizon_client::spans::time("save_chunks", || {
                self.save.save_later_written(
                    &modified,
                    self.pack.assets.registry,
                    world_biomes!(self),
                    written,
                )
            })
        {
            log::warn!("Horizon: failed to save unloaded chunks: {e}");
        }
        self.spawn_fresh_chunks();
        // Vanilla saves the chunk before its unload hook and drops its block entities afterwards.
        self.native_chunks_loaded();
        self.native_chunks_unloaded(&unloaded);
        for chunk in &unloaded {
            self.world.modified.unloaded(&(chunk.x, chunk.z));
        }
        (uploads, unloaded)
    }
}
