//! `HORIZON_PROFILE=<dir>`: the playtest profiler's shared pieces: JFR options, per-call timing and the log line format.
use std::{
    cell::Cell,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Duration,
};

/// Rows each log line keeps.
pub const TOP: usize = 15;

/// Count, total and slowest of one kind of work.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Stat {
    pub count: u64,
    pub total_ns: u64,
    pub max_ns: u64,
}

impl Stat {
    pub fn add(&mut self, ns: u64) {
        self.count += 1;
        self.total_ns += ns;
        self.max_ns = self.max_ns.max(ns);
    }
}

/// The folder the profile goes to, when `HORIZON_PROFILE` names one.
pub fn dir() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = PathBuf::from(std::env::var_os("HORIZON_PROFILE").filter(|d| !d.is_empty())?);
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir.canonicalize().unwrap_or(dir))
    })
    .as_deref()
}

/// The JVM options of a continuous recording that dumps into the profile folder when the JVM exits.
pub fn jfr_options(dir: &Path) -> Vec<String> {
    vec![format!(
        "-XX:StartFlightRecording=name=horizon,settings=profile,maxsize=2g,maxage=6h,dumponexit=true,filename={}",
        dir.join("exit.jfr").display()
    )]
}

static CALLS: Mutex<Vec<(String, Stat)>> = Mutex::new(Vec::new());

thread_local! {
    static THREAD_NS: Cell<u64> = const { Cell::new(0) };
}

/// One `Host::request` of `op` that took `took`, on this thread.
pub fn record_call(op: &str, took: Duration) {
    let ns = took.as_nanos() as u64;
    THREAD_NS.set(THREAD_NS.get() + ns);
    let mut calls = CALLS.lock().unwrap_or_else(|e| e.into_inner());
    match calls.iter_mut().find(|(name, _)| name == op) {
        Some((_, stat)) => stat.add(ns),
        None => {
            let mut stat = Stat::default();
            stat.add(ns);
            calls.push((op.to_owned(), stat));
        }
    }
}

/// The nanoseconds this thread spent in `Host::request` since the last take.
pub fn take_thread_ns() -> u64 {
    THREAD_NS.replace(0)
}

/// Every thread's calls since the last take.
pub fn take_calls() -> Vec<(String, Stat)> {
    std::mem::take(&mut *CALLS.lock().unwrap_or_else(|e| e.into_inner()))
}

/// `name|count|total_us|max_us` rows, slowest total first, comma separated; `-` when empty.
pub fn format_stats<N: AsRef<str>>(rows: impl IntoIterator<Item = (N, Stat)>) -> String {
    let mut rows: Vec<(N, Stat)> = rows.into_iter().collect();
    rows.sort_by_key(|a| std::cmp::Reverse(a.1.total_ns));
    let text: Vec<String> = rows
        .iter()
        .take(TOP)
        .map(|(name, s)| {
            format!(
                "{}|{}|{}|{}",
                name.as_ref(),
                s.count,
                s.total_ns / 1000,
                s.max_ns / 1000
            )
        })
        .collect();
    if text.is_empty() {
        "-".into()
    } else {
        text.join(",")
    }
}

/// Rows read back from `format_stats` text.
pub fn parse_stats(text: &str) -> HashMap<String, Stat> {
    text.split(',')
        .filter_map(|row| {
            let mut f = row.split('|');
            let name = f.next()?.to_owned();
            let mut n = || f.next()?.parse::<u64>().ok();
            let (count, total, max) = (n()?, n()?, n()?);
            Some((
                name,
                Stat {
                    count,
                    total_ns: total * 1000,
                    max_ns: max * 1000,
                },
            ))
        })
        .collect()
}

/// One line of `java-ticks.log`; `java` is the JVM's `be=... lock=...` text.
pub fn java_line(unix_ms: u128, calls: Vec<(String, Stat)>, java: &str) -> String {
    format!("t={unix_ms} calls={} {java}", format_stats(calls))
}

/// Appends a line to a log in the profile folder.
pub fn append(dir: &Path, file: &str, line: &str) {
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

    fn stat(count: u64, total_us: u64, max_us: u64) -> Stat {
        Stat {
            count,
            total_ns: total_us * 1000,
            max_ns: max_us * 1000,
        }
    }

    #[test]
    fn rows_sort_by_total_and_round_trip() {
        let text = format_stats([("a", stat(1, 10, 10)), ("b:c", stat(4, 90, 40))]);
        assert_eq!(text, "b:c|4|90|40,a|1|10|10");
        let back = parse_stats(&text);
        assert_eq!(back["b:c"], stat(4, 90, 40));
        assert_eq!(format_stats(Vec::<(String, Stat)>::new()), "-");
        assert!(parse_stats("-").is_empty());
    }

    #[test]
    fn a_line_keeps_only_the_top_rows() {
        let rows: Vec<(String, Stat)> = (0..40).map(|i| (format!("t{i}"), stat(1, i, i))).collect();
        assert_eq!(format_stats(rows).split(',').count(), TOP);
    }

    #[test]
    fn stats_track_count_total_and_max() {
        let mut s = Stat::default();
        s.add(5);
        s.add(9);
        assert_eq!((s.count, s.total_ns, s.max_ns), (2, 14, 9));
    }

    #[test]
    fn the_recording_dumps_into_the_folder() {
        let o = jfr_options(Path::new("/p"));
        assert!(o[0].contains("settings=profile") && o[0].contains("filename=/p/exit.jfr"));
        assert!(o[0].contains("maxsize=2g") && o[0].contains("maxage=6h"));
    }

    #[test]
    fn a_java_line_has_the_call_rows_then_the_jvms() {
        let line = java_line(5, vec![("x".into(), stat(2, 3, 2))], "be=- lock=-");
        assert_eq!(line, "t=5 calls=x|2|3|2 be=- lock=-");
    }
}
