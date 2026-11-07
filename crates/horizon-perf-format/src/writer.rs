use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};

use crate::gzip::GzipWriter;

pub struct PerfWriter {
    tx: Option<SyncSender<Vec<u8>>>,
    dropped: Arc<AtomicU64>,
    handle: Option<JoinHandle<()>>,
    _path: PathBuf,
}

impl PerfWriter {
    pub fn new(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let path2 = path.clone();
        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(4096);
        let dropped = Arc::new(AtomicU64::new(0));
        let handle = thread::Builder::new()
            .name("horizon-perf-writer".to_string())
            .spawn(move || {
                let mut writer = match GzipWriter::new(&path2) {
                    Ok(w) => w,
                    Err(_) => return,
                };
                while let Ok(bytes) = rx.recv() {
                    let _ = writer.write_field_bytes(&bytes);
                }
                let _ = writer.finish();
            })
            .map_err(std::io::Error::other)?;
        Ok(Self {
            tx: Some(tx),
            dropped,
            handle: Some(handle),
            _path: path,
        })
    }

    pub fn send(&self, bytes: Vec<u8>) -> bool {
        match &self.tx {
            None => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
            Some(tx) => match tx.try_send(bytes) {
                Ok(()) => true,
                Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                    false
                }
            },
        }
    }

    pub fn try_send(&self, bytes: Vec<u8>) -> bool {
        self.send(bytes)
    }

    pub fn dropped_messages(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    pub fn finish(&mut self) -> std::io::Result<()> {
        if let Some(tx) = self.tx.take() {
            drop(tx);
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        Ok(())
    }
}

impl Drop for PerfWriter {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            drop(tx);
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
