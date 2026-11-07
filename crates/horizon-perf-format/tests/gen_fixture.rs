use horizon_perf_format::{Capture, GzipWriter};
use prost::Message;

#[test]
#[ignore]
fn gen_fixture() {
    let c = Capture {
        format: 1,
        ..Default::default()
    };
    let bytes = c.encode_to_vec();
    let path = std::path::Path::new("tests/fixtures/v1.hzperf");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut w = GzipWriter::new(path).unwrap();
    w.write_field_bytes(&bytes).unwrap();
    w.finish().unwrap();
}
