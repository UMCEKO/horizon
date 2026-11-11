// game-module-cfg: feature java-mods
//! `HORIZON_PROFILE=<dir>`: a line per second of the server's tick times, spans and load in `server-ticks.log`,
//! and the JVM's in `java-ticks.log`. Read by `tools/profile_report.py`.
use super::Game;
use horizon_client::spans::Span;
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Span rows each line keeps.
const TOP: usize = 15;

/// The profile folder, when `HORIZON_PROFILE` names one.
pub(super) fn dir() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = PathBuf::from(std::env::var_os("HORIZON_PROFILE").filter(|d| !d.is_empty())?);
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir.canonicalize().unwrap_or(dir))
    })
    .as_deref()
}

/// What the level in place held when a tick ended.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(super) struct Load {
    pub players: usize,
    pub chunks: usize,
    pub mobs: usize,
    pub items: usize,
    pub orbs: usize,
    pub block_entities: usize,
}

/// One second of ticks.
#[derive(Default)]
struct Window {
    ticks_ms: Vec<f32>,
    spans: HashMap<&'static str, (u64, f32, f32)>,
    java_ns: u64,
}

impl Window {
    fn add(&mut self, wall_ms: f32, spans: &[Span], java_ns: u64) {
        self.ticks_ms.push(wall_ms);
        self.java_ns += java_ns;
        for span in spans {
            let row = self.spans.entry(span.name).or_default();
            row.0 += 1;
            row.1 += span.ms;
            row.2 = row.2.max(span.ms);
        }
    }

    /// The line for this window, which it empties.
    fn line(&mut self, unix_ms: u128, load: Load) -> String {
        let mut ticks = std::mem::take(&mut self.ticks_ms);
        ticks.sort_by(f32::total_cmp);
        let total: f32 = ticks.iter().fold(0.0, |a, t| a + t);
        let mean = total / ticks.len().max(1) as f32;
        let p99 = ticks
            .get(((ticks.len() as f32 * 0.99).ceil() as usize).saturating_sub(1))
            .copied()
            .unwrap_or(0.0);
        let max = ticks.last().copied().unwrap_or(0.0);
        let java_ms = std::mem::take(&mut self.java_ns) as f64 / 1e6;
        let java_pct = if total > 0.0 {
            java_ms / f64::from(total) * 100.0
        } else {
            0.0
        };
        let mut spans: Vec<_> = std::mem::take(&mut self.spans).into_iter().collect();
        spans.sort_by(|a, b| b.1.1.total_cmp(&a.1.1));
        let spans: Vec<String> = spans
            .iter()
            .take(TOP)
            .map(|(name, (n, total, max))| {
                format!(
                    "{name}|{n}|{}|{}",
                    (total * 1000.0) as u64,
                    (max * 1000.0) as u64
                )
            })
            .collect();
        let spans = if spans.is_empty() {
            "-".into()
        } else {
            spans.join(",")
        };
        format!(
            "t={unix_ms} ticks={} mean_ms={mean:.2} max_ms={max:.2} p99_ms={p99:.2} players={} chunks={} mobs={} items={} orbs={} block_entities={} java_ms={java_ms:.2} java_pct={java_pct:.1} spans={spans}",
            ticks.len(),
            load.players,
            load.chunks,
            load.mobs,
            load.items,
            load.orbs,
            load.block_entities,
        )
    }
}

struct Profiler {
    window: Window,
    since: Instant,
}

thread_local! {
    static PROFILER: RefCell<Option<Profiler>> = const { RefCell::new(None) };
}

/// A server tick starts: the JVM time before it (console commands, startup) is not the tick's.
pub(super) fn begin_tick() {
    #[cfg(feature = "java-mods")]
    if dir().is_some() {
        horizon_java_host::profile::take_thread_ns();
    }
}

impl Game<'_> {
    /// A server tick ended: it joins this second's line, written when the second is up.
    pub(super) fn profile_tick(&mut self, wall_ms: f32, spans: &[Span]) {
        let Some(dir) = dir() else {
            return;
        };
        #[cfg(feature = "java-mods")]
        let java_ns = horizon_java_host::profile::take_thread_ns();
        #[cfg(not(feature = "java-mods"))]
        let java_ns = 0;
        let due = PROFILER.with_borrow_mut(|p| {
            let p = p.get_or_insert_with(|| Profiler {
                window: Window::default(),
                since: Instant::now(),
            });
            p.window.add(wall_ms, spans, java_ns);
            (p.since.elapsed() >= Duration::from_secs(1)).then(|| {
                p.since = Instant::now();
            })
        });
        if due.is_none() {
            return;
        }
        let load = Load {
            players: self.lan.as_ref().map_or(0, |h| h.hub.guests.len()),
            chunks: self.world.chunks.len(),
            mobs: self.mobs.len(),
            items: self.dropped.len(),
            orbs: self.orbs.len(),
            block_entities: self.world.block_entities.len(),
        };
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let line = PROFILER.with_borrow_mut(|p| p.as_mut().map(|p| p.window.line(unix_ms, load)));
        if let Some(line) = line {
            append(dir, "server-ticks.log", &line);
        }
        #[cfg(feature = "java-mods")]
        if let Some(host) = horizon_java_host::get() {
            let java = host.profile("profile_take", &[]).unwrap_or_default();
            let calls = horizon_java_host::profile::take_calls();
            let dims = horizon_java_host::profile::take_dimension_names();
            let line = horizon_java_host::profile::java_line(unix_ms, calls, dims, &java);
            horizon_java_host::profile::append(dir, "java-ticks.log", &line);
        }
    }

    /// The console's `profile dump`: the JVM's recording, written now, named by the time.
    pub(super) fn profile_dump(&mut self) -> Result<PathBuf, String> {
        let dir = dir().ok_or("HORIZON_PROFILE is not set")?;
        #[cfg(feature = "java-mods")]
        if let Some(host) = horizon_java_host::get() {
            let unix_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_millis());
            let path = dir.join(format!("dump-{unix_ms}.jfr"));
            let reply = host
                .profile("profile_dump", path.display().to_string().as_bytes())
                .ok_or("the JVM did not answer")?;
            return match reply.strip_prefix("ERROR ") {
                Some(error) => Err(error.to_owned()),
                None if reply.is_empty() => Err("no recording is running".into()),
                None => Ok(path),
            };
        }
        let _ = dir;
        Err("no JVM is running".into())
    }
}

fn append(dir: &Path, file: &str, line: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(file))
    {
        let _ = writeln!(f, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(name: &'static str, ms: f32) -> Span {
        Span {
            depth: 0,
            name,
            ms,
            cpu_ms: ms,
        }
    }

    #[test]
    fn a_line_has_the_tick_times_load_and_ranked_spans() {
        let mut w = Window::default();
        for ms in [2.0, 4.0, 40.0] {
            w.add(ms, &[span("mobs", ms / 2.0), span("lan", 0.5)], 1_000_000);
        }
        let load = Load {
            players: 12,
            chunks: 900,
            ..Default::default()
        };
        let line = w.line(77, load);
        assert!(line.starts_with(
            "t=77 ticks=3 mean_ms=15.33 max_ms=40.00 p99_ms=40.00 players=12 chunks=900 "
        ));
        assert!(line.contains("java_ms=3.00 java_pct=6.5"), "{line}");
        assert!(
            line.ends_with("spans=mobs|3|23000|20000,lan|3|1500|500"),
            "{line}"
        );
        let empty = w.line(78, load);
        assert!(
            empty.contains("ticks=0 mean_ms=0.00"),
            "a window empties: {empty}"
        );
    }

    #[test]
    fn a_window_without_spans_says_so() {
        let mut w = Window::default();
        w.add(1.0, &[], 0);
        assert!(w.line(1, Load::default()).ends_with("spans=-"));
    }
}
