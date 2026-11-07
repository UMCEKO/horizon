use std::collections::VecDeque;

use crate::horizon::perf::v1::HealthSample;
use crate::SlowEvent;

#[derive(Debug, Clone, Default)]
pub struct HealthTier {
    pub step_s: f64,
    pub capacity: usize,
    pub samples: VecDeque<HealthSample>,
    #[doc(hidden)]
    pub pending: Vec<HealthSample>,
}

impl HealthTier {
    pub fn new(step_s: f64, capacity: usize) -> Self {
        Self {
            step_s,
            capacity,
            samples: VecDeque::with_capacity(capacity),
            pending: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
    pub fn cap(&self) -> usize {
        self.capacity
    }

    pub fn mean_field(&self, field: &str) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let mut s = 0.0f64;
        let mut n = 0usize;
        for smp in &self.samples {
            if let Some(v) = field_value(smp, field) {
                s += v;
                n += 1;
            }
        }
        if n == 0 { 0.0 } else { s / n as f64 }
    }

    pub fn mean_mspt(&self) -> f64 { self.mean_field("mspt_avg") }
    pub fn mean_mspt_avg(&self) -> f64 { self.mean_field("mspt_avg") }
    pub fn mean_mspt_p50(&self) -> f64 { self.mean_field("mspt_p50") }
    pub fn mean_mspt_p95(&self) -> f64 { self.mean_field("mspt_p95") }
    pub fn mean_mspt_max(&self) -> f64 { self.mean_field("mspt_max") }
    pub fn mean_tps(&self) -> f64 { self.mean_field("tps") }
    pub fn mean_fps(&self) -> f64 { self.mean_field("fps") }
    pub fn mean_cpu_process_cores(&self) -> f64 { self.mean_field("cpu_process_cores") }
    pub fn mean_cpu_system_cores(&self) -> f64 { self.mean_field("cpu_system_cores") }
    pub fn mean_rss_mb(&self) -> f64 { self.mean_field("rss_mb") }

    pub fn mean(&self, f: impl Fn(&HealthSample) -> Option<f64>) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let mut s = 0.0f64;
        let mut n = 0usize;
        for smp in &self.samples {
            if let Some(v) = f(smp) {
                s += v;
                n += 1;
            }
        }
        if n == 0 { 0.0 } else { s / n as f64 }
    }
}

fn field_value(s: &HealthSample, field: &str) -> Option<f64> {
    match field {
        "tps" => s.tps.map(|x| x as f64),
        "mspt_avg" => s.mspt_avg.map(|x| x as f64),
        "mspt_p50" => s.mspt_p50.map(|x| x as f64),
        "mspt_p95" => s.mspt_p95.map(|x| x as f64),
        "mspt_max" => s.mspt_max.map(|x| x as f64),
        "fps" => s.fps.map(|x| x as f64),
        "frame_ms_p50" => s.frame_ms_p50.map(|x| x as f64),
        "frame_ms_p95" => s.frame_ms_p95.map(|x| x as f64),
        "frame_ms_p99" => s.frame_ms_p99.map(|x| x as f64),
        "frame_ms_max" => s.frame_ms_max.map(|x| x as f64),
        "frame_cpu_ms_p50" => s.frame_cpu_ms_p50.map(|x| x as f64),
        "frame_gpu_ms_p50" => s.frame_gpu_ms_p50.map(|x| x as f64),
        "cpu_process_cores" => Some(s.cpu_process_cores as f64),
        "cpu_system_cores" => Some(s.cpu_system_cores as f64),
        "rss_mb" => Some(s.rss_mb as f64),
        "physical_used_mb" => Some(s.physical_used_mb as f64),
        "swap_used_mb" => Some(s.swap_used_mb as f64),
        "allocator_commit_mb" => s.allocator_commit_mb.map(|x| x as f64),
        "allocs_per_s" => s.allocs_per_s.map(|x| x as f64),
        "alloc_mib_per_s" => s.alloc_mib_per_s.map(|x| x as f64),
        "players" => Some(s.players as f64),
        "chunks" => Some(s.chunks as f64),
        "entities" => Some(s.entities as f64),
        _ => None,
    }
}

#[allow(clippy::field_reassign_with_default)]
fn avg_samples(samples: &[HealthSample]) -> HealthSample {
    let mut out = HealthSample::default();
    fn avg_opt(samples: &[HealthSample], f: impl Fn(&HealthSample) -> Option<f32>) -> Option<f32> {
        let mut s = 0.0f64;
        let mut n = 0usize;
        for smp in samples {
            if let Some(v) = f(smp) {
                s += v as f64;
                n += 1;
            }
        }
        if n == 0 { None } else { Some((s / n as f64) as f32) }
    }
    out.tps = avg_opt(samples, |s| s.tps);
    out.mspt_avg = avg_opt(samples, |s| s.mspt_avg);
    out.mspt_p50 = avg_opt(samples, |s| s.mspt_p50);
    out.mspt_p95 = avg_opt(samples, |s| s.mspt_p95);
    out.mspt_max = avg_opt(samples, |s| s.mspt_max);
    out.fps = avg_opt(samples, |s| s.fps);
    out.frame_ms_p50 = avg_opt(samples, |s| s.frame_ms_p50);
    out.frame_ms_p95 = avg_opt(samples, |s| s.frame_ms_p95);
    out.frame_ms_p99 = avg_opt(samples, |s| s.frame_ms_p99);
    out.frame_ms_max = avg_opt(samples, |s| s.frame_ms_max);
    out.frame_cpu_ms_p50 = avg_opt(samples, |s| s.frame_cpu_ms_p50);
    out.frame_gpu_ms_p50 = avg_opt(samples, |s| s.frame_gpu_ms_p50);
    out.allocator_commit_mb = avg_opt(samples, |s| s.allocator_commit_mb);
    out.allocs_per_s = avg_opt(samples, |s| s.allocs_per_s);
    out.alloc_mib_per_s = avg_opt(samples, |s| s.alloc_mib_per_s);
    let mut cpu_p = 0.0f64;
    let mut cpu_s = 0.0f64;
    let mut rss = 0.0f64;
    let mut phys = 0.0f64;
    let mut swap = 0.0f64;
    let mut players = 0.0f64;
    let mut chunks = 0.0f64;
    let mut ents = 0.0f64;
    let len = samples.len().max(1) as f64;
    for s in samples {
        cpu_p += s.cpu_process_cores as f64;
        cpu_s += s.cpu_system_cores as f64;
        rss += s.rss_mb as f64;
        phys += s.physical_used_mb as f64;
        swap += s.swap_used_mb as f64;
        players += s.players as f64;
        chunks += s.chunks as f64;
        ents += s.entities as f64;
    }
    out.cpu_process_cores = (cpu_p / len) as f32;
    out.cpu_system_cores = (cpu_s / len) as f32;
    out.rss_mb = (rss / len) as f32;
    out.physical_used_mb = (phys / len) as f32;
    out.swap_used_mb = (swap / len) as f32;
    out.players = (players / len) as u32;
    out.chunks = (chunks / len) as u32;
    out.entities = (ents / len) as u32;
    if let Some(last) = samples.last() {
        out.t = last.t;
        out.side = last.side;
    }
    out
}

#[derive(Debug, Clone)]
pub struct HealthRecorder {
    pub tiers: Vec<HealthTier>,
    pub windows: Vec<HealthTier>,
    pub history: Vec<HealthTier>,
    pub slow: VecDeque<SlowEvent>,
    pub slow_ring: VecDeque<SlowEvent>,
    pub slow_events: VecDeque<SlowEvent>,
    pub slow_history: VecDeque<SlowEvent>,
}

const STEPS: [usize; 4] = [1, 10, 60, 900];
const CAPS: [usize; 4] = [600, 360, 1440, 672];

impl HealthRecorder {
    pub fn new() -> Self {
        let mut tiers = Vec::with_capacity(4);
        for i in 0..4 {
            tiers.push(HealthTier::new(STEPS[i] as f64, CAPS[i]));
        }
        Self {
            tiers: tiers.clone(),
            windows: tiers.clone(),
            history: tiers,
            slow: VecDeque::with_capacity(256),
            slow_ring: VecDeque::with_capacity(256),
            slow_events: VecDeque::with_capacity(256),
            slow_history: VecDeque::with_capacity(256),
        }
    }

    fn sync_tiers(&mut self) {
        self.windows = self.tiers.clone();
        self.history = self.tiers.clone();
    }

    fn push_tier_sample(&mut self, tier_idx: usize, sample: HealthSample) {
        let tier = &mut self.tiers[tier_idx];
        if tier.samples.len() >= tier.capacity {
            tier.samples.pop_front();
        }
        tier.samples.push_back(sample);
    }

    #[allow(clippy::needless_range_loop)]
    pub fn record(&mut self, sample: HealthSample) {
        self.push_tier_sample(0, sample);
        for ti in 1..4 {
            let step = STEPS[ti];
            let tier = &mut self.tiers[ti];
            tier.pending.push(sample);
            if tier.pending.len() >= step {
                let avg = avg_samples(&tier.pending);
                if tier.samples.len() >= tier.capacity {
                    tier.samples.pop_front();
                }
                tier.samples.push_back(avg);
                tier.pending.clear();
            }
        }
        self.sync_tiers();
    }

    pub fn record_sample(&mut self, sample: HealthSample) { self.record(sample) }
    pub fn observe(&mut self, sample: HealthSample) { self.record(sample) }
    pub fn add_sample(&mut self, sample: HealthSample) { self.record(sample) }
    pub fn push_sample(&mut self, sample: HealthSample) { self.record(sample) }
    pub fn record_sample_ref(&mut self, sample: &HealthSample) { self.record(*sample) }
    pub fn record_health_sample(&mut self, sample: HealthSample) { self.record(sample) }

    pub fn record_tick(&mut self, mspt: f64) {
        let s = HealthSample {
            mspt_avg: Some(mspt as f32),
            tps: Some((1000.0 / mspt.max(1.0)) as f32),
            ..Default::default()
        };
        self.record(s);
    }
    pub fn record_tick_mspt(&mut self, mspt: f64) { self.record_tick(mspt) }
    pub fn record_frame(&mut self, frame_ms: f64) {
        let s = HealthSample {
            frame_ms_p50: Some(frame_ms as f32),
            ..Default::default()
        };
        self.record(s);
    }

    pub fn tier(&self, i: usize) -> &HealthTier { &self.tiers[i] }
    pub fn tier_mut(&mut self, i: usize) -> &mut HealthTier { &mut self.tiers[i] }
    pub fn tiers_slice(&self) -> &[HealthTier] { &self.tiers }

    pub fn tps_1m(&self) -> f64 { self.last_mean_tier(0, 60, "tps") }
    pub fn tps_5m(&self) -> f64 { self.last_mean_tier(0, 300, "tps") }
    pub fn tps_15m(&self) -> f64 { self.last_mean_tier(1, 90, "tps") }
    pub fn tps_1(&self) -> f64 { self.tps_1m() }
    pub fn tps_5(&self) -> f64 { self.tps_5m() }
    pub fn tps_15(&self) -> f64 { self.tps_15m() }

    fn last_mean_tier(&self, tier_idx: usize, n: usize, field: &str) -> f64 {
        let t = &self.tiers[tier_idx];
        if t.samples.is_empty() { return 0.0; }
        let take = n.min(t.samples.len());
        let skip = t.samples.len() - take;
        let mut s = 0.0f64;
        let mut cnt = 0usize;
        for (i, smp) in t.samples.iter().enumerate() {
            if i < skip { continue; }
            if let Some(v) = field_value(smp, field) {
                s += v;
                cnt += 1;
            }
        }
        if cnt == 0 { 0.0 } else { s / cnt as f64 }
    }

    pub fn mean_mspt(&self, tier: usize) -> f64 {
        self.tiers.get(tier).map(|t| t.mean_mspt()).unwrap_or(0.0)
    }
    pub fn mean_tps(&self, tier: usize) -> f64 {
        self.tiers.get(tier).map(|t| t.mean_tps()).unwrap_or(0.0)
    }

    pub fn push_slow(&mut self, e: SlowEvent) {
        if self.slow.len() >= 256 { self.slow.pop_front(); }
        self.slow.push_back(e.clone());
        if self.slow_ring.len() >= 256 { self.slow_ring.pop_front(); }
        self.slow_ring.push_back(e.clone());
        if self.slow_events.len() >= 256 { self.slow_events.pop_front(); }
        self.slow_events.push_back(e.clone());
        if self.slow_history.len() >= 256 { self.slow_history.pop_front(); }
        self.slow_history.push_back(e);
    }
    pub fn record_slow(&mut self, event: SlowEvent, threshold_ms: f64) {
        if event.wall_ms as f64 > threshold_ms {
            self.push_slow(event);
        }
    }
    pub fn record_slow_event(&mut self, event: SlowEvent) { self.push_slow(event) }
    pub fn slow_threshold_filter(&mut self, event: SlowEvent, threshold_ms: f64) {
        self.record_slow(event, threshold_ms)
    }
}

impl Default for HealthRecorder {
    fn default() -> Self { Self::new() }
}
