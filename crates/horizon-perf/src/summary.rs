use crate::Capture;

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
