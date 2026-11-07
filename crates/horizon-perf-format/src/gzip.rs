use crc32fast::Hasher;
use miniz_oxide::deflate::core::CompressorOxide;
use miniz_oxide::deflate::stream::deflate;
use miniz_oxide::{MZFlush, MZStatus};
use std::fs::File;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct GzipWriter {
    file: File,
    compressor: Box<CompressorOxide>,
    crc: Hasher,
    size: u32,
    out_buf: Vec<u8>,
}

impl GzipWriter {
    pub fn new(path: impl AsRef<std::path::Path>) -> io::Result<Self> {
        let mut file = File::create(path)?;
        // RFC 1952 gzip header.
        let mtime = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as u32;
        #[allow(clippy::write_with_newline)]
        file.write_all(&[
            0x1f, 0x8b, // magic
            0x08, // deflate
            0x00, // flags
            (mtime & 0xff) as u8,
            ((mtime >> 8) & 0xff) as u8,
            ((mtime >> 16) & 0xff) as u8,
            ((mtime >> 24) & 0xff) as u8,
            0x00, // XFL
            0x03, // OS Unix
        ])?;
        file.flush()?;
        Ok(Self {
            file,
            compressor: Box::new(CompressorOxide::default()),
            crc: Hasher::new(),
            size: 0,
            out_buf: vec![0u8; 64 * 1024],
        })
    }

    pub fn write_field_bytes(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.crc.update(bytes);
        self.size = self.size.wrapping_add(bytes.len() as u32);
        // feed input entirely through the compressor, flushing sync at end
        let mut consumed = 0;
        let mut it = 0;
        while consumed < bytes.len() {
            let res = deflate(
                &mut self.compressor,
                &bytes[consumed..],
                &mut self.out_buf,
                MZFlush::None,
            );
            if let Err(e) = res.status {
                return Err(io::Error::other(format!("deflate: {e:?}")));
            }
            let bw = res.bytes_written;
            if bw > 0 {
                self.file.write_all(&self.out_buf[..bw])?;
            }
            consumed += res.bytes_consumed;
            it += 1;
            if it > 1000 { panic!("too many none iters"); }
            if res.bytes_consumed == 0 && res.bytes_written == 0 {
                break;
            }
        }
        // single sync flush
        let res = deflate(&mut self.compressor, &[], &mut self.out_buf, MZFlush::Sync);
        if let Err(e) = res.status {
            if !matches!(e, miniz_oxide::MZError::Buf) {
                return Err(io::Error::other(format!("deflate sync: {e:?}")));
            }
        }
        let bw = res.bytes_written;
        if bw > 0 {
            self.file.write_all(&self.out_buf[..bw])?;
        }
        self.file.flush()?;
        Ok(())
    }

    pub fn finish(&mut self) -> io::Result<()> {
        loop {
            let res = deflate(&mut self.compressor, &[], &mut self.out_buf, MZFlush::Finish);
            let bw = res.bytes_written;
            if bw > 0 {
                self.file.write_all(&self.out_buf[..bw])?;
            }
            if res.status == Ok(MZStatus::StreamEnd) {
                break;
            }
            if res.bytes_written == 0 && res.status.is_err() {
                break;
            }
        }
        let crc = self.crc.clone().finalize();
        self.file.write_all(&crc.to_le_bytes())?;
        self.file.write_all(&self.size.to_le_bytes())?;
        self.file.flush()?;
        Ok(())
    }
}
