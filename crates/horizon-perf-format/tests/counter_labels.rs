use horizon_perf_format::CounterRegistry;
#[test]
fn counter_labels() {
    let r = CounterRegistry::new();
    let a = r.counter("x", &[("k", "v1")]);
    let b = r.counter("x", &[("k", "v1")]);
    a.add(1.0);
    b.add(2.0);
    assert_eq!(a.get(), 3.0);
    let c = r.counter("x", &[("k", "v2")]);
    c.add(5.0);
    assert_eq!(c.get(), 5.0);
    assert_eq!(a.get(), 3.0);
}
