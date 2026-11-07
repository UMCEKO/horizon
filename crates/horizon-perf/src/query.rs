use horizon_perf_format::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Default)]
pub struct QueryOpts {
    pub side: Option<i32>,
    pub thread: Option<String>,
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub limit: Option<usize>,
    pub slow: bool,
    pub names: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct SpanRow {
    pub id: String,
    pub name: String,
    pub wall_ms: f64,
    pub cpu_ms: f64,
    pub waited_ms: f64,
    pub seen: u64,
}
#[derive(Debug, Clone)]
pub struct TickRow {
    pub id: String,
    pub t: f64,
    pub wall_ms: f64,
    pub cpu_ms: f64,
    pub spans: Vec<(String, f64, f64)>,
}
#[derive(Debug, Clone)]
pub struct SeriesPoint {
    pub t: f64,
    pub value: f64,
}
#[derive(Debug, Clone)]
pub struct Series {
    pub field: String,
    pub step_s: f64,
    pub points: Vec<SeriesPoint>,
}
#[derive(Debug, Clone)]
pub struct MemoryRow {
    pub id: String,
    pub path: String,
    pub value_mb: f64,
}
#[derive(Debug, Clone)]
pub struct MemoryDiffRow {
    pub id: String,
    pub path: String,
    pub grew_mb: f64,
    pub shrank_mb: f64,
    pub count: u64,
}
#[derive(Debug, Clone)]
pub struct WorldRow {
    pub dim: String,
    pub chunks: u32,
    pub block_entities: u32,
    pub entities: u32,
}
#[derive(Debug, Clone)]
pub struct NetRow {
    pub index: u32,
    pub joined_t: f64,
    pub left_t: f64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub packets_in: u64,
    pub packets_out: u64,
}
#[derive(Debug, Clone)]
pub struct MarkRow {
    pub t: f64,
    pub name: String,
}
#[derive(Debug, Clone)]
pub struct WarningRow {
    pub t: f64,
    pub code: i32,
    pub args: Vec<f64>,
}
#[derive(Debug, Clone)]
pub struct Meta {
    pub format: u32,
    pub sides: Vec<i32>,
    pub end_time: f64,
    pub players: u32,
}
fn string_map(cap: &Capture, side: Option<i32>) -> BTreeMap<u32, String> {
    let mut m = BTreeMap::new();
    for se in &cap.strings {
        if side.is_none_or(|s| se.side == s) {
            m.insert(se.id, se.text.clone());
        }
    }
    if m.is_empty() {
        for se in &cap.strings {
            m.insert(se.id, se.text.clone());
        }
    }
    m
}
fn resolve(cap: &Capture, side: Option<i32>, id: u32) -> String {
    string_map(cap, side)
        .get(&id)
        .cloned()
        .unwrap_or_else(|| id.to_string())
}
pub fn span_id(name: &str) -> String {
    format!("s:{name}")
}
pub fn function_id(name: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in name.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("f:{}", base32_8(h))
}
pub fn memory_id(path: &str) -> String {
    format!("m:{path}")
}
pub fn counter_id(name: &str) -> String {
    format!("x:{name}")
}
pub fn tick_id(counter: u64) -> String {
    format!("k:{counter}")
}
fn base32_8(v: u64) -> String {
    const A: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::new();
    let mut x = v;
    for _ in 0..8 {
        out.push(A[(x & 31) as usize] as char);
        x >>= 5;
    }
    out
}
fn chosen_side(cap: &Capture, side: Option<i32>) -> i32 {
    if let Some(s) = side {
        return s;
    }
    if cap.start.iter().any(|s| s.side == 2) {
        2
    } else if cap.start.iter().any(|s| s.side == 1) {
        1
    } else {
        2
    }
}
pub fn query_spans(cap: &Capture, opts: &QueryOpts) -> Vec<SpanRow> {
    let side = chosen_side(cap, opts.side);
    let mut rows = Vec::new();
    if opts.slow {
        let mut agg: BTreeMap<String, (f64, f64, u64)> = BTreeMap::new();
        for ev in &cap.slow {
            if ev.side != side {
                continue;
            }
            for tc in &ev.top {
                let name = resolve(cap, Some(side), tc.name);
                let e = agg.entry(name.clone()).or_insert((0.0, 0.0, 0));
                e.0 += tc.wall_ms as f64;
                e.1 += tc.cpu_ms as f64;
                e.2 += 1;
            }
        }
        for (name, (wall, cpu, seen)) in agg {
            if !opts.names.is_empty() && !opts.names.iter().any(|n| n == &name) {
                continue;
            }
            rows.push(SpanRow {
                id: span_id(&name),
                name,
                wall_ms: wall / seen.max(1) as f64,
                cpu_ms: cpu / seen.max(1) as f64,
                waited_ms: (wall - cpu) / seen.max(1) as f64,
                seen,
            });
        }
        rows.sort_by(|a, b| b.wall_ms.partial_cmp(&a.wall_ms).unwrap());
        return rows;
    }
    for ss in &cap.spans {
        if ss.side != side {
            continue;
        }
        if let Some(t) = opts.thread.as_ref() {
            let tn = resolve(cap, Some(side), ss.thread);
            if &tn != t {
                continue;
            }
        }
        for st in &ss.spans {
            let name = resolve(cap, Some(side), st.name);
            if !opts.names.is_empty() && !opts.names.iter().any(|n| n == &name) {
                continue;
            }
            rows.push(SpanRow {
                id: span_id(&name),
                name,
                wall_ms: st.wall_ms as f64 / st.seen.max(1) as f64,
                cpu_ms: st.cpu_ms as f64 / st.seen.max(1) as f64,
                waited_ms: (st.wall_ms - st.cpu_ms) as f64 / st.seen.max(1) as f64,
                seen: st.seen as u64,
            });
        }
    }
    let mut seen_ids = BTreeSet::new();
    rows.retain(|r| seen_ids.insert(r.id.clone()));
    rows.sort_by(|a, b| b.wall_ms.partial_cmp(&a.wall_ms).unwrap());
    if let Some(l) = opts.limit {
        rows.truncate(l);
    }
    rows
}
pub fn spans(cap: &Capture) -> Vec<SpanRow> {
    query_spans(cap, &QueryOpts::default())
}
pub fn query_ticks(cap: &Capture, opts: &QueryOpts) -> Vec<TickRow> {
    let side = chosen_side(cap, opts.side);
    let mut rows = Vec::new();
    for sh in &cap.slow_history {
        if sh.side != side {
            continue;
        }
        if let Some(th) = opts.to
            && sh.t > th
        {
            continue;
        }
        if let Some(fr) = opts.from
            && fr >= 0.0
            && sh.t < fr
        {
            continue;
        }
        rows.push(TickRow {
            id: tick_id(sh.counter),
            t: sh.t,
            wall_ms: sh.wall_ms as f64,
            cpu_ms: sh.cpu_ms as f64,
            spans: sh
                .top
                .iter()
                .map(|tc| {
                    (
                        resolve(cap, Some(side), tc.name),
                        tc.wall_ms as f64,
                        tc.cpu_ms as f64,
                    )
                })
                .collect(),
        });
    }
    for ev in &cap.slow {
        if ev.side != side {
            continue;
        }
        if let Some(th) = opts.to
            && ev.t > th
        {
            continue;
        }
        if let Some(fr) = opts.from
            && fr >= 0.0
            && ev.t < fr
        {
            continue;
        }
        rows.push(TickRow {
            id: tick_id(ev.counter),
            t: ev.t,
            wall_ms: ev.wall_ms as f64,
            cpu_ms: ev.cpu_ms as f64,
            spans: ev
                .top
                .iter()
                .map(|tc| {
                    (
                        resolve(cap, Some(side), tc.name),
                        tc.wall_ms as f64,
                        tc.cpu_ms as f64,
                    )
                })
                .collect(),
        });
    }
    rows.sort_by(|a, b| b.wall_ms.partial_cmp(&a.wall_ms).unwrap());
    if let Some(n) = opts.limit {
        rows.truncate(n);
    }
    rows
}
pub fn ticks(cap: &Capture) -> Vec<TickRow> {
    query_ticks(cap, &QueryOpts::default())
}
pub fn query_tick(cap: &Capture, id: &str) -> Option<TickRow> {
    ticks(cap).into_iter().find(|r| r.id == id)
}
pub fn tick(cap: &Capture, id: &str) -> Option<TickRow> {
    query_tick(cap, id)
}
pub fn series(cap: &Capture, field: &str, from: Option<f64>, to: Option<f64>) -> Series {
    let side = chosen_side(cap, None);
    let mut points = Vec::new();
    if field.starts_with("h:") {
        let fname = field.trim_start_matches("h:");
        let mut samples: Vec<&HealthSample> =
            cap.health.iter().filter(|h| h.side == side).collect();
        for hw in &cap.history {
            if hw.side == side {
                for s in &hw.samples {
                    samples.push(s);
                }
            }
        }
        for s in samples {
            if let Some(fr) = from
                && fr >= 0.0
                && s.t < fr
            {
                continue;
            }
            if let Some(t) = to
                && s.t > t
            {
                continue;
            }
            let v = match fname {
                "mspt_p95" => s.mspt_p95.unwrap_or(0.0),
                "mspt_p50" => s.mspt_p50.unwrap_or(0.0),
                "mspt_avg" => s.mspt_avg.unwrap_or(0.0),
                "mspt_max" => s.mspt_max.unwrap_or(0.0),
                "tps" => s.tps.unwrap_or(0.0),
                "fps" => s.fps.unwrap_or(0.0),
                "rss_mb" => s.rss_mb,
                _ => 0.0,
            } as f64;
            points.push(SeriesPoint { t: s.t, value: v });
        }
        points.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
        if let Some(fr) = from
            && fr < 0.0
        {
            let end = points.last().map(|p| p.t).unwrap_or(0.0);
            let thr = end + fr;
            points.retain(|p| p.t >= thr);
        }
    }
    Series {
        field: field.to_string(),
        step_s: 0.0,
        points,
    }
}
pub fn world(cap: &Capture, opts: &QueryOpts) -> Vec<WorldRow> {
    let side = chosen_side(cap, opts.side);
    let mut rows = Vec::new();
    for ws in &cap.world {
        if ws.side != side {
            continue;
        }
        for dc in &ws.dims {
            rows.push(WorldRow {
                dim: dc.dim.clone(),
                chunks: dc.chunks,
                block_entities: dc.block_entities,
                entities: dc.entities,
            });
        }
    }
    rows
}
pub fn net(cap: &Capture, opts: &QueryOpts) -> Vec<NetRow> {
    let side = chosen_side(cap, opts.side);
    let mut rows = Vec::new();
    for nt in &cap.net {
        if nt.side != side {
            continue;
        }
        for p in &nt.players {
            rows.push(NetRow {
                index: p.index,
                joined_t: p.joined_t,
                left_t: p.left_t,
                bytes_in: p.bytes_in,
                bytes_out: p.bytes_out,
                packets_in: p.packets_in,
                packets_out: p.packets_out,
            });
        }
    }
    rows
}
pub fn meta(cap: &Capture) -> Meta {
    Meta {
        format: cap.format,
        sides: cap.start.iter().map(|s| s.side).collect(),
        end_time: cap.stop.iter().map(|s| s.t).fold(0.0f64, f64::max),
        players: cap.start.iter().map(|s| s.players).max().unwrap_or(0),
    }
}
pub fn marks(cap: &Capture, opts: &QueryOpts) -> Vec<MarkRow> {
    let side = chosen_side(cap, opts.side);
    let mut rows: Vec<MarkRow> = cap
        .marks
        .iter()
        .filter(|m| m.side == side)
        .map(|m| MarkRow {
            t: m.t,
            name: m.name.clone(),
        })
        .collect();
    if let Some(fr) = opts.from {
        let fr = if fr < 0.0 {
            let end = rows.last().map(|r| r.t).unwrap_or(0.0);
            end + fr
        } else {
            fr
        };
        rows.retain(|r| r.t >= fr);
    }
    if let Some(t) = opts.to {
        rows.retain(|r| r.t <= t);
    }
    rows
}
pub fn warnings(cap: &Capture, opts: &QueryOpts) -> Vec<WarningRow> {
    let side = chosen_side(cap, opts.side);
    cap.warnings
        .iter()
        .filter(|w| w.side == side)
        .map(|w| WarningRow {
            t: w.t,
            code: w.code,
            args: w.args.clone(),
        })
        .collect()
}
pub fn memory(cap: &Capture, opts: &QueryOpts) -> Vec<MemoryRow> {
    let side = chosen_side(cap, opts.side);
    let mut rows = Vec::new();
    for ms in &cap.memory {
        if ms.side != side {
            continue;
        }
        let path = ms
            .path
            .iter()
            .map(|id| resolve(cap, Some(side), *id))
            .collect::<Vec<_>>()
            .join("/");
        let path = if path.is_empty() {
            resolve(cap, Some(side), ms.label)
        } else {
            path
        };
        if ms.value_mb.is_empty() {
            rows.push(MemoryRow {
                id: memory_id(&path),
                path,
                value_mb: 0.0,
            });
        } else {
            for v in &ms.value_mb {
                rows.push(MemoryRow {
                    id: memory_id(&path),
                    path: path.clone(),
                    value_mb: *v,
                });
            }
        }
    }
    rows
}
pub fn memory_diff(cap_a: &Capture, cap_b: &Capture) -> Vec<MemoryDiffRow> {
    let rows_a = memory(cap_a, &QueryOpts::default());
    let rows_b = memory(cap_b, &QueryOpts::default());
    let mut map: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for r in rows_a {
        map.entry(r.path.clone()).or_insert((0.0, 0.0)).0 = r.value_mb;
    }
    for r in rows_b {
        map.entry(r.path.clone()).or_insert((0.0, 0.0)).1 = r.value_mb;
    }
    let mut out = Vec::new();
    for (path, (a, b)) in map {
        let diff = b - a;
        let (grew, shrank) = if diff >= 0.0 {
            (diff, 0.0)
        } else {
            (0.0, -diff)
        };
        out.push(MemoryDiffRow {
            id: memory_id(&path),
            path,
            grew_mb: grew,
            shrank_mb: shrank,
            count: 1,
        });
    }
    out
}
pub fn summary(cap: &Capture) -> String {
    crate::summary::health_only(cap)
}
