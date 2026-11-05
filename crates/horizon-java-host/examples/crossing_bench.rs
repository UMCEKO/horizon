//! The Rust to Java crossing: ns per request through the real `Host::request` (`python3 tools/java_mods.py crossing`).
use horizon_java_host::jni::objects::JClass;
use horizon_java_host::jni::sys::jint;
use horizon_java_host::jni::{JNIEnv, NativeMethod};
use horizon_java_host::{Host, get, ready, start};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const HOST: &str = "dev/horizonlab/javamods/host/Host";

#[derive(Clone, Copy)]
struct Options {
    rounds: usize,
    n: usize,
    soak: u64,
    limit: f64,
}

fn options() -> Options {
    let mut options = Options {
        rounds: 5,
        n: 200_000,
        soak: 3,
        limit: 1000.0,
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().unwrap_or_default();
        match flag.as_str() {
            "--rounds" => options.rounds = value.parse().unwrap(),
            "--n" => options.n = value.parse().unwrap(),
            "--soak" => options.soak = value.parse().unwrap(),
            "--limit" => options.limit = value.parse().unwrap(),
            other => panic!("unknown option {other}"),
        }
    }
    options
}

/// The reentrancy native: Java's `bench_nested` asks Rust, which asks Java again on the same thread.
extern "system" fn back(_env: JNIEnv, _class: JClass, tag: jint) -> jint {
    let host = get().expect("the host booted");
    let reply = host.request("bench_echo", &tag.to_be_bytes());
    reply
        .and_then(|r| r.try_into().ok())
        .map_or(-1, i32::from_be_bytes)
}

/// The JVM's used heap in MB after a full GC, seen from the calling thread.
fn live_mb(host: &Host) -> f64 {
    let mut env = host.vm().attach_current_thread_permanently().unwrap();
    env.call_static_method("java/lang/System", "gc", "()V", &[])
        .unwrap();
    let rt = env
        .call_static_method(
            "java/lang/Runtime",
            "getRuntime",
            "()Ljava/lang/Runtime;",
            &[],
        )
        .unwrap()
        .l()
        .unwrap();
    let total = env
        .call_method(&rt, "totalMemory", "()J", &[])
        .unwrap()
        .j()
        .unwrap();
    let free = env
        .call_method(&rt, "freeMemory", "()J", &[])
        .unwrap()
        .j()
        .unwrap();
    (total - free) as f64 / 1048576.0
}

fn median(options: &Options, n: usize, mut f: impl FnMut()) -> f64 {
    for _ in 0..n / 4 {
        f();
    }
    let mut rounds: Vec<f64> = (0..options.rounds)
        .map(|_| {
            let started = Instant::now();
            for _ in 0..n {
                f();
            }
            started.elapsed().as_nanos() as f64 / n as f64
        })
        .collect();
    rounds.sort_by(f64::total_cmp);
    rounds[rounds.len() / 2]
}

fn report(name: &str, ns: f64) {
    println!("RESULT {name:<28} {ns:>10.1} ns/op");
}

fn check(ok: bool, what: &str, failures: &mut Vec<String>) {
    println!("CHECK {what}: {}", if ok { "ok" } else { "FAILED" });
    if !ok {
        failures.push(what.to_string());
    }
}

fn nested_ok(host: &Host, tag: i32) -> bool {
    let mut input = tag.to_be_bytes().to_vec();
    input.extend(0x5eed_i32.to_be_bytes());
    let want: Vec<u8> = [tag, tag, 0x5eed]
        .iter()
        .flat_map(|v| v.to_be_bytes())
        .collect();
    host.request("bench_nested", &input).as_deref() == Some(&want[..])
}

fn soak(secs: u64, failures: &mut Vec<String>) {
    let stop = AtomicBool::new(false);
    let bad = AtomicU64::new(0);
    let total = AtomicU64::new(0);
    // (thread name, op, whether a reply is expected): the window's client ops are empty without a client.
    let threads = [
        ("Server thread", "bench_echo", true),
        ("Window", "client_tick", false),
        ("Network", "bench_echo_free", true),
        ("Server thread 2", "bench_nested", true),
    ];
    std::thread::scope(|scope| {
        for (id, (name, op, replies)) in threads.into_iter().enumerate() {
            let (stop, bad, total) = (&stop, &bad, &total);
            std::thread::Builder::new()
                .name(name.into())
                .spawn_scoped(scope, move || {
                    let host = get().unwrap();
                    let mut counter = 0u32;
                    while !stop.load(Ordering::Relaxed) {
                        counter += 1;
                        let tag = (id as i32) << 24 | (counter & 0xff_ffff) as i32;
                        let good = if op == "bench_nested" {
                            nested_ok(host, tag)
                        } else {
                            let mut input = tag.to_be_bytes().to_vec();
                            input.extend_from_slice(&[id as u8; 40]);
                            let reply = host.request(op, &input);
                            if replies {
                                reply.as_deref() == Some(&input[..])
                            } else {
                                reply.is_none()
                            }
                        };
                        if !good {
                            bad.fetch_add(1, Ordering::Relaxed);
                        }
                        total.fetch_add(1, Ordering::Relaxed);
                    }
                })
                .unwrap();
        }
        std::thread::sleep(Duration::from_secs(secs));
        stop.store(true, Ordering::Relaxed);
    });
    println!(
        "SOAK {} requests on 4 threads, {} wrong",
        total.load(Ordering::Relaxed),
        bad.load(Ordering::Relaxed)
    );
    check(
        bad.load(Ordering::Relaxed) == 0,
        "concurrent threads each get their own replies",
        failures,
    );
}

fn main() {
    let options = options();
    let facts =
        std::env::temp_dir().join(format!("crossing-bench-{}/facts.json", std::process::id()));
    start(Vec::new(), facts.clone());
    if let Err(e) = ready() {
        panic!("the host didn't boot: {e}");
    }
    let host = get().expect("the host booted");
    {
        let mut env = host.vm().attach_current_thread_permanently().unwrap();
        let class = env.find_class(HOST).unwrap();
        let natives = [NativeMethod {
            name: "benchBack".into(),
            sig: "(I)I".into(),
            fn_ptr: back as *mut c_void,
        }];
        env.register_native_methods(&class, &natives).unwrap();
    }
    let mut failures = Vec::new();
    let empty = std::thread::Builder::new()
        .name("Server thread".into())
        .spawn(move || {
            let host = get().unwrap();
            let mut failures: Vec<String> = Vec::new();
            let input64 = [7u8; 64];
            let empty = median(&options, options.n, || {
                std::hint::black_box(host.request("bench_empty", &[]));
            });
            report("bench_empty", empty);
            report(
                "bench_empty_direct",
                median(&options, options.n, || {
                    std::hint::black_box(host.request("bench_empty_direct", &[]));
                }),
            );
            let with = median(&options, options.n, || {
                std::hint::black_box(host.call_with(
                    "bench_empty_direct",
                    |_| {},
                    |reply| reply.byte(),
                ));
            });
            report("call_with bench_empty_direct", with);
            report(
                "bench_echo 64 B",
                median(&options, options.n, || {
                    std::hint::black_box(host.request("bench_echo", &input64));
                }),
            );
            report(
                "bench_nested",
                median(&options, options.n / 2, || {
                    std::hint::black_box(host.request("bench_nested", &[0, 0, 0, 1, 0, 0, 0, 2]));
                }),
            );
            let route = |out: &mut horizon_java_host::wire::Out| {
                out.int(0);
                out.0.push(2);
            };
            report(
                "bench_route (static no-op)",
                median(&options, options.n, || {
                    std::hint::black_box(
                        host.call_with("bench_route", route, |reply| reply.byte()),
                    );
                }),
            );
            check(
                host.call_with("bench_route", route, |reply| reply.byte()) == Some(0),
                "a route op answers status 0",
                &mut failures,
            );
            let big = (1i32 << 20).to_be_bytes();
            report(
                "overflow reply 1 MB",
                median(&options, options.n / 400, || {
                    std::hint::black_box(host.request("bench_big", &big));
                }),
            );
            check(
                host.request("bench_empty", &[]).as_deref() == Some(&[1][..]),
                "empty op replies {1}",
                &mut failures,
            );
            check(
                host.call_with("bench_empty_direct", |_| {}, |reply| reply.byte()) == Some(1),
                "call_with replies {1}",
                &mut failures,
            );
            check(
                host.call_with(
                    "bench_echo",
                    |out| out.text("h\u{e9}llo \u{4e16}"),
                    |reply| reply.text().map(str::to_owned),
                )
                .as_deref()
                    == Some("h\u{e9}llo \u{4e16}"),
                "call_with round trip",
                &mut failures,
            );
            check(
                host.call_with(
                    "bench_echo",
                    |out| out.int(7),
                    |reply| {
                        let inner =
                            host.call_with("bench_echo", |out| out.int(8), |inner| inner.int())?;
                        Some((reply.int()?, inner))
                    },
                ) == Some((7, 8)),
                "a request nested in a reply read keeps both replies",
                &mut failures,
            );
            check(
                host.call_with(
                    "bench_echo",
                    |out| {
                        let inner =
                            host.call_with("bench_echo", |out| out.int(9), |inner| inner.int());
                        out.int(inner.unwrap_or(-1));
                    },
                    |reply| reply.int(),
                ) == Some(9),
                "a request nested in a write is its own",
                &mut failures,
            );
            check(
                nested_ok(host, 41),
                "nested request: replies and the outer input stay correct",
                &mut failures,
            );
            check(
                host.request("bench_big", &big)
                    .is_some_and(|r| r.len() == 1 << 20),
                "a 1 MB reply arrives whole",
                &mut failures,
            );
            let huge = (17i32 << 20).to_be_bytes();
            // The first one sizes this thread's reply buffer on the Java side; only the growth after it is a leak.
            std::hint::black_box(host.request("bench_big", &huge));
            let before = live_mb(host);
            for _ in 0..8 {
                std::hint::black_box(host.request("bench_big", &huge));
            }
            let grown = live_mb(host) - before;
            check(
                grown < 8.0,
                &format!("8 overflowing replies of 17 MB leave the heap {grown:.1} MB higher"),
                &mut failures,
            );
            let long = vec![9u8; 200_000];
            check(
                host.request("bench_echo", &long).as_deref() == Some(&long[..]),
                "an input over the arena's size arrives whole",
                &mut failures,
            );
            check(
                host.request("bench_throw", &[]).is_none(),
                "an op that throws is None",
                &mut failures,
            );
            check(
                host.request("bench_echo", &input64).as_deref() == Some(&input64[..]),
                "the next request succeeds",
                &mut failures,
            );
            check(
                host.request("no_such_op", &[]).is_none(),
                "an unknown op is None",
                &mut failures,
            );
            (empty, failures)
        })
        .unwrap()
        .join()
        .unwrap();
    failures.extend(empty.1);
    if options.soak > 0 {
        soak(options.soak, &mut failures);
    }
    let _ = std::fs::remove_dir_all(facts.parent().unwrap());
    if empty.0 > options.limit {
        failures.push(format!(
            "bench_empty {:.0} ns is over {:.0}",
            empty.0, options.limit
        ));
    }
    if failures.is_empty() {
        println!(
            "PASS bench_empty {:.0} ns/op (limit {:.0})",
            empty.0, options.limit
        );
    } else {
        println!("FAIL {}", failures.join("; "));
        std::process::exit(1);
    }
}
