use horizon_perf_format::PerfWriter;
use prost::Message;

#[test]
fn whole_file_and_follow() {
    let dir = std::env::temp_dir();
    let path = dir.join("t.hzperf");
    let _ = std::fs::remove_file(&path);
    let mut w = PerfWriter::new(&path).unwrap();
    w.send(vec![0x08, 0x01]);
    let start = horizon_perf_format::Start {
        side: horizon_perf_format::Side::Server as i32,
        ..Default::default()
    };
    let mut sb = Vec::new();
    start.encode_length_delimited(&mut sb).unwrap();
    let mut tagged = vec![0x12u8];
    tagged.extend_from_slice(&sb);
    w.send(tagged);
    let stop = horizon_perf_format::Stop {
        side: horizon_perf_format::Side::Server as i32,
        ..Default::default()
    };
    let mut sb = Vec::new();
    stop.encode_length_delimited(&mut sb).unwrap();
    let mut tagged = vec![0xBAu8, 0x01];
    tagged.extend_from_slice(&sb);
    w.send(tagged);
    w.finish().unwrap();

    let cap = horizon_perf::read_file(&path).unwrap();
    assert_eq!(cap.format, 1);
    assert_eq!(cap.start.len(), 1);
    assert_eq!(cap.stop.len(), 1);

    let path2 = dir.join("nostop.hzperf");
    let _ = std::fs::remove_file(&path2);
    let mut w2 = PerfWriter::new(&path2).unwrap();
    w2.send(vec![0x08, 0x01]);
    w2.finish().unwrap();
    assert!(horizon_perf::read_file(&path2).is_err());

    let path3 = dir.join("fmt2.hzperf");
    let _ = std::fs::remove_file(&path3);
    let mut w3 = PerfWriter::new(&path3).unwrap();
    w3.send(vec![0x08, 0x02]);
    w3.finish().unwrap();
    assert!(horizon_perf::read_file(&path3).is_err());

    let data = std::fs::read(&path).unwrap();
    let mut r = horizon_perf::FollowReader::new();
    let mut fields = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let n = (i * 37 + 13) % (data.len() - i).max(1) + 1;
        let end = (i + n).min(data.len());
        let got = r.push(&data[i..end]).unwrap();
        fields.extend(got);
        i = end;
    }
    assert!(
        fields
            .iter()
            .any(|f| matches!(f, horizon_perf::Field::Stop(_)))
    );
    assert!(fields.len() >= 3);

    let d = dir.join("perfdir");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::copy(&path, d.join("a.hzperf")).unwrap();
    std::fs::copy(&path2, d.join("b.hzperf")).unwrap();
    let latest = horizon_perf::latest(&d).unwrap().unwrap();
    assert!(latest.ends_with("a.hzperf"));
    let lines = horizon_perf::list(&d).unwrap();
    assert_eq!(lines.len(), 2);
}
