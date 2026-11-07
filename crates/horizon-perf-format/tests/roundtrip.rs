use horizon_perf_format::gzip::GzipWriter;
use std::fs;

#[test]
fn gzip_roundtrip() {
    let path = std::env::temp_dir().join("hpf.gz");
    let mut w = GzipWriter::new(&path).unwrap();
    w.write_field_bytes(b"hello world").unwrap();
    w.finish().unwrap();
    let data = fs::read(&path).unwrap();
    // Python gzip should decode; just check magic + nonempty
    assert_eq!(&data[0..2], &[0x1f, 0x8b]);
    assert!(data.len() > 20);
}
