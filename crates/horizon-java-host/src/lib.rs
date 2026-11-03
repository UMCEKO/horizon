//! Real NeoForge and real mod jars in a JVM embedded in this process (`docs/java-mods.md`).

pub mod bridge;
pub mod profile;
pub mod shapes;
pub mod values_bench;
pub mod wire;

pub use jni;

use jni::objects::{GlobalRef, JByteArray, JClass, JObject, JString, JValue};
use jni::{InitArgsBuilder, JNIEnv, JNIVersion, JavaVM, NativeMethod};
use std::cell::Cell;
use std::collections::HashSet;
use std::ffi::{CString, c_void};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};

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

/// The embedded JVM's memory, as a pack's launcher sets it: Rust holds the world, so Java keeps only mods' state (`docs/java-mods.md`).
pub const MEMORY: &[&str] = &["-Xmx640m"];

/// The platform's classpath separator; the `cp` line of the args file uses it too.
const CP_SEP: char = if cfg!(windows) { ';' } else { ':' };

/// The JVM options, classpath and FML arguments `java_mods.py` wrote for this run, one `kind value` per line.
pub fn create_vm(run_dir: &str) -> (JavaVM, Vec<String>) {
    let args = std::fs::read_to_string(format!("{run_dir}/horizon-java.args")).unwrap();
    let host_jar = std::env::var("HORIZON_JAVA_HOST_JAR").unwrap();
    let mut b = InitArgsBuilder::new().version(JNIVersion::V8);
    // First, so the run's own arguments and HORIZON_JAVA_OPTS override them.
    for option in MEMORY {
        b = b.option(*option);
    }
    if let Some(dir) = profile::dir() {
        for option in profile::jfr_options(dir) {
            b = b.option(option);
        }
    }
    let mut prog = vec!["--gameDir".to_string(), run_dir.to_string()];
    for line in args.lines() {
        match line.split_once(' ') {
            Some(("jvm", option)) => b = b.option(option),
            Some(("cp", classpath)) => {
                b = b.option(format!("-Djava.class.path={classpath}{CP_SEP}{host_jar}"))
            }
            Some(("prog", arg)) => prog.push(arg.to_string()),
            _ => {}
        }
    }
    let extra = std::env::var("HORIZON_JAVA_OPTS").unwrap_or_default();
    for o in extra.split_whitespace() {
        b = b.option(o);
    }
    (JavaVM::new(b.build().unwrap()).expect("JavaVM::new"), prog)
}

/// Whether this run boots NeoForge's client side, as a game window does, rather than a dedicated server's.
fn client_side(run_dir: &str) -> bool {
    std::fs::read_to_string(format!("{run_dir}/horizon-java.args"))
        .is_ok_and(|args| args.lines().any(|line| line == "dist client"))
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
pub fn run_fml(env: &mut JNIEnv, prog: &[String], run_dir: &str) {
    try_run_fml(env, prog, run_dir).unwrap_or_else(|e| panic!("Host.run: {e}"));
}

/// The pending Java exception's stack trace, cleared.
fn java_error(env: &mut JNIEnv, e: jni::errors::Error) -> String {
    let Ok(thrown) = env.exception_occurred() else {
        return e.to_string();
    };
    if thrown.is_null() {
        return e.to_string();
    }
    let _ = env.exception_clear();
    env.call_static_method(
        HOST,
        "trace",
        "(Ljava/lang/Throwable;)Ljava/lang/String;",
        &[JValue::Object(&thrown)],
    )
    .and_then(|t| t.l())
    .map(|t| js(env, &JString::from(t)))
    .unwrap_or_else(|_| e.to_string())
}

/// [`run_fml`], with what stopped FML as the error.
pub fn try_run_fml(env: &mut JNIEnv, prog: &[String], run_dir: &str) -> Result<(), String> {
    publish_bridges(env);
    let client = client_side(run_dir);
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
        "([Ljava/lang/String;Z)V",
        &[JValue::Object(&args), JValue::Bool(client.into())],
    )
    .map(drop)
    .map_err(|e| java_error(env, e))
}

pub struct Host {
    vm: JavaVM,
    items: Items,
    server_group: Option<GlobalRef>,
    facts: PathBuf,
    client_resources: Vec<u8>,
}

/// The mods' items, and those whose class has its own `use` or `useOn`; the others' clicks are vanilla's, from their components.
#[derive(Default)]
struct Items {
    owned: HashSet<String>,
    use_in_air: HashSet<String>,
    use_on: HashSet<String>,
    entity: HashSet<String>,
    mine: HashSet<String>,
}

impl Items {
    fn read(bytes: &[u8]) -> Option<Items> {
        let mut list = wire::In(bytes);
        let mut items = Items::default();
        for _ in 0..list.int()? {
            let id = list.text()?.to_string();
            if list.bool()? {
                items.use_in_air.insert(id.clone());
            }
            if list.bool()? {
                items.use_on.insert(id.clone());
            }
            if list.bool()? {
                items.entity.insert(id.clone());
            }
            if list.bool()? {
                items.mine.insert(id.clone());
            }
            items.owned.insert(id);
        }
        Some(items)
    }
}

/// Whether this run has Java mods, and how their boot went.
enum Boot {
    Off,
    Failed(String),
    Ready(Box<Host>),
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

static FACTS: OnceLock<PathBuf> = OnceLock::new();

static RUNNING: LazyLock<Boot> = LazyLock::new(|| {
    let Ok(run_dir) = std::env::var("HORIZON_JAVA_RUN") else {
        return Boot::Off;
    };
    let booted = std::panic::catch_unwind(|| boot(&run_dir)).unwrap_or_else(|panic| {
        let text = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()));
        Err(text.unwrap_or_else(|| "the boot panicked".into()))
    });
    match booted {
        Ok(host) => Boot::Ready(Box::new(host)),
        Err(e) => {
            eprintln!("Horizon: Java mods failed to start: {e}");
            Boot::Failed(e)
        }
    }
});

fn boot(run_dir: &str) -> Result<Host, String> {
    let began = std::time::Instant::now();
    let (vm, prog) = create_vm(run_dir);
    let server_group = {
        let mut env = vm
            .attach_current_thread_permanently()
            .map_err(|e| e.to_string())?;
        register_natives(&mut env);
        try_run_fml(&mut env, &prog, run_dir)?;
        let group = env
            .get_static_field(HOST, "serverGroup", "Ljava/lang/ThreadGroup;")
            .and_then(|group| group.l())
            .ok();
        group.and_then(|group| env.new_global_ref(group).ok())
    };
    let booted = began.elapsed().as_secs_f64();
    let facts = FACTS.get().cloned().unwrap_or_else(|| {
        std::env::temp_dir().join(format!("horizon-boot-facts-{}.json", std::process::id()))
    });
    let mut host = Host {
        vm,
        items: Items::default(),
        server_group,
        facts,
        client_resources: Vec::new(),
    };
    host.call("prepare", "")?;
    let prepared = began.elapsed().as_secs_f64();
    if let Some(dir) = host.facts.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    host.call("boot_facts", &host.facts.to_string_lossy())?;
    let exported = began.elapsed().as_secs_f64();
    let list = host
        .request("items", &[])
        .ok_or("the mods' items didn't list")?;
    host.items = Items::read(&list).ok_or("the mods' item list ended early")?;
    host.client_resources = host.request("client_resources", &[]).unwrap_or_default();
    let baked = host.request("client_baked", &[]).unwrap_or_default();
    if !baked.is_empty() {
        std::fs::write(host.baked(), baked)
            .map_err(|e| format!("{}: {e}", host.baked().display()))?;
    }
    let keys = host.request("client_keys", &[]).unwrap_or_default();
    if !keys.is_empty() {
        std::fs::write(host.keys(), keys).map_err(|e| format!("{}: {e}", host.keys().display()))?;
    }
    println!(
        "Horizon: Java mods ready in {exported:.1}s (mods {booted:.1}s, data {:.1}s, facts {:.1}s), {} items",
        prepared - booted,
        exported - prepared,
        host.items.owned.len()
    );
    Ok(host)
}

/// Starts the JVM behind the game's back when this run has Java mods, so a title screen never waits for it; the boot writes its facts to `facts`.
pub fn start(bridges: Vec<bridge::Bridge>, facts: PathBuf) {
    if configured() {
        let _ = bridge::TABLE.set(bridges);
        let _ = FACTS.set(facts);
        let _ = std::thread::Builder::new()
            .name("Java mods".into())
            .stack_size(16 << 20)
            .spawn(|| {
                LazyLock::force(&RUNNING);
            });
    }
}

/// Whether this run has Java mods (`HORIZON_JAVA_RUN`), booted or not.
pub fn configured() -> bool {
    std::env::var_os("HORIZON_JAVA_RUN").is_some()
}

/// The running host, once the mods have loaded (this waits for them); None when this run has no Java mods or their boot failed.
pub fn get() -> Option<&'static Host> {
    match &*RUNNING {
        Boot::Ready(host) => Some(host),
        _ => None,
    }
}

/// Why the mods this run has failed to boot (this waits for the boot); Ok without Java mods.
pub fn ready() -> Result<(), &'static str> {
    match &*RUNNING {
        Boot::Failed(e) => Err(e),
        _ => Ok(()),
    }
}

impl Host {
    /// The facts file this boot wrote (`BootFacts`).
    pub fn facts(&self) -> &Path {
        &self.facts
    }

    /// Where the boot writes the block models its mods' code builds, from the Java client's resource reload export (absent without a client).
    pub fn baked(&self) -> PathBuf {
        self.facts.with_extension("baked.json")
    }

    /// Where the boot writes the client's key mappings, the mods' and vanilla's conflict contexts (absent without a client).
    pub fn keys(&self) -> PathBuf {
        self.facts.with_extension("keys.tsv")
    }

    /// What the Java client's first resource reload made for the window (`ClientResources`), taken as the boot ended; empty without a client.
    pub fn client_resources(&self) -> &[u8] {
        &self.client_resources
    }

    /// One `Boot.op` call; its Java error when it threw.
    fn call(&self, op: &str, arg: &str) -> Result<String, String> {
        self.attach();
        let mut env = self
            .vm
            .attach_current_thread_permanently()
            .map_err(|e| e.to_string())?;
        let out = env
            .with_local_frame(8, |env| -> jni::errors::Result<String> {
                let (o, a) = (env.new_string(op)?, env.new_string(arg)?);
                let r = env
                    .call_static_method(
                        HOST,
                        "call",
                        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                        &[JValue::Object(&o), JValue::Object(&a)],
                    )?
                    .l()?;
                Ok(js(env, &JString::from(r)))
            })
            .map_err(|e| java_error(&mut env, e))?;
        match out.strip_prefix("ERROR ") {
            Some(trace) => Err(format!("{op}: {trace}")),
            None => Ok(out),
        }
    }

    /// The JVM's side of the profile since the last take (`be=... lock=...`), a `profile_dump` to `path` when given.
    pub fn profile(&self, op: &str, input: &[u8]) -> Option<String> {
        let out = self.request_untimed(op, input)?;
        String::from_utf8(out).ok()
    }

    /// Whether the item is one a Java mod registered.
    pub fn owns_item(&self, id: &str) -> bool {
        self.items.owned.contains(id)
    }

    /// Whether a click with the item runs its mod's own code (`BootFacts.USE`, `USE_ON`): in the air, from `use` to a held use's end, or on a block.
    pub fn item_code_runs(&self, id: &str, on_block: bool) -> bool {
        if on_block {
            self.items.use_on.contains(id)
        } else {
            self.items.use_in_air.contains(id)
        }
    }

    /// Whether the item's `interactLivingEntity` is its mod's own code.
    pub fn item_entity_code(&self, id: &str) -> bool {
        self.items.entity.contains(id)
    }

    /// Whether the item's `mineBlock` is its mod's own code.
    pub fn item_mine_code(&self, id: &str) -> bool {
        self.items.mine.contains(id)
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
        let started = profile::dir().map(|_| std::time::Instant::now());
        let out = self.request_untimed(op, input);
        if let Some(started) = started {
            profile::record_call(op, started.elapsed());
        }
        out
    }

    fn request_untimed(&self, op: &str, input: &[u8]) -> Option<Vec<u8>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_keep_which_clicks_run_their_code() {
        let mut out = wire::Out::default();
        out.int(2);
        out.text("m:snack");
        out.bool(false);
        out.bool(false);
        out.bool(false);
        out.bool(false);
        out.text("m:wrench");
        out.bool(true);
        out.bool(true);
        out.bool(true);
        out.bool(true);
        let items = Items::read(&out.0).unwrap();
        assert!(items.owned.contains("m:snack") && items.owned.contains("m:wrench"));
        assert!(!items.use_in_air.contains("m:snack") && !items.use_on.contains("m:snack"));
        assert!(!items.entity.contains("m:snack"));
        assert!(items.use_in_air.contains("m:wrench") && items.use_on.contains("m:wrench"));
        assert!(items.entity.contains("m:wrench") && items.mine.contains("m:wrench"));
        assert!(
            Items::read(&out.0[..out.0.len() - 1]).is_none(),
            "a short list fails"
        );
    }
}
