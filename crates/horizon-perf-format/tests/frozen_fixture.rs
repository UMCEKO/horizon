use horizon_perf_format::Capture;
use prost::Message;

#[test]
fn frozen_fixture_decodes() {
    let data = std::fs::read("tests/fixtures/v1.hzperf").expect("fixture present");
    assert_eq!(&data[0..2], &[0x1f, 0x8b]);
    // strip gzip header (10 bytes) and trailer (8 bytes); inflate raw deflate
    let raw = miniz_oxide::inflate::decompress_to_vec(&data[10..data.len() - 8]).expect("inflate");
    let c = Capture::decode(raw.as_slice()).expect("decode");
    assert_eq!(c.format, 1);
}
