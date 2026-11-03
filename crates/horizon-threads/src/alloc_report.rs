//! A measuring allocator for memory investigations (`--features alloc-report`): live bytes by thread, and every live block of 256 KiB or more by call stack, written to `HORIZON_ALLOC_REPORT` every `HORIZON_ALLOC_REPORT_SECONDS`.
use std::alloc::{GlobalAlloc, Layout};
use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

const TAGS: usize = 64;
const BIG: usize = 256 << 10;
const HEADER: usize = 16;

static LIVE: [AtomicI64; TAGS] = [const { AtomicI64::new(0) }; TAGS];
static NAMES: Mutex<[[u8; 24]; TAGS]> = Mutex::new([[0; 24]; TAGS]);
static BIGS: Mutex<Option<HashMap<usize, (usize, std::backtrace::Backtrace)>>> = Mutex::new(None);
static STARTED: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static INSIDE: Cell<bool> = const { Cell::new(false) };
    static TAG: Cell<u32> = const { Cell::new(u32::MAX) };
}

pub struct Counting;

/// The thread's name without its trailing digits, hashed into one of `TAGS` slots.
fn tag() -> u32 {
    let known = TAG.with(Cell::get);
    if known != u32::MAX {
        return known;
    }
    let thread = std::thread::current();
    let name = thread
        .name()
        .unwrap_or("unnamed")
        .trim_end_matches(|c: char| c.is_ascii_digit() || c == '-' || c == '#');
    let hash = name
        .bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33) ^ u32::from(b));
    let slot = hash % TAGS as u32;
    if let Ok(mut names) = NAMES.lock() {
        let n = name.len().min(24);
        names[slot as usize][..n].copy_from_slice(&name.as_bytes()[..n]);
    }
    TAG.with(|t| t.set(slot));
    slot
}

fn header(layout: Layout) -> usize {
    HEADER.max(layout.align())
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let offset = header(layout);
        // SAFETY: the inner allocator gets a valid layout of at least one header.
        let base = unsafe {
            crate::Allocator.alloc(Layout::from_size_align_unchecked(
                layout.size() + offset,
                layout.align().max(HEADER),
            ))
        };
        if base.is_null() {
            return base;
        }
        // SAFETY: `offset` lies within the block, and the 16 bytes before it are ours.
        let ptr = unsafe { base.add(offset) };
        let counted = !INSIDE.with(|i| i.replace(true));
        let mut slot = u32::MAX;
        if counted {
            slot = tag();
            LIVE[slot as usize].fetch_add(layout.size() as i64, Ordering::Relaxed);
            if layout.size() >= BIG
                && let Ok(mut bigs) = BIGS.lock()
            {
                bigs.get_or_insert_with(HashMap::new).insert(
                    ptr as usize,
                    (layout.size(), std::backtrace::Backtrace::force_capture()),
                );
            }
            INSIDE.with(|i| i.set(false));
        }
        // SAFETY: the header's 16 bytes are inside the block, before `ptr`.
        unsafe { (ptr.sub(16) as *mut u64).write_unaligned(u64::from(slot)) };
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let offset = header(layout);
        // SAFETY: `alloc` wrote the slot just before `ptr`.
        let slot = unsafe { (ptr.sub(16) as *const u64).read_unaligned() } as u32;
        if slot != u32::MAX {
            LIVE[slot as usize].fetch_sub(layout.size() as i64, Ordering::Relaxed);
            if layout.size() >= BIG && !INSIDE.with(|i| i.replace(true)) {
                let gone = BIGS
                    .lock()
                    .ok()
                    .and_then(|mut b| b.as_mut().and_then(|b| b.remove(&(ptr as usize))));
                drop(gone);
                INSIDE.with(|i| i.set(false));
            }
        }
        // SAFETY: the same block and layout `alloc` got from the inner allocator.
        unsafe {
            crate::Allocator.dealloc(
                ptr.sub(offset),
                Layout::from_size_align_unchecked(
                    layout.size() + offset,
                    layout.align().max(HEADER),
                ),
            )
        };
    }
}

/// Starts the report thread when `HORIZON_ALLOC_REPORT` names a file.
pub fn start() {
    let Some(path) = std::env::var_os("HORIZON_ALLOC_REPORT") else {
        return;
    };
    if STARTED.swap(1, Ordering::SeqCst) != 0 {
        return;
    }
    let every = std::env::var("HORIZON_ALLOC_REPORT_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    std::thread::Builder::new()
        .name("alloc report".into())
        .spawn(move || {
            let began = std::time::Instant::now();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(every));
                let text = report(began.elapsed().as_secs());
                let _ = std::fs::write(&path, text);
            }
        })
        .ok();
}

fn report(seconds: u64) -> String {
    let mut out = format!("after {seconds} s\n== live bytes by thread\n");
    let names = NAMES.lock().map(|n| *n).unwrap_or([[0; 24]; TAGS]);
    let mut rows: Vec<(i64, String)> = (0..TAGS)
        .map(|i| {
            (
                LIVE[i].load(Ordering::Relaxed),
                String::from_utf8_lossy(&names[i])
                    .trim_end_matches('\0')
                    .to_string(),
            )
        })
        .filter(|r| r.0 != 0)
        .collect();
    rows.sort_by_key(|r| -r.0);
    for (bytes, name) in &rows {
        let _ = writeln!(out, "{:9.1} MB  {name}", *bytes as f64 / 1e6);
    }
    let _ = writeln!(
        out,
        "{:9.1} MB  total",
        rows.iter().map(|r| r.0).sum::<i64>() as f64 / 1e6
    );
    INSIDE.with(|i| i.set(true));
    let mut stacks: HashMap<String, (usize, usize)> = HashMap::new();
    if let Ok(bigs) = BIGS.lock()
        && let Some(bigs) = bigs.as_ref()
    {
        for (size, trace) in bigs.values() {
            let text = trace.to_string();
            let frames: Vec<&str> = text
                .lines()
                .map(str::trim)
                .filter(|l| {
                    l.contains(": ")
                        && (l.contains("horizon") || l.contains("wgpu"))
                        && !l.contains("alloc_report")
                        && !l.contains("horizon_threads")
                })
                .map(|l| l.split_once(": ").map_or(l, |x| x.1))
                .take(8)
                .collect();
            let entry = stacks.entry(frames.join("\n      <- ")).or_default();
            entry.0 += size;
            entry.1 += 1;
        }
    }
    INSIDE.with(|i| i.set(false));
    let mut stacks: Vec<_> = stacks.into_iter().collect();
    stacks.sort_by_key(|s| std::cmp::Reverse(s.1.0));
    let _ = writeln!(
        out,
        "== live blocks of 256 KiB or more, by stack: {:.1} MB",
        stacks.iter().map(|s| s.1.0).sum::<usize>() as f64 / 1e6
    );
    for (stack, (bytes, count)) in stacks.iter().take(60) {
        let _ = writeln!(out, "{:9.1} MB  x{count}  {stack}", *bytes as f64 / 1e6);
    }
    out
}
