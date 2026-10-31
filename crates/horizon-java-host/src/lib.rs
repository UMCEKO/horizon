//! Real NeoForge and real mod jars in a JVM embedded in this process (`docs/java-mods.md`).

pub mod bridge;
pub mod shapes;
pub mod values_bench;
pub mod wire;

pub use jni;

use jni::objects::{GlobalRef, JByteArray, JClass, JObject, JString, JValue};
use jni::{InitArgsBuilder, JNIEnv, JNIVersion, JavaVM, NativeMethod};
use std::cell::Cell;
use std::collections::HashSet;
use std::ffi::{CString, c_void};
use std::sync::{LazyLock, Mutex};

const HOST: &str = "dev/horizonlab/javamods/host/Host";
static EVENTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn js(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(|x| x.into()).unwrap_or_default()
}

extern "system" fn n_on_event(mut env: JNIEnv, _c: JClass, s: JString) {
    let s = js(&mut env, &s);
    println!("[rust] listener: {s}");
    EVENTS.lock().unwrap().push(s);
}

/// The JVM options of NeoForge's own `unix_args.txt`, minus the launcher-only parts, and FML's program arguments.
pub fn create_vm(run_dir: &str) -> (JavaVM, Vec<String>) {
    let neoforge = std::env::var("HORIZON_NEOFORGE").unwrap();
    let args = std::fs::read_to_string(format!(
        "{run_dir}/libraries/net/neoforged/neoforge/{neoforge}/unix_args.txt"
    ))
    .unwrap();
    let lines: Vec<&str> = args.lines().collect();
    let mut b = InitArgsBuilder::new().version(JNIVersion::V8);
    let mut prog = vec!["--gameDir".to_string(), run_dir.to_string()];
    let mut i = 0;
    let host_jar = std::env::var("HORIZON_JAVA_HOST_JAR").unwrap();
    while i < lines.len() {
        let l = lines[i].trim();
        if l == "-classpath" {
            let classpath: Vec<String> = lines[i + 1]
                .trim()
                .split(':')
                .map(|entry| format!("{run_dir}/{entry}"))
                .collect();
            b = b.option(format!(
                "-Djava.class.path={}:{host_jar}",
                classpath.join(":")
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
            b = b.option(l.replace("=libraries", &format!("={run_dir}/libraries")));
        }
        i += 1;
    }
    let extra = std::env::var("HORIZON_JAVA_OPTS").unwrap_or_default();
    for o in extra.split_whitespace() {
        b = b.option(o);
    }
    (JavaVM::new(b.build().unwrap()).expect("JavaVM::new"), prog)
}

pub fn register_natives(env: &mut JNIEnv) {
    let c = env.find_class(HOST).unwrap();
    let methods = [
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
        NativeMethod {
            name: "shapeHandle".into(),
            sig: "([D[D[D[Z)J".into(),
            fn_ptr: shapes::n_shape_handle as *mut c_void,
        },
        NativeMethod {
            name: "shapeFree".into(),
            sig: "(J)V".into(),
            fn_ptr: shapes::n_shape_free as *mut c_void,
        },
        NativeMethod {
            name: "link".into(),
            sig: "(Ljava/lang/Class;)V".into(),
            fn_ptr: bridge::n_link as *mut c_void,
        },
    ];
    env.register_native_methods(&c, &methods).unwrap();
}

/// Tells the class processor which methods to serve from Rust, before FML loads a class.
fn publish_bridges(env: &mut JNIEnv) {
    let lines: Vec<String> = bridge::TABLE
        .get()
        .into_iter()
        .flatten()
        .map(bridge::Bridge::line)
        .collect();
    let array = env
        .new_object_array(lines.len() as i32, "java/lang/String", JObject::null())
        .unwrap();
    for (i, line) in lines.iter().enumerate() {
        let line = env.new_string(line).unwrap();
        env.set_object_array_element(&array, i as i32, line)
            .unwrap();
    }
    env.call_static_method(
        HOST,
        "publish",
        "([Ljava/lang/String;)V",
        &[JValue::Object(&array)],
    )
    .unwrap();
}

/// Starts FML and loads the mods, on this thread.
pub fn run_fml(env: &mut JNIEnv, prog: &[String]) {
    publish_bridges(env);
    let args = env
        .new_object_array(prog.len() as i32, "java/lang/String", JObject::null())
        .unwrap();
    for (i, p) in prog.iter().enumerate() {
        let s = env.new_string(p).unwrap();
        env.set_object_array_element(&args, i as i32, s).unwrap();
    }
    env.call_static_method(
        HOST,
        "run",
        "([Ljava/lang/String;)V",
        &[JValue::Object(&args)],
    )
    .unwrap_or_else(|e| {
        env.exception_describe().ok();
        panic!("Host.run: {e}")
    });
}

pub struct Host {
    vm: JavaVM,
    items: HashSet<String>,
    server_group: Option<GlobalRef>,
}

/// Detaches a thread that attached itself when it ends, so a closed world's server thread leaves nothing in the JVM.
struct Attached(*mut jni::sys::JavaVM);

impl Drop for Attached {
    fn drop(&mut self) {
        // SAFETY: this runs on the thread that attached, as its thread-locals end.
        unsafe {
            if let Some(detach) = (**self.0).DetachCurrentThread {
                detach(self.0);
            }
        }
    }
}

static RUNNING: LazyLock<Option<Host>> = LazyLock::new(|| {
    // A mod that stops FML from starting costs the Java mods, not the game.
    std::panic::catch_unwind(boot).unwrap_or_else(|_| {
        eprintln!("Horizon: Java mods failed to start, so this run has none: see the errors above");
        None
    })
});

fn boot() -> Option<Host> {
    let run_dir = std::env::var("HORIZON_JAVA_RUN").ok()?;
    let began = std::time::Instant::now();
    let (vm, prog) = create_vm(&run_dir);
    let server_group = {
        let mut env = vm.attach_current_thread_permanently().unwrap();
        register_natives(&mut env);
        run_fml(&mut env, &prog);
        let group = env
            .get_static_field(HOST, "serverGroup", "Ljava/lang/ThreadGroup;")
            .and_then(|group| group.l())
            .ok();
        group.and_then(|group| env.new_global_ref(group).ok())
    };
    let booted = began.elapsed().as_secs_f64();
    let mut host = Host {
        vm,
        items: HashSet::new(),
        server_group,
    };
    host.request("prepare", &[]);
    let prepared = began.elapsed().as_secs_f64();
    let list = host.request("items", &[]).unwrap_or_default();
    let mut list = wire::In(&list);
    for _ in 0..list.int().unwrap_or(0) {
        host.items.extend(list.text().map(str::to_string));
    }
    println!(
        "Horizon: Java mods ready in {prepared:.1}s (mods {booted:.1}s), {} items",
        host.items.len()
    );
    Some(host)
}

/// Starts the JVM behind the game's back when this run has Java mods, so a title screen never waits for it.
pub fn start(bridges: Vec<bridge::Bridge>) {
    if std::env::var_os("HORIZON_JAVA_RUN").is_some() {
        let _ = bridge::TABLE.set(bridges);
        let _ = std::thread::Builder::new()
            .name("Java mods".into())
            .stack_size(16 << 20)
            .spawn(|| {
                LazyLock::force(&RUNNING);
            });
    }
}

/// The running host, once the mods have loaded (this waits for them); None when this run has no Java mods.
pub fn get() -> Option<&'static Host> {
    RUNNING.as_ref()
}

impl Host {
    /// Whether the item is one a Java mod registered.
    pub fn owns_item(&self, id: &str) -> bool {
        self.items.contains(id)
    }

    /// Attaches this thread as FML's real server group, as `EffectiveSide` asks every thread's group which side it's on.
    fn attach(&self) {
        thread_local! {
            static ATTACHED: Cell<Option<Attached>> = const { Cell::new(None) };
        }
        ATTACHED.with(|attached| {
            let already = attached.take();
            if already.is_none()
                && let Some(group) = &self.server_group
            {
                let name = std::thread::current()
                    .name()
                    .unwrap_or("Server thread")
                    .to_string();
                let name = CString::new(name).unwrap_or_default();
                let mut args = jni::sys::JavaVMAttachArgs {
                    version: jni::sys::JNI_VERSION_1_8,
                    name: name.as_ptr().cast_mut(),
                    group: group.as_raw(),
                };
                let vm = self.vm.get_java_vm_pointer();
                let mut env = std::ptr::null_mut();
                // SAFETY: the JVM outlives this process's threads, and `args` outlives the call, which copies it.
                let status = unsafe {
                    let attach = (**vm)
                        .AttachCurrentThread
                        .expect("every JVM has AttachCurrentThread");
                    attach(vm, &mut env, std::ptr::from_mut(&mut args).cast())
                };
                if status == jni::sys::JNI_OK {
                    attached.set(Some(Attached(vm)));
                }
            } else {
                attached.set(already);
            }
        });
    }

    /// One call into `GameBridge`; None when it threw, which its own stack trace explains.
    pub fn request(&self, op: &str, input: &[u8]) -> Option<Vec<u8>> {
        self.attach();
        let mut env = self.vm.attach_current_thread_permanently().ok()?;
        env.with_local_frame(8, |env| -> jni::errors::Result<_> {
            let name = env.new_string(op)?;
            let bytes = env.byte_array_from_slice(input)?;
            let out = env
                .call_static_method(
                    HOST,
                    "handle",
                    "(Ljava/lang/String;[B)[B",
                    &[JValue::Object(&name), JValue::Object(&bytes)],
                )?
                .l()?;
            env.convert_byte_array(JByteArray::from(out))
        })
        .inspect_err(|_| {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        })
        .ok()
        .filter(|out| !out.is_empty())
    }
}
