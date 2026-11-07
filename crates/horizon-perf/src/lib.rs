use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use prost::Message;

pub use horizon_perf_format::*;

pub mod follow;
pub mod query;
pub mod stats;
pub mod summary;

pub const MAX_FORMAT: u32 = 1;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Gzip(String),
    Decode(prost::DecodeError),
    Format(u32, u32),
    NoStop(String),
    Invalid(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Gzip(s) => write!(f, "{s}"),
            Error::Decode(e) => write!(f, "{e}"),
            Error::Format(n, m) => write!(
                f,
                "this capture is format {n}; this horizon-perf reads up to {m}: update it"
            ),
            Error::NoStop(s) => write!(f, "{s}"),
            Error::Invalid(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
impl From<prost::DecodeError> for Error {
    fn from(e: prost::DecodeError) -> Self {
        Error::Decode(e)
    }
}

pub fn inflate_gzip(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    use std::io::Read;
    let mut d = GzDecoder::new(bytes);
    let mut out = Vec::new();
    d.read_to_end(&mut out)
        .map_err(|e| Error::Gzip(e.to_string()))?;
    Ok(out)
}

pub fn side_name(side: i32) -> &'static str {
    match side {
        1 => "client",
        2 => "server",
        _ => "server",
    }
}

pub fn validate_capture(cap: &horizon_perf_format::Capture) -> Result<(), Error> {
    if cap.format != MAX_FORMAT {
        return Err(Error::Format(cap.format, MAX_FORMAT));
    }
    let mut started: std::collections::BTreeSet<i32> = std::collections::BTreeSet::new();
    let mut finished: std::collections::BTreeSet<i32> = std::collections::BTreeSet::new();
    for s in &cap.start {
        started.insert(s.side);
    }
    for s in &cap.stop {
        finished.insert(s.side);
    }
    let mut to_check = started.clone();
    if to_check.is_empty() && cap.stop.is_empty() {
        to_check.insert(2);
    }
    for side in to_check {
        if !finished.contains(&side) {
            let name = side_name(side);
            return Err(Error::NoStop(format!(
                "this capture has no end for its {name} side: it is still being written or the game stopped"
            )));
        }
    }
    if started.is_empty() && finished.is_empty() && !cap.health.is_empty() && cap.stop.is_empty() {
        return Err(Error::NoStop(
            "this capture has no end for its server side: it is still being written or the game stopped".to_string(),
        ));
    }
    Ok(())
}

pub fn read_bytes(bytes: &[u8]) -> Result<horizon_perf_format::Capture, Error> {
    let inflated = inflate_gzip(bytes)?;
    let cap = horizon_perf_format::Capture::decode(inflated.as_slice())?;
    validate_capture(&cap)?;
    Ok(cap)
}

pub fn read_file(path: impl AsRef<Path>) -> Result<horizon_perf_format::Capture, Error> {
    let bytes = fs::read(path.as_ref())?;
    read_bytes(&bytes)
}

pub mod read {
    pub use super::{read_bytes, read_file};
    pub fn read(path: impl AsRef<std::path::Path>) -> Result<super::Capture, super::Error> {
        super::read_file(path)
    }
}

pub use follow::FollowReader;

pub type CaptureSide = horizon_perf_format::Side;

#[derive(Debug, Clone)]
pub struct CaptureSummary {
    pub path: PathBuf,
    pub format: u32,
    pub finished: bool,
    pub sides: Vec<CaptureSide>,
    pub end_time: f64,
    pub size: u64,
}

impl fmt::Display for CaptureSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}\tformat={}\t{}",
            self.path.display(),
            self.format,
            if self.finished {
                "finished"
            } else {
                "unfinished"
            }
        )
    }
}

pub fn is_finished(path: &Path) -> bool {
    read_file(path).is_ok()
}

pub fn list_entries(dir: impl AsRef<Path>) -> Result<Vec<CaptureSummary>, Error> {
    let mut out = Vec::new();
    let entries = fs::read_dir(dir.as_ref())?;
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let meta = fs::metadata(&p).ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        match read_file(&p) {
            Ok(cap) => {
                let mut sides: Vec<CaptureSide> = cap
                    .start
                    .iter()
                    .map(|s| CaptureSide::try_from(s.side).unwrap_or(CaptureSide::Unspecified))
                    .collect();
                sides.sort();
                sides.dedup();
                let end_time = cap.stop.iter().map(|s| s.t).fold(0.0f64, f64::max);
                out.push(CaptureSummary {
                    path: p,
                    format: cap.format,
                    finished: true,
                    sides,
                    end_time,
                    size,
                });
            }
            Err(_) => {
                out.push(CaptureSummary {
                    path: p,
                    format: 0,
                    finished: false,
                    sides: Vec::new(),
                    end_time: 0.0,
                    size,
                });
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

pub fn list(dir: impl AsRef<Path>) -> Result<Vec<String>, Error> {
    let entries = list_entries(dir)?;
    Ok(entries.iter().map(|e| e.to_string()).collect())
}

pub fn latest(dir: impl AsRef<Path>) -> Result<Option<PathBuf>, Error> {
    let entries = list_entries(dir)?;
    let mut finished: Vec<&CaptureSummary> = entries.iter().filter(|e| e.finished).collect();
    if finished.is_empty() {
        return Ok(None);
    }
    finished.sort_by(|a, b| {
        a.end_time
            .partial_cmp(&b.end_time)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let ma = fs::metadata(&a.path).and_then(|m| m.modified()).ok();
                let mb = fs::metadata(&b.path).and_then(|m| m.modified()).ok();
                ma.cmp(&mb)
            })
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(Some(finished.last().unwrap().path.clone()))
}

pub fn summary(cap: &horizon_perf_format::Capture) -> String {
    summary::health_only(cap)
}
pub fn summary_with_baseline(
    cap: &horizon_perf_format::Capture,
    baseline: Option<&horizon_perf_format::Capture>,
) -> String {
    summary::health_only_with_baseline(cap, baseline)
}
pub fn summary_baseline(
    cap: &horizon_perf_format::Capture,
    baseline: &horizon_perf_format::Capture,
) -> String {
    summary::health_only_with_baseline(cap, Some(baseline))
}

pub use follow::Field;
pub use query::*;
