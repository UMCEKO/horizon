//! `HORIZON_PROFILE=<dir>`: the playtest profiler's shared pieces: JFR options, per-call timing and the log line format.
use std::{
    cell::Cell,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
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

static DIMENSION_NAMES: AtomicU64 = AtomicU64::new(0);

/// A dimension's name crossed to Java: once per level when it is created, never per call.
pub fn record_dimension_name() {
    DIMENSION_NAMES.fetch_add(1, Ordering::Relaxed);
}

/// The dimension names sent since the last take.
pub fn take_dimension_names() -> u64 {
    DIMENSION_NAMES.swap(0, Ordering::Relaxed)
}

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

/// Runs and their entries by the call they batch: `(name, runs, entries)`.
static RUNS: Mutex<Vec<(String, u64, u64)>> = Mutex::new(Vec::new());

/// One crossing that carried `entries` entries of `op`'s work.
pub fn record_run(op: &str, entries: usize) {
    let mut runs = RUNS.lock().unwrap_or_else(|e| e.into_inner());
    match runs.iter_mut().find(|(name, ..)| name == op) {
        Some((_, n, e)) => {
            *n += 1;
            *e += entries as u64;
        }
        None => runs.push((op.to_owned(), 1, entries as u64)),
    }
}

/// Every thread's runs since the last take, as `name|runs|entries` rows; `-` when empty.
pub fn take_runs() -> String {
    let runs = std::mem::take(&mut *RUNS.lock().unwrap_or_else(|e| e.into_inner()));
    if runs.is_empty() {
        return "-".into();
    }
    runs.iter()
        .map(|(name, n, e)| format!("{name}|{n}|{e}"))
        .collect::<Vec<_>>()
        .join(",")
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

/// Which way a node of the call tree crossed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    /// A Rust to Java request (`Host::request`, `Host::call_with`).
    Request,
    /// A Java to Rust native, inside a request or on a thread of Java's own.
    Native,
}

/// One kind of crossing over a window: inclusive time counts what ran nested in it, exclusive does not.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Node {
    pub count: u64,
    pub incl_ns: u64,
    pub excl_ns: u64,
    pub max_ns: u64,
    /// Java to Rust natives entered directly inside it.
    pub callbacks: u64,
    /// Rust to Java requests made directly inside it.
    pub nested: u64,
    /// The deepest place in the tree it ran at, 1 for a root.
    pub depth: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    /// Bytes copied whole outside serialization: the request into its arena, a reply out of it, an overflow.
    pub copied: u64,
    /// Time in the request's writer and the reply's reader.
    pub ser_ns: u64,
}

impl Node {
    fn merge(&mut self, o: &Node) {
        self.count += o.count;
        self.incl_ns += o.incl_ns;
        self.excl_ns += o.excl_ns;
        self.max_ns = self.max_ns.max(o.max_ns);
        self.callbacks += o.callbacks;
        self.nested += o.nested;
        self.depth = self.depth.max(o.depth);
        self.bytes_in += o.bytes_in;
        self.bytes_out += o.bytes_out;
        self.copied += o.copied;
        self.ser_ns += o.ser_ns;
    }
}

struct Open {
    name: String,
    way: Way,
    started: std::time::Instant,
    child_ns: u64,
    node: Node,
}

thread_local! {
    static OPEN: std::cell::RefCell<Vec<Open>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The call tree over a window: its nodes by name, chains (`a>b>c`) and the highest bytes a thread's arenas and scratch held.
#[derive(Default)]
pub struct Tree {
    pub nodes: HashMap<String, Node>,
    pub chains: HashMap<String, Stat>,
    pub scratch_high: u64,
    /// The highest bytes a thread's arenas and scratch still held once a request had given back its oversize buffers.
    pub scratch_kept: u64,
}

static TREE: Mutex<Option<Tree>> = Mutex::new(None);

/// Deepest chain a `chains=` row names, counted from the call's end.
const CHAIN_DEPTH: usize = 4;

/// The call tree's view of one crossing: a node of `Way` and name, its inclusive time ending when it closes.
pub struct Crossed(());

/// Opens a node for `name` when profiling: what runs before it is dropped is nested in it.
pub fn cross(way: Way, name: &str) -> Option<Crossed> {
    dir()?;
    Some(open(way, name))
}

fn open(way: Way, name: &str) -> Crossed {
    OPEN.with_borrow_mut(|open| {
        open.push(Open {
            name: match way {
                Way::Request => name.to_owned(),
                Way::Native => format!("native:{name}"),
            },
            way,
            started: std::time::Instant::now(),
            child_ns: 0,
            node: Node::default(),
        })
    });
    Crossed(())
}

fn with_top(f: impl FnOnce(&mut Node)) {
    OPEN.with_borrow_mut(|open| {
        if let Some(top) = open.last_mut() {
            f(&mut top.node);
        }
    });
}

/// The open node's request and reply sizes.
pub fn note_bytes(sent: usize, got: usize) {
    with_top(|n| {
        n.bytes_in += sent as u64;
        n.bytes_out += got as u64;
    });
}

/// A whole copy of `bytes` the open node made.
pub fn note_copy(bytes: usize) {
    with_top(|n| n.copied += bytes as u64);
}

/// Time the open node spent writing its request or reading its reply.
pub fn note_ser(took: Duration) {
    with_top(|n| n.ser_ns += took.as_nanos() as u64);
}

/// This thread's arenas and scratch buffers now hold `bytes`: the window keeps the highest.
pub fn note_scratch(bytes: usize) {
    let mut tree = TREE.lock().unwrap_or_else(|e| e.into_inner());
    let tree = tree.get_or_insert_with(Tree::default);
    tree.scratch_high = tree.scratch_high.max(bytes as u64);
}

/// What this thread's arenas and scratch buffers keep once a request is done: the window keeps the highest.
pub fn note_scratch_kept(bytes: usize) {
    let mut tree = TREE.lock().unwrap_or_else(|e| e.into_inner());
    let tree = tree.get_or_insert_with(Tree::default);
    tree.scratch_kept = tree.scratch_kept.max(bytes as u64);
}

impl Drop for Crossed {
    fn drop(&mut self) {
        let Some((name, node, path)) = OPEN.with_borrow_mut(|open| {
            let closed = open.pop()?;
            let incl = closed.started.elapsed().as_nanos() as u64;
            if let Some(parent) = open.last_mut() {
                parent.child_ns += incl;
                match closed.way {
                    Way::Request => parent.node.nested += 1,
                    Way::Native => parent.node.callbacks += 1,
                }
            }
            let name = closed.name;
            let above = open.len().saturating_sub(CHAIN_DEPTH - 1);
            let mut path: Vec<&str> = open[above..].iter().map(|o| o.name.as_str()).collect();
            path.push(&name);
            let path = path.join(">");
            let node = Node {
                count: 1,
                incl_ns: incl,
                excl_ns: incl.saturating_sub(closed.child_ns),
                max_ns: incl,
                depth: open.len() as u64 + 1,
                ..closed.node
            };
            Some((name, node, path))
        }) else {
            return;
        };
        let mut tree = TREE.lock().unwrap_or_else(|e| e.into_inner());
        let tree = tree.get_or_insert_with(Tree::default);
        tree.nodes.entry(name).or_default().merge(&node);
        tree.chains.entry(path).or_default().add(node.incl_ns);
    }
}

/// The call tree since the last take: its nodes, its chains (`a>b>c`, up to [`CHAIN_DEPTH`] long) and the scratch high-water in bytes.
pub fn take_tree() -> Tree {
    TREE.lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .unwrap_or_default()
}

/// `name|count|incl_us|excl_us|max_us|callbacks|nested|depth|bytes_in|bytes_out|copied|ser_us` rows, most exclusive time first; `-` when empty.
pub fn format_tree(mut rows: Vec<(String, Node)>) -> String {
    rows.sort_by_key(|a| std::cmp::Reverse(a.1.excl_ns));
    let text: Vec<String> = rows
        .iter()
        .take(TOP)
        .map(|(name, n)| {
            format!(
                "{name}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                n.count,
                n.incl_ns / 1000,
                n.excl_ns / 1000,
                n.max_ns / 1000,
                n.callbacks,
                n.nested,
                n.depth,
                n.bytes_in,
                n.bytes_out,
                n.copied,
                n.ser_ns / 1000
            )
        })
        .collect();
    if text.is_empty() {
        "-".into()
    } else {
        text.join(",")
    }
}

/// One line of `java-ticks.log`; `dims` the dimension names sent, `runs` is `take_runs`'s text, `java` the JVM's `be=... lock=...` text.
pub fn java_line(
    unix_ms: u128,
    calls: Vec<(String, Stat)>,
    dims: u64,
    runs: &str,
    java: &str,
) -> String {
    format!(
        "t={unix_ms} calls={} dims={dims} runs={runs} {java}",
        format_stats(calls)
    )
}

/// [`java_line`] followed by the call tree [`take_tree`] gave: `tree=`, `chains=`, `scratch_kb=` and `scratch_kept_kb=`.
pub fn java_tree_line(
    unix_ms: u128,
    calls: Vec<(String, Stat)>,
    dims: u64,
    runs: &str,
    java: &str,
    tree: Tree,
) -> String {
    format!(
        "{} tree={} chains={} scratch_kb={} scratch_kept_kb={}",
        java_line(unix_ms, calls, dims, runs, java),
        format_tree(tree.nodes.into_iter().collect()),
        format_stats(tree.chains),
        tree.scratch_high / 1024,
        tree.scratch_kept / 1024
    )
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
    fn a_nested_crossing_counts_in_its_parents_inclusive_time_only() {
        let _ = take_tree();
        {
            let _outer = open(Way::Request, "tick");
            note_bytes(16, 4);
            note_copy(16);
            std::thread::sleep(Duration::from_millis(2));
            {
                let _cb = open(Way::Native, "get_block");
                {
                    let _inner = open(Way::Request, "states");
                    std::thread::sleep(Duration::from_millis(4));
                }
            }
            {
                let _cb = open(Way::Native, "get_block");
            }
        }
        let Tree { nodes, chains, .. } = take_tree();
        let (tick, cb, inner) = (&nodes["tick"], &nodes["native:get_block"], &nodes["states"]);
        assert_eq!(
            (tick.count, tick.callbacks, tick.nested, tick.depth),
            (1, 2, 0, 1)
        );
        assert_eq!((tick.bytes_in, tick.bytes_out, tick.copied), (16, 4, 16));
        assert_eq!((cb.count, cb.nested, cb.depth), (2, 1, 2));
        assert_eq!(inner.depth, 3);
        assert!(tick.excl_ns >= 2_000_000 && tick.incl_ns - tick.excl_ns >= 4_000_000);
        assert_eq!(tick.incl_ns - tick.excl_ns, cb.incl_ns);
        assert_eq!(cb.incl_ns - cb.excl_ns, inner.incl_ns);
        assert_eq!(chains["tick>native:get_block>states"].count, 1);
        assert_eq!(chains["tick>native:get_block"].count, 2);
        assert_eq!(chains["tick"].count, 1);
    }

    #[test]
    fn a_java_line_has_the_call_rows_then_the_jvms() {
        let line = java_line(5, vec![("x".into(), stat(2, 3, 2))], 0, "-", "be=- lock=-");
        assert_eq!(line, "t=5 calls=x|2|3|2 dims=0 runs=- be=- lock=-");
    }
}
