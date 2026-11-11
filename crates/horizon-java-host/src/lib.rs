//! Real NeoForge and real mod jars in a JVM embedded in this process (`docs/java-mods.md`).

pub mod bridge;
pub mod entity_facts;
pub mod profile;
pub mod routes;
pub mod shapes;
pub mod values_bench;
pub mod wire;

pub use jni;

use jni::objects::{
    GlobalRef, JByteArray, JByteBuffer, JClass, JObject, JObjectArray, JStaticMethodID, JString,
    JValue,
};
use jni::signature::ReturnType;
use jni::sys::{jint, jvalue};
use jni::{InitArgsBuilder, JNIEnv, JNIVersion, JavaVM, NativeMethod};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::{CString, c_void};
use std::hash::{BuildHasherDefault, Hasher};
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
pub const MEMORY: &[&str] = &["-Xmx640m", "-XX:+UseCompactObjectHeaders"];

/// The window's main thread calls Java at its full depth, but HotSpot gives an attached main thread only `ThreadStackSize` of its stack (1 MiB by default), past which the attach itself overflows.
pub const MAIN_THREAD_STACK: &str = "-Xss8m";

/// The platform's classpath separator; the `cp` line of the args file uses it too.
const CP_SEP: char = if cfg!(windows) { ';' } else { ':' };

/// What `horizon-java.args` asks the JVM for.
pub struct RunArgs {
    pub java: PathBuf,
    pub options: Vec<String>,
    pub prog: Vec<String>,
    pub client: bool,
}

impl RunArgs {
    /// `host_override` is `HORIZON_JAVA_HOST_JAR` when set: a developer's private host jar.
    pub fn parse(
        run_dir: &Path,
        text: &str,
        host_override: Option<&str>,
    ) -> Result<RunArgs, String> {
        let run = run_dir.display();
        let mut java = None;
        let mut host = host_override.map(str::to_string);
        let mut classpath = None;
        let mut options = Vec::new();
        let mut prog = vec!["--gameDir".to_string(), run.to_string()];
        let mut client = false;
        let mut cp_at = 0;
        for line in text.lines() {
            client |= line == "dist client";
            match line.split_once(' ') {
                Some(("java", home)) => java = Some(PathBuf::from(home)),
                Some(("host", jar)) if host_override.is_none() => host = Some(jar.to_string()),
                Some(("jvm", option)) => options.push(option.to_string()),
                Some(("cp", path)) => {
                    classpath = Some(path.to_string());
                    cp_at = options.len();
                }
                Some(("prog", arg)) => prog.push(arg.to_string()),
                Some(("analysis", path)) => options.push(format!("-Dhorizon.java.analysis={path}")),
                _ => {}
            }
        }
        let java = java.ok_or_else(|| {
            format!(
                "{run}/horizon-java.args names no Java (a `java` line): install the Java mods again"
            )
        })?;
        let host = host.ok_or_else(|| {
            format!("{run}/horizon-java.args names no host jar (a `host` line): install the Java mods again")
        })?;
        let classpath = classpath
            .ok_or_else(|| format!("{run}/horizon-java.args has no classpath (a `cp` line)"))?;
        options.insert(
            cp_at,
            format!("-Djava.class.path={classpath}{CP_SEP}{host}"),
        );
        Ok(RunArgs {
            java,
            options,
            prog,
            client,
        })
    }
}

/// The JDK's major version, from the `JAVA_VERSION` line of its `release` file.
fn java_major(home: &Path) -> Result<u32, String> {
    let release = std::fs::read_to_string(home.join("release"))
        .map_err(|_| format!("{} is not a Java home (no release file)", home.display()))?;
    release
        .lines()
        .find_map(|line| line.strip_prefix("JAVA_VERSION="))
        .map(|v| v.trim().trim_matches('"'))
        .and_then(|v| v.split('.').next())
        .and_then(|major| major.parse().ok())
        .ok_or_else(|| format!("{}/release names no JAVA_VERSION", home.display()))
}

/// The JDK's `libjvm`, where the platform keeps it.
fn libjvm(home: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        home.join("bin/server/jvm.dll")
    } else if cfg!(target_os = "macos") {
        home.join("lib/server/libjvm.dylib")
    } else {
        home.join("lib/server/libjvm.so")
    }
}

/// Starts the JVM `horizon-java.args` in `run_dir` names, opening that JDK's `libjvm` by path.
pub fn create_vm(run_dir: &str) -> Result<(JavaVM, RunArgs), String> {
    let file = format!("{run_dir}/horizon-java.args");
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{file}: {e}"))?;
    let host = std::env::var("HORIZON_JAVA_HOST_JAR").ok();
    let run = RunArgs::parse(Path::new(run_dir), &text, host.as_deref())?;
    let home = &run.java;
    let major = java_major(home)?;
    if major < 25 {
        return Err(format!(
            "Java mods need Java 25 or later; {} is Java {major}",
            home.display()
        ));
    }
    let lib = libjvm(home);
    if !lib.exists() {
        return Err(format!(
            "{} is missing: {} is not a full JDK",
            lib.display(),
            home.display()
        ));
    }
    let mut b = InitArgsBuilder::new().version(JNIVersion::V8);
    // First, so the run's own arguments and HORIZON_JAVA_OPTS override them.
    for option in MEMORY {
        b = b.option(*option);
    }
    b = b.option(MAIN_THREAD_STACK);
    if let Some(dir) = profile::dir() {
        for option in profile::jfr_options(dir) {
            b = b.option(option);
        }
    }
    for option in &run.options {
        b = b.option(option);
    }
    let extra = std::env::var("HORIZON_JAVA_OPTS").unwrap_or_default();
    for o in extra.split_whitespace() {
        b = b.option(o);
    }
    let args = b.build().map_err(|e| e.to_string())?;
    let vm = JavaVM::with_libjvm(args, || Ok(lib)).map_err(|e| e.to_string())?;
    Ok((vm, run))
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
            name: "earlyNative".into(),
            sig: "()Z".into(),
            fn_ptr: bridge::n_route_early as *mut c_void,
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
pub fn run_fml(env: &mut JNIEnv, run: &RunArgs) {
    try_run_fml(env, run).unwrap_or_else(|e| panic!("Host.run: {e}"));
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
pub fn try_run_fml(env: &mut JNIEnv, run: &RunArgs) -> Result<(), String> {
    publish_bridges(env);
    let args = env
        .new_object_array(run.prog.len() as i32, "java/lang/String", JObject::null())
        .unwrap();
    for (i, p) in run.prog.iter().enumerate() {
        let s = env.new_string(p).unwrap();
        env.set_object_array_element(&args, i as i32, s).unwrap();
    }
    env.call_static_method(
        HOST,
        "run",
        "([Ljava/lang/String;Z)V",
        &[JValue::Object(&args), JValue::Bool(run.client.into())],
    )
    .map(drop)
    .map_err(|e| java_error(env, e))
}

/// FNV-1a: op names are short and few, and SipHash would be a tenth of a request.
#[derive(Default)]
struct Fnv(u64);

impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        let mut hash = if self.0 == 0 {
            0xcbf2_9ce4_8422_2325
        } else {
            self.0
        };
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
        self.0 = hash;
    }
}

/// What a request crosses with: Host's class, its `dispatch`, `arena` and `overflow`, and the ops by their index in Java's table.
struct Entry {
    class: GlobalRef,
    dispatch: JStaticMethodID,
    arena: JStaticMethodID,
    overflow: JStaticMethodID,
    ops: HashMap<Box<str>, jint, BuildHasherDefault<Fnv>>,
}

pub struct Host {
    vm: JavaVM,
    entry: Entry,
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
struct Attached(*mut jni::sys::JavaVM, *mut jni::sys::JNIEnv);

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
    let (vm, run) = create_vm(run_dir)?;
    let (server_group, entry) = {
        let mut env = vm
            .attach_current_thread_permanently()
            .map_err(|e| e.to_string())?;
        register_natives(&mut env);
        try_run_fml(&mut env, &run)?;
        let group = env
            .get_static_field(HOST, "serverGroup", "Ljava/lang/ThreadGroup;")
            .and_then(|group| group.l())
            .ok();
        let entry = Entry::read(&mut env)?;
        (
            group.and_then(|group| env.new_global_ref(group).ok()),
            entry,
        )
    };
    let booted = began.elapsed().as_secs_f64();
    let facts = FACTS.get().cloned().unwrap_or_else(|| {
        std::env::temp_dir().join(format!("horizon-boot-facts-{}.json", std::process::id()))
    });
    let mut host = Host {
        vm,
        entry,
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
    let baked = host.baked();
    host.request("client_baked", baked.to_string_lossy().as_bytes());
    let keys = host.request("client_keys", &[]).unwrap_or_default();
    if !keys.is_empty() {
        std::fs::write(host.keys(), keys).map_err(|e| format!("{}: {e}", host.keys().display()))?;
    }
    let analysed = host.request("entity_facts", &[]).unwrap_or_default();
    if !analysed.is_empty() {
        entity_facts::set(
            entity_facts::EntityFacts::read(&analysed).map_err(|e| format!("entities.bin: {e}"))?,
        );
        std::fs::write(host.entity_facts(), analysed)
            .map_err(|e| format!("{}: {e}", host.entity_facts().display()))?;
    }
    let routed = host.request("routes_bin", &[]).unwrap_or_default();
    if !routed.is_empty() {
        routes::set(routes::Routes::read(&routed).map_err(|e| format!("routes.bin: {e}"))?);
        std::fs::write(host.routes_file(), routed)
            .map_err(|e| format!("{}: {e}", host.routes_file().display()))?;
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
    /// The embedded JVM.
    pub fn vm(&self) -> &JavaVM {
        &self.vm
    }

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

    /// Where the boot writes `entities.bin`, the install's entity analysis confirmed against the booted mods (absent without an analysis).
    pub fn entity_facts(&self) -> PathBuf {
        self.facts.with_extension("entities.bin")
    }

    /// Where the boot writes `routes.bin`, the install's routing confirmed against the booted mods (absent without an analysis).
    pub fn routes_file(&self) -> PathBuf {
        self.facts.with_extension("routes.bin")
    }

    /// Loads the entity types' classes, once the first server has started: from `cached` when it exists, else built by Java on a view level and kept there.
    pub fn entity_types(&self, cached: &Path) -> Result<(), String> {
        let Some(facts) = entity_facts::get().filter(|f| !f.types_loaded()) else {
            return Ok(());
        };
        let bytes = match std::fs::read(cached) {
            Ok(bytes) => bytes,
            Err(_) => {
                let bytes = self
                    .request("entity_types", &[])
                    .ok_or("Java built no entity types")?;
                std::fs::write(cached, &bytes).map_err(|e| format!("{}: {e}", cached.display()))?;
                bytes
            }
        };
        facts.load_types(&bytes)
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
        self.cross(op, input, |reply| String::from_utf8(reply.to_vec()).ok())
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
        CROSSING.with_borrow_mut(|crossing| {
            if crossing.attached.is_some() {
                return;
            }
            let Some(group) = &self.server_group else {
                return;
            };
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
                crossing.attached = Some(Attached(vm, env.cast()));
            }
        });
    }

    /// One call into `GameBridge`; None when it threw, which its own stack trace explains.
    pub fn request(&self, op: &str, input: &[u8]) -> Option<Vec<u8>> {
        self.timed(op, || self.cross(op, input, |reply| Some(reply.to_vec())))
    }

    /// [`request`](Self::request) without the copies: `write` fills the request in this thread's buffer and `read` parses the reply where it lies.
    pub fn call_with<R>(
        &self,
        op: &str,
        write: impl FnOnce(&mut wire::Out),
        read: impl FnOnce(&mut wire::In) -> Option<R>,
    ) -> Option<R> {
        self.timed(op, || {
            let level = DEPTH.get();
            let mut out = wire::Out(CROSSING.with_borrow_mut(|c| {
                c.scratch
                    .get_mut(level)
                    .map(std::mem::take)
                    .unwrap_or_default()
            }));
            out.0.clear();
            write(&mut out);
            let reply = self.cross(op, &out.0, |bytes| read(&mut wire::In(bytes)));
            CROSSING.with_borrow_mut(|c| {
                if c.scratch.len() <= level {
                    c.scratch.resize_with(level + 1, Vec::new);
                }
                c.scratch[level] = out.0;
            });
            reply
        })
    }

    fn timed<R>(&self, op: &str, f: impl FnOnce() -> R) -> R {
        let started = profile::dir().map(|_| std::time::Instant::now());
        let out = f();
        if let Some(started) = started {
            profile::record_call(op, started.elapsed());
        }
        out
    }

    /// An arena of this thread at this nesting level with room for `need` bytes: its Java buffer and address.
    fn arena(
        &self,
        env: &mut JNIEnv,
        level: usize,
        need: usize,
    ) -> Option<(jni::sys::jobject, *mut u8)> {
        CROSSING.with_borrow_mut(|c| {
            if let Some(arena) = c.arenas.get(level)
                && arena.capacity >= need
            {
                return Some((arena.buffer.as_raw(), arena.address));
            }
            let capacity = capacity_for(need);
            let arena = self.entry.new_arena(env, capacity)?;
            let found = (arena.buffer.as_raw(), arena.address);
            match c.arenas.get_mut(level) {
                Some(slot) => *slot = arena,
                None => c.arenas.push(arena),
            }
            Some(found)
        })
    }

    /// One request, its reply read where it lies; None when Java threw or had nothing to say.
    fn cross<R>(&self, op: &str, input: &[u8], read: impl FnOnce(&[u8]) -> Option<R>) -> Option<R> {
        let id = *self.entry.ops.get(op)?;
        let level = DEPTH.get();
        let ready = CROSSING.with_borrow(|c| {
            let arena = c.arenas.get(level).filter(|a| a.capacity >= input.len())?;
            Some((c.attached.as_ref()?.1, arena.buffer.as_raw(), arena.address))
        });
        let (mut env, buffer, address) = match ready {
            // SAFETY: the pointer is this thread's own env, from the call that attached it.
            Some((env, buffer, address)) => {
                (unsafe { JNIEnv::from_raw(env) }.ok()?, buffer, address)
            }
            None => {
                self.attach();
                let mut env = self.vm.attach_current_thread_permanently().ok()?;
                let (buffer, address) = self.arena(&mut env, level, input.len())?;
                (env, buffer, address)
            }
        };
        // SAFETY: the arena holds at least `input.len()` bytes and nothing else uses it at this level.
        unsafe { std::ptr::copy_nonoverlapping(input.as_ptr(), address, input.len()) };
        let _nested = Nesting::enter(level);
        let raw = env.get_raw();
        let args = [
            jvalue { i: id },
            jvalue { l: buffer },
            jvalue {
                i: input.len() as jint,
            },
        ];
        // SAFETY: `dispatch` is `Host.dispatch(int, ByteBuffer, int)` and these are its arguments.
        let (n, thrown) = unsafe {
            let call = (**raw)
                .CallStaticIntMethodA
                .expect("every JVM has CallStaticIntMethodA");
            let n = call(
                raw,
                self.entry.class.as_raw(),
                self.entry.dispatch.into_raw(),
                args.as_ptr(),
            );
            (
                n,
                ((**raw)
                    .ExceptionCheck
                    .expect("every JVM has ExceptionCheck"))(raw)
                    != 0,
            )
        };
        if thrown {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
            return None;
        }
        let result = match n {
            // SAFETY: Java wrote `n` bytes to the arena's start, and a request nested in `read` uses the next level's.
            1.. => read(unsafe { std::slice::from_raw_parts(address, n as usize) }),
            OVERFLOW => {
                let reply = self.entry.overflow(&mut env)?;
                if reply.len() <= ARENA_CAP {
                    self.arena(&mut env, level, reply.len());
                }
                read(&reply)
            }
            _ => None,
        };
        self.shrink(&mut env, level);
        result
    }

    /// Gives back a large arena of this level once its request is read, so one big call doesn't pin native memory for the thread's life.
    fn shrink(&self, env: &mut JNIEnv, level: usize) {
        CROSSING.with_borrow_mut(|c| {
            if let Some(slot) = c.arenas.get_mut(level)
                && capacity_after(slot.capacity) != slot.capacity
                && let Some(small) = self.entry.new_arena(env, ARENA)
            {
                *slot = small;
            }
        });
    }
}

impl Entry {
    /// Host's entry points and the ops Java registered, once the boot has returned.
    fn read(env: &mut JNIEnv) -> Result<Entry, String> {
        let find = |env: &mut JNIEnv, class: &JClass, name: &str, sig: &str| {
            env.get_static_method_id(class, name, sig)
                .map_err(|e| e.to_string())
        };
        let class = env.find_class(HOST).map_err(|e| e.to_string())?;
        let dispatch = find(env, &class, "dispatch", "(ILjava/nio/ByteBuffer;I)I")?;
        let arena = find(env, &class, "arena", "(I)Ljava/nio/ByteBuffer;")?;
        let overflow = find(env, &class, "overflow", "()[B")?;
        let names = env
            .call_static_method(&class, "ops", "()[Ljava/lang/String;", &[])
            .and_then(|names| names.l())
            .map_err(|e| java_error(env, e))?;
        let names = JObjectArray::from(names);
        let mut ops = HashMap::default();
        for index in 0..env.get_array_length(&names).map_err(|e| e.to_string())? {
            let name = env
                .get_object_array_element(&names, index)
                .map_err(|e| e.to_string())?;
            ops.insert(js(env, &JString::from(name)).into_boxed_str(), index);
        }
        let class = env.new_global_ref(class).map_err(|e| e.to_string())?;
        Ok(Entry {
            class,
            dispatch,
            arena,
            overflow,
            ops,
        })
    }

    fn new_arena(&self, env: &mut JNIEnv, capacity: usize) -> Option<Arena> {
        // SAFETY: `arena` is `Host.arena(int)`.
        let buffer = unsafe {
            let class = JClass::from_raw(self.class.as_raw());
            env.call_static_method_unchecked(
                &class,
                self.arena,
                ReturnType::Object,
                &[JValue::Int(capacity as jint).as_jni()],
            )
        }
        .and_then(|buffer| buffer.l())
        .ok()?;
        let buffer = JByteBuffer::from(buffer);
        let address = env.get_direct_buffer_address(&buffer).ok()?;
        let global = env.new_global_ref(&buffer).ok()?;
        let _ = env.delete_local_ref(buffer);
        Some(Arena {
            buffer: global,
            address,
            capacity,
        })
    }

    /// The reply that didn't fit its arena.
    fn overflow(&self, env: &mut JNIEnv) -> Option<Vec<u8>> {
        // SAFETY: `overflow` is `Host.overflow()`.
        let reply = unsafe {
            let class = JClass::from_raw(self.class.as_raw());
            env.call_static_method_unchecked(&class, self.overflow, ReturnType::Array, &[])
        }
        .and_then(|reply| reply.l())
        .ok()?;
        let reply = JByteArray::from(reply);
        let bytes = env.convert_byte_array(&reply).ok();
        // A permanently attached thread has no Java frame to free it.
        let _ = env.delete_local_ref(reply);
        bytes
    }
}

/// Where a thread's requests cross: a Java-owned buffer per nesting level, as a native called from Java may ask Java again.
struct Arena {
    buffer: GlobalRef,
    address: *mut u8,
    capacity: usize,
}

/// A thread's crossing state; fields drop in order, so the buffers go before the thread detaches.
struct Crossing {
    arenas: Vec<Arena>,
    scratch: Vec<Vec<u8>>,
    attached: Option<Attached>,
}

/// A reply longer than this takes the overflow path without growing the arena.
const ARENA_CAP: usize = 16 << 20;
const ARENA: usize = 64 << 10;
const OVERFLOW: jint = -2;

thread_local! {
    static CROSSING: RefCell<Crossing> = const {
        RefCell::new(Crossing { arenas: Vec::new(), scratch: Vec::new(), attached: None })
    };
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// Arenas up to this size are kept for reuse; a larger one is replaced by a default one after its request.
const ARENA_KEEP: usize = 1 << 20;

/// What a held arena of `capacity` shrinks to once its request is done.
fn capacity_after(capacity: usize) -> usize {
    if capacity > ARENA_KEEP {
        ARENA
    } else {
        capacity
    }
}

/// An arena's size for a request or reply of `need` bytes: a power of two, so growth is rare.
fn capacity_for(need: usize) -> usize {
    need.next_power_of_two().max(ARENA)
}

/// The nesting level of this thread's requests, restored when a request ends however it ends.
struct Nesting(usize);

impl Nesting {
    fn enter(level: usize) -> Nesting {
        DEPTH.set(level + 1);
        Nesting(level)
    }
}

impl Drop for Nesting {
    fn drop(&mut self) {
        DEPTH.set(self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARGS: &str =
        "java /j\nhost /h.jar\ndist client\ncp /a.jar:/b.jar\njvm -Dx=1\nprog --fml.x\nprog 1\n";

    #[test]
    fn args_file_names_the_jvm_and_its_options() {
        let run = RunArgs::parse(Path::new("/run"), ARGS, None).unwrap();
        assert_eq!(
            run.options,
            ["-Djava.class.path=/a.jar:/b.jar:/h.jar", "-Dx=1"]
        );
        assert_eq!(run.prog, ["--gameDir", "/run", "--fml.x", "1"]);
        assert!(run.client);
        assert_eq!(run.java, PathBuf::from("/j"));
    }

    #[test]
    fn host_override_replaces_the_hosts_jar() {
        let run = RunArgs::parse(Path::new("/run"), ARGS, Some("/p.jar")).unwrap();
        assert_eq!(run.options[0], "-Djava.class.path=/a.jar:/b.jar:/p.jar");
    }

    #[test]
    fn missing_lines_are_named() {
        let err = |text: &str| RunArgs::parse(Path::new("/run"), text, None).err().unwrap();
        assert_eq!(
            err("host /h\ncp /a\n"),
            "/run/horizon-java.args names no Java (a `java` line): install the Java mods again"
        );
        assert_eq!(
            err("java /j\ncp /a\n"),
            "/run/horizon-java.args names no host jar (a `host` line): install the Java mods again"
        );
        assert_eq!(
            err("java /j\nhost /h\n"),
            "/run/horizon-java.args has no classpath (a `cp` line)"
        );
    }

    fn home_with(release: Option<&str>) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "hz-java-major-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(text) = release {
            std::fs::write(dir.join("release"), text).unwrap();
        }
        dir
    }

    #[test]
    fn java_major_reads_the_release_file() {
        for (text, major) in [
            ("JAVA_VERSION=\"25.0.5\"\n", 25),
            ("IMPLEMENTOR=\"x\"\nJAVA_VERSION=\"21.0.12.1\"\n", 21),
            ("JAVA_VERSION=\"26\"\n", 26),
        ] {
            assert_eq!(java_major(&home_with(Some(text))), Ok(major));
        }
        let none = home_with(None);
        assert_eq!(
            java_major(&none),
            Err(format!(
                "{} is not a Java home (no release file)",
                none.display()
            ))
        );
        let bare = home_with(Some("X=1\n"));
        assert_eq!(
            java_major(&bare),
            Err(format!("{}/release names no JAVA_VERSION", bare.display()))
        );
    }

    #[test]
    fn ops_are_found_by_name_in_java_order() {
        let mut ops: HashMap<Box<str>, jint, BuildHasherDefault<Fnv>> = HashMap::default();
        let names = [
            "block_events",
            "block_entity_tick",
            "level_tick",
            "bench_empty",
            "",
        ];
        for (index, name) in names.iter().enumerate() {
            ops.insert((*name).into(), index as jint);
        }
        for (index, name) in names.iter().enumerate() {
            assert_eq!(ops.get(*name), Some(&(index as jint)));
        }
        assert_eq!(ops.get("client_tick"), None);
    }

    #[test]
    fn arenas_grow_by_powers_of_two() {
        assert_eq!(capacity_for(0), ARENA);
        assert_eq!(capacity_for(ARENA), ARENA);
        assert_eq!(capacity_for(ARENA + 1), ARENA * 2);
        assert_eq!(capacity_for(1 << 20), 1 << 20);
        assert_eq!(capacity_for((1 << 20) + 1), 1 << 21);
        assert!(capacity_after(capacity_for(10 << 20)) <= 1 << 20);
        assert_eq!(capacity_after(capacity_for(1 << 20)), 1 << 20);
        assert_eq!(capacity_after(capacity_for(1000)), ARENA);
    }

    #[test]
    fn nesting_is_restored_after_a_panic() {
        assert_eq!(DEPTH.get(), 0);
        let outer = Nesting::enter(0);
        assert_eq!(DEPTH.get(), 1);
        let panicked = std::panic::catch_unwind(|| {
            let _inner = Nesting::enter(1);
            assert_eq!(DEPTH.get(), 2);
            panic!("a read that panics");
        });
        assert!(panicked.is_err());
        assert_eq!(DEPTH.get(), 1);
        drop(outer);
        assert_eq!(DEPTH.get(), 0);
    }

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
