use horizon_perf_format::PerfWriter;

#[test]
fn disconnected_send_drops_and_counts() {
    let path = std::env::temp_dir().join("hpf-channel2.gz");
    let mut w = PerfWriter::new(&path).unwrap();
    let _ = w.finish();
    assert!(!w.send(vec![1, 2, 3]));
    // dropped count at least 1
    assert!(w.dropped_messages() >= 1);
}
