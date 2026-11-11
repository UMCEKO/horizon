//! The dedicated server's `horizon-report` on the game: the recording and the report's numbers live in `horizon_gameplay::dedicated::report`.
use super::super::{Dim, Game, tick_profile};
use horizon_client::streaming::job_stats;
pub(super) use horizon_gameplay::dedicated::report::{
    MAX_SECONDS, Recording, TRACE_MAX_SECONDS, start_trace,
};
use horizon_gameplay::dedicated::report::{
    mob_costs, network, overhead, process_limits, round, span_cost_nanos, summary, tick_summary,
    worker_jobs, write_report,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

impl Game<'_> {
    /// The level in place and the parked ones: their chunks and their entities by type.
    fn report_levels(&self) -> Value {
        let mut levels = Vec::new();
        let here = |dim: Dim,
                    world: &horizon_client::world::World,
                    mobs: &[horizon_client::mobs::Mob]| {
            let mut by_type: BTreeMap<&str, usize> = BTreeMap::new();
            for m in mobs.iter().filter(|m| !m.removed) {
                *by_type.entry(&m.kind.id).or_default() += 1;
            }
            let mut by_type: Vec<_> = by_type.into_iter().collect();
            by_type.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            json!({
                "dimension": dim.id(),
                "chunks": world.chunks.len(),
                "block_entities": world.block_entities.len(),
                "mobs": mobs.iter().filter(|m| !m.removed).count(),
                "mobs_by_type": by_type.into_iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
            })
        };
        let mut current = here(self.dim, &self.world, &self.mobs);
        current["item_entities"] = json!(self.dropped.len());
        current["experience_orbs"] = json!(self.orbs.len());
        current["projectiles"] = json!(self.creatures.projectiles.len());
        current["minecarts"] = json!(self.carts.len());
        current["boats"] = json!(self.boats.len());
        levels.push(current);
        for dim in self.levels.parked_dims() {
            if let (Some(world), Some(mobs)) =
                (self.levels.parked_world(dim), self.levels.parked_mobs(dim))
            {
                let mut level = here(dim, world, mobs);
                if let Some([items, orbs, projectiles, carts, boats]) =
                    self.levels.parked_entity_counts(dim)
                {
                    level["item_entities"] = json!(items);
                    level["experience_orbs"] = json!(orbs);
                    level["projectiles"] = json!(projectiles);
                    level["minecarts"] = json!(carts);
                    level["boats"] = json!(boats);
                }
                levels.push(level);
            }
        }
        Value::Array(levels)
    }

    /// Each player's wire bytes (in, out) by connection.
    pub(super) fn wire_bytes(&self) -> Vec<(u32, (u64, u64))> {
        self.lan.as_ref().map_or_else(Vec::new, |h| {
            h.hub
                .guests
                .iter()
                .filter_map(|g| Some((g.conn, g.sender.wire_bytes()?)))
                .collect()
        })
    }

    /// The report of a finished recording: written to `reports/` in the server's folder, its summary logged.
    pub(super) fn finish_report(&mut self, r: Recording, view: (u32, u32), max_players: usize) {
        let span_ns = span_cost_nanos();
        let mob_ns = tick_profile::sample_cost_nanos();
        // Off unless `HORIZON_SPIKES` or a mod (`native_spans`) still wants them.
        tick_profile::want_spans(tick_profile::SPANS_REPORT, false);
        job_stats::enable(false);
        let window_s = r.started.elapsed().as_secs_f64();
        let (ticks, spans, worst) = tick_summary(&r, window_s);
        let costs = tick_profile::take();
        let mob_samples = costs.iter().map(|c| c.mob_ticks).sum();
        let jobs = job_stats::take();
        let cpu = horizon_bench::cpu_seconds(std::process::id())
            .zip(r.cpu_s)
            .map(|(end, start)| round((end - start) / window_s.max(1e-9)));
        let players = self.lan.as_ref().map_or(0, |h| h.hub.guests.len());
        let report = json!({
            "format": 2,
            "horizon": horizon_client::crash::version(),
            "minecraft": horizon_core::VANILLA_VERSION,
            "window_s": round(window_s),
            "ticks": ticks,
            "spans": spans,
            "worst_ticks": worst,
            "mob_costs": mob_costs(&costs, r.sampled_ticks),
            "mob_sampled_ticks": r.sampled_ticks,
            "chunk_workers": worker_jobs(&jobs, window_s),
            "network": network(&r.wire_start, &self.wire_bytes(), window_s),
            "overhead": overhead(&r, mob_samples, span_ns, mob_ns),
            "players": players,
            "levels": self.report_levels(),
            "memory": { "rss_mb": horizon_bench::rss_bytes(std::process::id()).map(|b| b >> 20) },
            "process": process_limits(),
            "cpu_cores_used": cpu,
            "config": {
                "view_distance": view.0,
                "simulation_distance": view.1,
                "max_players": max_players,
            },
        });
        let text = summary(&report);
        // The machine's description runs commands: off the tick.
        std::thread::spawn(move || {
            let mut report = report;
            let hw = horizon_bench::Hardware::detect();
            report["machine"] = json!({
                "os": hw.os, "arch": hw.arch, "cpu": hw.cpu, "cpu_threads": hw.cpu_threads, "ram_mb": hw.ram_mb,
            });
            match write_report(Path::new("reports"), &report, &text) {
                Ok(path) => log::info!(
                    "Horizon: performance report written to {}\n{text}",
                    path.display()
                ),
                Err(e) => log::error!("Horizon: could not write the performance report: {e}"),
            }
        });
    }
}
