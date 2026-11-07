use crate::Capture;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    TooFewBlocks,
    WithinNoise,
    Negligible,
    Slower,
    Faster,
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Verdict::TooFewBlocks => "too_few_blocks",
            Verdict::WithinNoise => "within_noise",
            Verdict::Negligible => "negligible",
            Verdict::Slower => "slower",
            Verdict::Faster => "faster",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone)]
pub struct TwoSample {
    pub effect: f64,
    pub lo: f64,
    pub hi: f64,
    pub ci_lo: f64,
    pub ci_hi: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
    pub verdict: Verdict,
    pub drift_a: bool,
    pub drift_b: bool,
    pub slope_a: f64,
    pub slope_b: f64,
}

#[derive(Debug, Clone)]
pub struct BlockDiff {
    pub key: String,
    pub name: String,
    pub a: Vec<f64>,
    pub b: Vec<f64>,
    pub effect: f64,
    pub lo: f64,
    pub hi: f64,
    pub verdict: Verdict,
}

#[derive(Debug, Clone)]
pub struct DiffResult {
    pub rows: Vec<BlockDiff>,
    pub spans: Vec<BlockDiff>,
    pub drift_a: bool,
    pub drift_b: bool,
    pub slope_a: f64,
    pub slope_b: f64,
}

fn chosen_side(cap: &Capture) -> i32 {
    if cap.start.iter().any(|s| s.side == 2) {
        2
    } else if cap.start.iter().any(|s| s.side == 1) {
        1
    } else {
        2
    }
}

fn rank_sum_counts(n: usize, m: usize) -> Vec<u64> {
    let total = n + m;
    let max_sum = n * (2 * total - n + 1) / 2;
    let mut dpp = vec![vec![0u64; max_sum + 1]; n + 1];
    dpp[0][0] = 1;
    for rank in 1..=total {
        for cnt in (1..=n).rev() {
            for s in (rank..=max_sum).rev() {
                if dpp[cnt - 1][s - rank] != 0 {
                    dpp[cnt][s] += dpp[cnt - 1][s - rank];
                }
            }
        }
    }
    dpp[n].clone()
}

fn wilcoxon_cdf_leq(n: usize, m: usize, u: i64) -> f64 {
    let counts = rank_sum_counts(n, m);
    let total_ways: u64 = counts.iter().sum();
    if total_ways == 0 {
        return 0.0;
    }
    let min_sum = (n * (n + 1) / 2) as i64;
    let w_threshold = u + min_sum;
    let mut acc = 0u64;
    for (s, c) in counts.iter().enumerate() {
        if s as i64 <= w_threshold {
            acc += c;
        }
    }
    acc as f64 / total_ways as f64
}

fn critical_u(n: usize, m: usize, alpha_half: f64) -> usize {
    let nm = n * m;
    let mut best: i64 = -1;
    for u in 0..=nm as i64 {
        if wilcoxon_cdf_leq(n, m, u) <= alpha_half {
            best = u;
        } else {
            break;
        }
    }
    if best < 0 { 0 } else { best as usize }
}

fn median_sorted(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn theil_sen(ys: &[f64]) -> f64 {
    if ys.len() < 2 {
        return 0.0;
    }
    let mut slopes = Vec::new();
    for i in 0..ys.len() {
        for j in (i + 1)..ys.len() {
            slopes.push((ys[j] - ys[i]) / (j as f64 - i as f64));
        }
    }
    median_sorted(&slopes)
}

pub fn two_sample(a: &[f64], b: &[f64]) -> TwoSample {
    let n = a.len();
    let m = b.len();
    let mut diffs: Vec<f64> = Vec::with_capacity(n * m);
    for ai in a {
        for bj in b {
            diffs.push(bj - ai);
        }
    }
    diffs.sort_by(|x, y| x.partial_cmp(y).unwrap());
    let effect = median_sorted(&diffs);

    let (lo, hi) = if n == 0 || m == 0 || diffs.is_empty() {
        (0.0, 0.0)
    } else {
        let k = critical_u(n, m, 0.025);
        let lo_idx = k.min(diffs.len().saturating_sub(1));
        let hi_idx = diffs
            .len()
            .saturating_sub(1)
            .saturating_sub(k)
            .min(diffs.len().saturating_sub(1));
        (diffs[lo_idx], diffs[hi_idx])
    };

    let verdict = if n < 6 || m < 6 {
        Verdict::TooFewBlocks
    } else if lo <= 0.0 && hi >= 0.0 {
        Verdict::WithinNoise
    } else {
        let mean_a = if n > 0 {
            a.iter().sum::<f64>() / n as f64
        } else {
            0.0
        };
        let negligible_threshold = 0.05_f64.max(0.02 * mean_a.abs());
        if effect.abs() < negligible_threshold {
            Verdict::Negligible
        } else if effect > 0.0 {
            Verdict::Slower
        } else {
            Verdict::Faster
        }
    };

    let slope_a = theil_sen(a);
    let slope_b = theil_sen(b);
    let median_a = median_sorted(a);
    let median_b = median_sorted(b);
    let window_a = n as f64;
    let window_b = m as f64;
    let drift_a =
        n >= 2 && slope_a != 0.0 && slope_a.abs() * window_a > 0.05 * median_a.abs().max(1e-9);
    let drift_b =
        m >= 2 && slope_b != 0.0 && slope_b.abs() * window_b > 0.05 * median_b.abs().max(1e-9);

    TwoSample {
        effect,
        lo,
        hi,
        ci_lo: lo,
        ci_hi: hi,
        ci_lower: lo,
        ci_upper: hi,
        verdict,
        drift_a,
        drift_b,
        slope_a,
        slope_b,
    }
}

pub fn mann_whitney_u(a: &[f64], b: &[f64]) -> f64 {
    let mut u = 0.0;
    for x in a {
        for y in b {
            if y > x {
                u += 1.0;
            } else if (y - x).abs() < f64::EPSILON {
                u += 0.5;
            }
        }
    }
    u
}

fn string_map(cap: &Capture, side: Option<i32>) -> std::collections::BTreeMap<u32, String> {
    crate::query::string_map(cap, side)
}

fn block_values(cap: &Capture, side: i32) -> std::collections::BTreeMap<String, Vec<f64>> {
    use std::collections::BTreeMap;
    let strs = string_map(cap, Some(side));
    let mut map: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for sb in &cap.span_blocks {
        if sb.side != side {
            continue;
        }
        let units = sb.units.max(1) as f64;
        let n = sb.span.len().min(sb.wall_ms.len());
        for i in 0..n {
            let name = strs
                .get(&sb.span[i])
                .cloned()
                .unwrap_or_else(|| sb.span[i].to_string());
            let v = sb.wall_ms[i] as f64 / units;
            map.entry(name).or_default().push(v);
        }
    }
    map
}

pub fn diff(a: &Capture, b: &Capture) -> DiffResult {
    let sa = chosen_side(a);
    let sb = chosen_side(b);
    let map_a = block_values(a, sa);
    let map_b = block_values(b, sb);
    let mut all: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for k in map_a.keys() {
        all.insert(k.clone());
    }
    for k in map_b.keys() {
        all.insert(k.clone());
    }
    let mut rows = Vec::new();
    for key in &all {
        let va = map_a.get(key).cloned().unwrap_or_default();
        let vb = map_b.get(key).cloned().unwrap_or_default();
        let ts = two_sample(&va, &vb);
        rows.push(BlockDiff {
            key: crate::query::span_id(key),
            name: key.clone(),
            a: va,
            b: vb,
            effect: ts.effect,
            lo: ts.lo,
            hi: ts.hi,
            verdict: ts.verdict,
        });
    }
    // drift based on first metric per capture? Use all values concatenated as mspt proxy.
    let a_vals: Vec<f64> = map_a.values().flatten().cloned().collect();
    let b_vals: Vec<f64> = map_b.values().flatten().cloned().collect();
    let tsa = two_sample(&a_vals, &b_vals);
    DiffResult {
        rows: rows.clone(),
        spans: rows,
        drift_a: tsa.drift_a,
        drift_b: tsa.drift_b,
        slope_a: tsa.slope_a,
        slope_b: tsa.slope_b,
    }
}

pub fn diff_captures(a: &Capture, b: &Capture) -> DiffResult {
    diff(a, b)
}

pub fn diff_blocks(a: &Capture, b: &Capture) -> DiffResult {
    diff(a, b)
}

pub fn drift_of(cap: &Capture) -> (bool, f64) {
    let side = chosen_side(cap);
    let map = block_values(cap, side);
    let vals: Vec<f64> = map.values().flatten().cloned().collect();
    let slope = theil_sen(&vals);
    let median = median_sorted(&vals);
    let window = vals.len() as f64;
    let drift =
        !vals.is_empty() && slope != 0.0 && slope.abs() * window > 0.05 * median.abs().max(1e-9);
    (drift, slope)
}
