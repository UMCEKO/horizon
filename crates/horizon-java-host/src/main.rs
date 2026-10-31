mod shapes;

use jni::objects::{JClass, JObject, JString, JValue};
use jni::{InitArgsBuilder, JNIEnv, JNIVersion, JavaVM, NativeMethod};
use std::ffi::c_void;
use std::sync::Mutex;
use std::time::Instant;

static EVENTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
fn js(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(|x| x.into()).unwrap_or_default()
}

extern "system" fn n_on_event(mut env: JNIEnv, _c: JClass, s: JString) {
    let s = js(&mut env, &s);
    println!("[rust] listener: {s}");
    EVENTS.lock().unwrap().push(s);
}

fn status(key: &str) -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    s.lines()
        .find(|l| l.starts_with(key))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|x| x.parse().ok())
        .unwrap_or(0)
}
fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}
fn check(env: &mut JNIEnv, what: &str) {
    if env.exception_check().unwrap() {
        env.exception_describe().ok();
        env.exception_clear().ok();
        panic!("java exception in {what}");
    }
}

fn heap_used_mb(env: &mut JNIEnv) -> f64 {
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
fn gc(env: &mut JNIEnv) {
    env.call_static_method("java/lang/System", "gc", "()V", &[])
        .unwrap();
}

fn call(env: &mut JNIEnv, op: &str, arg: &str) -> String {
    let (o, a) = (env.new_string(op).unwrap(), env.new_string(arg).unwrap());
    let r = env
        .call_static_method(
            "dev/horizonlab/javamods/host/Host",
            "call",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&o), JValue::Object(&a)],
        )
        .unwrap_or_else(|e| {
            env.exception_describe().ok();
            panic!("call {op}: {e}")
        })
        .l()
        .unwrap();
    js(env, &JString::from(r))
}

fn create_vm(run_dir: &str) -> (JavaVM, Vec<String>) {
    // the JVM options of NeoForge's own unix_args.txt, minus the launcher-only parts
    let neoforge = std::env::var("HORIZON_NEOFORGE").unwrap();
    let args = std::fs::read_to_string(format!(
        "{run_dir}/libraries/net/neoforged/neoforge/{neoforge}/unix_args.txt"
    ))
    .unwrap();
    let lines: Vec<&str> = args.lines().collect();
    let mut b = InitArgsBuilder::new().version(JNIVersion::V8);
    let mut prog = vec![];
    let mut i = 0;
    let host_jar = std::env::var("HORIZON_JAVA_HOST_JAR").unwrap();
    while i < lines.len() {
        let l = lines[i].trim();
        if l == "-classpath" {
            b = b.option(format!(
                "-Djava.class.path={}:{}",
                lines[i + 1].trim(),
                host_jar
            ));
            i += 2;
            continue;
        }
        if let Some((a, rest)) = l.split_once(' ') {
            if a == "--add-opens" || a == "--add-exports" {
                b = b.option(format!("{a}={rest}"));
            } else if a.starts_with("--fml") {
                prog.push(a.to_string());
                prog.push(rest.to_string());
            }
        } else if l.starts_with('-') {
            b = b.option(l);
        } else if l == "net.neoforged.fml.startup.Server" {
        }
        i += 1;
    }
    let extra = std::env::var("HORIZON_JAVA_OPTS").unwrap_or_default();
    for o in extra.split_whitespace() {
        b = b.option(o);
    }
    (JavaVM::new(b.build().unwrap()).expect("JavaVM::new"), prog)
}

fn main() {
    let t0 = Instant::now();
    let run_dir = std::env::var("HORIZON_JAVA_RUN").unwrap();
    std::env::set_current_dir(&run_dir).unwrap();
    let ops: Vec<String> = std::env::args().skip(1).collect();
    let rss_before = status("VmRSS");
    let t = Instant::now();
    let (vm, prog) = create_vm(&run_dir);
    let t_create = ms(t);
    let mut env = vm.attach_current_thread_permanently().unwrap();
    {
        let c = env.find_class("dev/horizonlab/javamods/host/Host").unwrap();
        let ms_: Vec<NativeMethod> = vec![
            NativeMethod {
                name: "onEvent".into(),
                sig: "(Ljava/lang/String;)V".into(),
                fn_ptr: n_on_event as *mut c_void,
            },
            NativeMethod {
                name: "shapeFlags".into(),
                sig: "([D[D[D[Z)I".into(),
                fn_ptr: shapes::n_shape_flags as *mut c_void,
            },
        ];
        env.register_native_methods(&c, &ms_).unwrap();
    }
    let rss_vm = status("VmRSS");
    let t = Instant::now();
    let jprog = env
        .new_object_array(prog.len() as i32, "java/lang/String", JObject::null())
        .unwrap();
    for (i, p) in prog.iter().enumerate() {
        let s = env.new_string(p).unwrap();
        env.set_object_array_element(&jprog, i as i32, s).unwrap();
    }
    env.call_static_method(
        "dev/horizonlab/javamods/host/Host",
        "run",
        "([Ljava/lang/String;)V",
        &[JValue::Object(&jprog)],
    )
    .unwrap_or_else(|e| {
        env.exception_describe().ok();
        panic!("Host.run: {e}")
    });
    check(&mut env, "run");
    let t_boot = ms(t);
    let t_point = ms(t0);
    let rss_point = status("VmRSS");
    let hwm = status("VmHWM");
    let rss_anon = status("RssAnon");
    let heap_point = heap_used_mb(&mut env);
    let marks = call(&mut env, "marks", "");
    gc(&mut env);
    gc(&mut env);
    let heap_gc = heap_used_mb(&mut env);
    let rss_gc = status("VmRSS");
    println!("MARKS {marks}");
    println!(
        "METRIC jvm_create_ms={t_create:.1} boot_ms={t_boot:.1} total_ms={t_point:.1} rss_before_kb={rss_before} rss_vm_kb={rss_vm} rss_point_kb={rss_point} hwm_kb={hwm} rss_anon_kb={rss_anon} heap_point_mb={heap_point:.1} rss_gc_kb={rss_gc} heap_gc_mb={heap_gc:.1}"
    );
    for op in &ops {
        let (name, arg) = op.split_once('=').unwrap_or((op, ""));
        println!("--- op {name}");
        if name == "mem" {
            let h = heap_used_mb(&mut env);
            let rss0 = status("VmRSS") as f64 / 1024.0;
            gc(&mut env);
            gc(&mut env);
            let (h2, rss2) = (heap_used_mb(&mut env), status("VmRSS") as f64 / 1024.0);
            println!(
                "MEM {arg}: rss={rss0:.0} MB heap_used={h:.0} MB | after full GC rss={rss2:.0} MB heap_used={h2:.0} MB"
            );
            continue;
        }
        let out = call(&mut env, name, arg);
        if name == "items" {
            compare(&out, arg);
        } else {
            println!("{out}");
        }
    }
}

fn compare(out: &str, ns: &str) {
    let got: serde_json::Value = serde_json::from_str(out).expect("items json");
    let facts: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::env::var("HORIZON_JAVA_FACTS").expect("HORIZON_JAVA_FACTS"))
            .unwrap(),
    )
    .unwrap();
    let want = facts["items"].as_object().unwrap();
    let got = got.as_object().unwrap();
    let (mut ok, mut cmp) = (0, 0);
    for (id, w) in want
        .iter()
        .filter(|(k, _)| k.starts_with(&format!("{ns}:")))
    {
        let Some(g) = got.get(id) else {
            println!("  {id}: MISSING in the live registry");
            continue;
        };
        let mut all = true;
        for f in ["class", "max_stack_size", "rarity"] {
            cmp += 1;
            if w[f] != g[f] {
                all = false;
                println!("  {id}.{f}: facts={} live={}", w[f], g[f]);
            }
        }
        let (wc, gc) = (
            w["components"].as_object().unwrap(),
            g["components"].as_object().unwrap(),
        );
        for (k, v) in wc {
            cmp += 1;
            if gc.get(k) != Some(v) {
                all = false;
                println!("  {id}.components[{k}]: facts={v} live={:?}", gc.get(k));
            }
        }
        for k in gc.keys().filter(|k| !wc.contains_key(*k)) {
            cmp += 1;
            all = false;
            println!("  {id}.components[{k}]: live only");
        }
        println!(
            "  {id}: {} ({} components)",
            if all {
                "EQUAL to facts.json"
            } else {
                "DIFFERS"
            },
            wc.len()
        );
        ok += all as u32;
    }
    for id in got.keys().filter(|k| !want.contains_key(*k)) {
        println!("  {id}: in live registry, absent from facts.json");
    }
    println!(
        "RESULT {ok}/{} items equal facts.json, {cmp} field comparisons",
        want.keys()
            .filter(|k| k.starts_with(&format!("{ns}:")))
            .count()
    );
}
