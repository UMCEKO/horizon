use crate::{Capture, stats};

pub fn health_only(cap: &Capture) -> String {
    let mut out = String::new();
    out.push_str("horizon-perf summary (health only)\n");
    out.push_str(&format!("format: {}\n", cap.format));
    for s in &cap.start {
        out.push_str(&format!(
            "start side={:?} started_unix_ms={}\n",
            s.side, s.started_unix_ms
        ));
    }
    out.push_str(&format!("health samples: {}\n", cap.health.len()));
    for h in &cap.health {
        out.push_str(&format!(
            "  t={:.3} side={:?} rss_mb={:.1} cpu_process_cores={:.2}\n",
            h.t, h.side, h.rss_mb, h.cpu_process_cores
        ));
    }
    for s in &cap.stop {
        out.push_str(&format!("stop side={:?} t={:.3}\n", s.side, s.t));
    }
    out
}

pub fn summary(cap: &Capture) -> String {
    health_only(cap)
}
pub fn summarize(cap: &Capture) -> String {
    health_only(cap)
}

pub fn health_only_with_baseline(cap: &Capture, baseline: Option<&Capture>) -> String {
    let mut out = health_only(cap);
    if let Some(b) = baseline {
        out.push_str(&format!(
            "baseline: format={} health_samples={}\n",
            b.format,
            b.health.len()
        ));
        let d = stats::diff(cap, b);
        if d.drift_a {
            out.push_str("warning: drifting baseline window\n");
        }
        if !d.rows.is_empty() {
            out.push_str("baseline diff:\n");
            for r in &d.rows {
                out.push_str(&format!(
                    "  {} {} delta={:.3} ({}, {:.3}..{:.3})\n",
                    r.name, r.key, r.effect, r.verdict, r.lo, r.hi
                ));
            }
        }
    }
    out
}

pub fn summary_with_baseline(cap: &Capture, baseline: Option<&Capture>) -> String {
    health_only_with_baseline(cap, baseline)
}
pub fn summary_baseline(cap: &Capture, baseline: &Capture) -> String {
    health_only_with_baseline(cap, Some(baseline))
}
pub fn summary_with_baseline_capture(cap: &Capture, baseline: &Capture) -> String {
    health_only_with_baseline(cap, Some(baseline))
}
