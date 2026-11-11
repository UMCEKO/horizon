//! Java methods served by Horizon's Rust: one [`java_bridges!`](crate::java_bridges) entry per method,
//! which the class processor in `java-mods-host/plugin` jumps to from the method's first instruction.

use horizon_core::{geometry::Direction, position::BlockPos};
use horizon_world::registry::BlockStateId;
use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JDoubleArray, JString};
use jni::sys::{jboolean, jbyte, jbyteArray, jdouble, jdoubleArray, jfloat, jint, jlong, jstring};
use std::cell::Cell;
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;

pub struct Bridge {
    pub class: &'static str,
    pub method: &'static str,
    pub desc: &'static str,
    /// The method runs in Java when the receiver isn't this class; `@owner.method` names a static `(Object)Z` that must hold instead, and the native then goes on `owner`.
    pub guard: &'static str,
    pub native: &'static str,
    pub args: Vec<&'static str>,
    /// The `Flat` method that flattens each argument, `raw` for most.
    pub flats: Vec<&'static str>,
    pub ret: &'static str,
    /// The native is the method's body for a route stub, not the method's whole.
    pub route: bool,
    pub pointer: *mut c_void,
}
// Only a pointer to a function, which every thread may call.
unsafe impl Send for Bridge {}
unsafe impl Sync for Bridge {}

impl Bridge {
    /// One tab-separated line of the table the class processor reads.
    pub fn line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\thorizon$bridge${}\t{}\t{}\t{}",
            self.class,
            self.method,
            self.desc,
            self.guard,
            self.native,
            self.args
                .iter()
                .enumerate()
                .map(
                    |(i, raw)| match self.flats.get(i).copied().unwrap_or("raw") {
                        "raw" => (*raw).to_owned(),
                        flat => format!("{flat}:{raw}"),
                    }
                )
                .collect::<Vec<_>>()
                .join(","),
            self.ret,
            if self.route { "route" } else { "" }
        )
    }
    /// Where the native is declared: interfaces can't hold natives, so a guarded method's goes on the guard's class.
    fn holder(&self) -> &'static str {
        if let Some(predicate) = self.guard.strip_prefix('@') {
            predicate
                .rsplit_once('.')
                .map_or(predicate, |(owner, _)| owner)
        } else if self.guard.is_empty() {
            self.class
        } else {
            self.guard
        }
    }
    fn signature(&self) -> String {
        let args: String = self
            .args
            .iter()
            .map(|a| if *a == Skip::RAW { "B" } else { a })
            .collect();
        format!("({args}){}", self.ret)
    }
}

pub static TABLE: OnceLock<Vec<Bridge>> = OnceLock::new();

/// `Flat.link`, from the static initializer of a class with bridges: gives them their natives.
pub extern "system" fn n_link(mut env: JNIEnv, _class: JClass, target: JClass) {
    let name = env
        .call_method(&target, "getName", "()Ljava/lang/String;", &[])
        .and_then(|n| n.l())
        .map(JString::from)
        .map(|n| crate::js(&mut env, &n).replace('.', "/"))
        .unwrap_or_default();
    crate::values_bench::register(&mut env, &target, &name);
    let methods: Vec<_> = TABLE
        .get()
        .into_iter()
        .flatten()
        .filter(|b| b.holder() == name)
        .map(|b| jni::NativeMethod {
            name: format!("horizon$bridge${}", b.native).into(),
            sig: b.signature().into(),
            fn_ptr: b.pointer,
        })
        .collect();
    let _ = env.register_native_methods(&target, &methods);
}

thread_local! {
    static EARLY: Cell<bool> = const { Cell::new(false) };
}

/// Marks a body fn's return as one of vanilla's early returns, for the route stub.
pub fn early<T>(value: T) -> T {
    EARLY.set(true);
    value
}

/// `Host.earlyNative`, behind `Routing.early`: whether the body fn just returned one of vanilla's early returns.
pub extern "system" fn n_route_early(_env: JNIEnv, _class: JClass) -> jboolean {
    take_early() as jboolean
}

pub(crate) fn take_early() -> bool {
    EARLY.replace(false)
}

thread_local! {
    static ACTIVE: Cell<*mut ()> = const { Cell::new(std::ptr::null_mut()) };
}

/// Runs `f`, a call into Java, with `game` as what bridges on this thread act on.
pub fn with_active<G, R>(game: &mut G, f: impl FnOnce() -> R) -> R {
    let prior = ACTIVE.replace(std::ptr::from_mut(game).cast());
    let out = f();
    ACTIVE.set(prior);
    out
}

/// Runs `f`, a call into Java made where no `&mut` game is in reach: a native it reaches finds no game.
pub fn without_active<R>(f: impl FnOnce() -> R) -> R {
    let prior = ACTIVE.replace(std::ptr::null_mut());
    let out = f();
    ACTIVE.set(prior);
    out
}

/// What the game runs on the thread of a native that just returned to Java, with the game it acted on; it may call back into Java.
static AFTER_NATIVE: OnceLock<After> = OnceLock::new();
type After = (std::any::TypeId, Box<dyn Fn(*mut ()) + Send + Sync>);

/// Registers what runs after each native acting on a `G`, once.
pub fn after_native<G: 'static>(hook: fn(&mut G)) {
    let run = move |game: *mut ()| {
        // SAFETY: `enter` passes the `G` its native acted on, which no one else uses until it returns.
        hook(unsafe { &mut *game.cast::<G>() });
    };
    let _ = AFTER_NATIVE.set((std::any::TypeId::of::<G>(), Box::new(run)));
}

/// The body of a native: a Rust panic, or a call with no game behind it, becomes a Java exception.
pub fn enter<G: 'static, R: Ret>(
    mut env: JNIEnv,
    name: &str,
    f: impl FnOnce(&mut JNIEnv, &mut G) -> R,
) -> R::Raw {
    let out = catch_unwind(AssertUnwindSafe(|| {
        // A table for `()` is pure: it answers on any thread, with no game behind it.
        let game = if std::mem::size_of::<G>() == 0 {
            std::ptr::NonNull::<G>::dangling().as_ptr()
        } else {
            ACTIVE.get().cast::<G>()
        };
        // SAFETY: `with_active` holds the only `&mut G` for this call, and no one uses it until the call returns.
        (!game.is_null()).then(|| {
            let out = f(&mut env, unsafe { &mut *game });
            if let Some((kind, after)) = AFTER_NATIVE.get()
                && *kind == std::any::TypeId::of::<G>()
            {
                after(game.cast());
            }
            out.into_raw_in(&mut env)
        })
    }));
    match out {
        Ok(Some(raw)) => raw,
        Ok(None) => {
            let message = format!("{name}: outside a call from the game");
            let _ = env.throw_new("java/lang/IllegalStateException", message);
            R::Raw::default()
        }
        Err(_) => {
            let _ = env.throw_new(
                "java/lang/IllegalStateException",
                format!("{name}: panicked"),
            );
            R::Raw::default()
        }
    }
}

/// A native's argument: `RAW` is its JNI type; `Flat.raw` (or the `Flat` method `FLAT` names) flattens Java objects to it.
pub trait Arg: Sized {
    type Raw;
    const RAW: &'static str;
    const FLAT: &'static str = "raw";
    fn from_raw(env: &mut JNIEnv, raw: Self::Raw) -> Self;
}
pub trait Ret {
    type Raw: Default;
    const RAW: &'static str;
    fn into_raw(self) -> Self::Raw;
    /// A result that needs the JVM to build it (a string, an array) overrides this.
    fn into_raw_in(self, _env: &mut JNIEnv) -> Self::Raw
    where
        Self: Sized,
    {
        self.into_raw()
    }
}

macro_rules! same {
    ($($ty:ty, $raw:literal;)*) => {$(
        impl Arg for $ty {
            type Raw = $ty;
            const RAW: &'static str = $raw;
            fn from_raw(_: &mut JNIEnv, raw: $ty) -> $ty { raw }
        }
        impl Ret for $ty {
            type Raw = $ty;
            const RAW: &'static str = $raw;
            fn into_raw(self) -> $ty { self }
        }
    )*};
}
same! { jint, "I"; jlong, "J"; jfloat, "F"; jdouble, "D"; }

impl Arg for bool {
    type Raw = jboolean;
    const RAW: &'static str = "Z";
    fn from_raw(_: &mut JNIEnv, raw: jboolean) -> bool {
        raw != 0
    }
}
impl Ret for bool {
    type Raw = jboolean;
    const RAW: &'static str = "Z";
    fn into_raw(self) -> jboolean {
        self as jboolean
    }
}
impl Ret for () {
    type Raw = ();
    const RAW: &'static str = "V";
    fn into_raw(self) {}
}

/// A Java argument the port doesn't need; Java passes nothing worth flattening.
pub struct Skip;
impl Arg for Skip {
    type Raw = jbyte;
    const RAW: &'static str = "-";
    fn from_raw(_: &mut JNIEnv, _: jbyte) -> Skip {
        Skip
    }
}

/// A string, the form of anything a port names: a stat, an event, a heightmap.
pub struct Text(pub String);
impl Arg for Text {
    type Raw = jstring;
    const RAW: &'static str = "Ljava/lang/String;";
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn from_raw(env: &mut JNIEnv, raw: jstring) -> Text {
        // SAFETY: the JVM passed this reference to the native running now.
        let text = unsafe { JString::from_raw(raw) };
        Text(crate::js(env, &text))
    }
}

/// Bytes, the form of anything a port reads as NBT.
pub struct Bytes(pub Vec<u8>);
impl Arg for Bytes {
    type Raw = jbyteArray;
    const RAW: &'static str = "[B";
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn from_raw(env: &mut JNIEnv, raw: jbyteArray) -> Bytes {
        // SAFETY: the JVM passed this reference to the native running now.
        let bytes = unsafe { JByteArray::from_raw(raw) };
        Bytes(env.convert_byte_array(bytes).unwrap_or_default())
    }
}
/// A result a `Flat.to` method decodes, as NBT.
impl Ret for Bytes {
    type Raw = jbyteArray;
    const RAW: &'static str = "[B";
    fn into_raw(self) -> jbyteArray {
        std::ptr::null_mut()
    }
    fn into_raw_in(self, env: &mut JNIEnv) -> jbyteArray {
        env.byte_array_from_slice(&self.0)
            .map_or(std::ptr::null_mut(), JByteArray::into_raw)
    }
}

/// `Vec3`, as three doubles.
pub struct Point(pub [f64; 3]);
impl Arg for Point {
    type Raw = jdoubleArray;
    const RAW: &'static str = "[D";
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn from_raw(env: &mut JNIEnv, raw: jdoubleArray) -> Point {
        // SAFETY: the JVM passed this reference to the native running now.
        let values = unsafe { JDoubleArray::from_raw(raw) };
        let mut point = [0.0; 3];
        let _ = env.get_double_array_region(&values, 0, &mut point);
        Point(point)
    }
}

/// `AABB`, as min x y z then max x y z.
pub struct Bounds(pub [f64; 6]);
impl Arg for Bounds {
    type Raw = jdoubleArray;
    const RAW: &'static str = "[D";
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn from_raw(env: &mut JNIEnv, raw: jdoubleArray) -> Bounds {
        // SAFETY: the JVM passed this reference to the native running now.
        let values = unsafe { JDoubleArray::from_raw(raw) };
        let mut bounds = [0.0; 6];
        let _ = env.get_double_array_region(&values, 0, &mut bounds);
        Bounds(bounds)
    }
}

/// A result of plain numbers a `Flat.to` method builds an object from; empty when there is no answer.
pub struct Doubles(pub Vec<f64>);
impl Arg for Doubles {
    type Raw = jdoubleArray;
    const RAW: &'static str = "[D";
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn from_raw(env: &mut JNIEnv, raw: jdoubleArray) -> Doubles {
        // SAFETY: the JVM passed this reference to the native running now.
        let values = unsafe { JDoubleArray::from_raw(raw) };
        let mut out = vec![0.0; env.get_array_length(&values).unwrap_or(0).max(0) as usize];
        let _ = env.get_double_array_region(&values, 0, &mut out);
        Doubles(out)
    }
}
impl Ret for Doubles {
    type Raw = jdoubleArray;
    const RAW: &'static str = "[D";
    fn into_raw(self) -> jdoubleArray {
        std::ptr::null_mut()
    }
    fn into_raw_in(self, env: &mut JNIEnv) -> jdoubleArray {
        let Ok(array) = env.new_double_array(self.0.len() as i32) else {
            return std::ptr::null_mut();
        };
        let _ = env.set_double_array_region(&array, 0, &self.0);
        array.into_raw()
    }
}

/// A result of longs, as chunk positions or block positions packed by `asLong`.
pub struct Longs(pub Vec<i64>);
impl Ret for Longs {
    type Raw = jni::sys::jlongArray;
    const RAW: &'static str = "[J";
    fn into_raw(self) -> jni::sys::jlongArray {
        std::ptr::null_mut()
    }
    fn into_raw_in(self, env: &mut JNIEnv) -> jni::sys::jlongArray {
        let Ok(array) = env.new_long_array(self.0.len() as i32) else {
            return std::ptr::null_mut();
        };
        let _ = env.set_long_array_region(&array, 0, &self.0);
        array.into_raw()
    }
}

/// A string result: a registry entry's id, for `Flat.to` to look up.
impl Ret for Text {
    type Raw = jstring;
    const RAW: &'static str = "Ljava/lang/String;";
    fn into_raw(self) -> jstring {
        std::ptr::null_mut()
    }
    fn into_raw_in(self, env: &mut JNIEnv) -> jstring {
        env.new_string(self.0)
            .map_or(std::ptr::null_mut(), JString::into_raw)
    }
}

/// `VoxelShape`, by a handle to Rust's copy that `ShapeHandles` keeps for the calling thread.
pub struct Shape(pub &'static horizon_core::geometry::VoxelShape);
impl Arg for Shape {
    type Raw = jlong;
    const RAW: &'static str = "J";
    fn from_raw(_: &mut JNIEnv, raw: jlong) -> Shape {
        Shape(crate::shapes::shape_of(raw))
    }
}

/// A `DamageSource`: its type, the entities it names by id (0 for none), whether a creative player caused it, and where it came from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Source {
    pub kind: String,
    pub causing: i32,
    pub direct: i32,
    pub creative: bool,
    pub at: Option<[f64; 3]>,
}
impl Source {
    /// The bytes `Flat.raw(DamageSource)` writes.
    pub fn read(bytes: &[u8]) -> Option<Self> {
        let mut r = crate::wire::In(bytes);
        let kind = r.text()?.to_owned();
        let (causing, direct) = (r.int()?, r.int()?);
        let creative = r.bool()?;
        let at = if r.bool()? {
            Some([r.double()?, r.double()?, r.double()?])
        } else {
            None
        };
        Some(Self {
            kind,
            causing,
            direct,
            creative,
            at,
        })
    }
}
impl Arg for Source {
    type Raw = jbyteArray;
    const RAW: &'static str = "[B";
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn from_raw(env: &mut JNIEnv, raw: jbyteArray) -> Source {
        // SAFETY: the JVM passed this reference to the native running now.
        let bytes = unsafe { JByteArray::from_raw(raw) };
        let bytes = env.convert_byte_array(bytes).unwrap_or_default();
        Source::read(&bytes).unwrap_or_default()
    }
}

/// An entity, by `Entity.getId`, which is Horizon's id for a backed view.
#[derive(Clone, Copy)]
pub struct Ent(pub u32);
impl Arg for Ent {
    type Raw = jint;
    const RAW: &'static str = "I";
    fn from_raw(_: &mut JNIEnv, raw: jint) -> Ent {
        Ent(raw as u32)
    }
}

/// An entity, by `Entity.getUUID` (`Flat.rawUuid`), for what vanilla keeps as an `EntityReference`; None for null.
#[derive(Clone, Copy)]
pub struct EntUuid(pub Option<[i32; 4]>);
impl Arg for EntUuid {
    type Raw = jstring;
    const RAW: &'static str = "Ljava/lang/String;";
    const FLAT: &'static str = "rawUuid";
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn from_raw(env: &mut JNIEnv, raw: jstring) -> EntUuid {
        let Text(text) = Text::from_raw(env, raw);
        let bits = u128::from_str_radix(&text.replace('-', ""), 16).ok();
        EntUuid(bits.map(|v| [96, 64, 32, 0].map(|shift| (v >> shift) as u32 as i32)))
    }
}

/// `BlockPos`, by `BlockPos.asLong`.
pub struct Pos(pub BlockPos);
impl Arg for Pos {
    type Raw = jlong;
    const RAW: &'static str = "J";
    fn from_raw(_: &mut JNIEnv, raw: jlong) -> Pos {
        Pos(BlockPos::unpack(raw))
    }
}

/// The `ServerLevel` a level bridge was called on, by `Flat.level`: vanilla's three by index, another by its id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LevelOn {
    Overworld,
    Nether,
    End,
    Named(String),
}
impl Arg for LevelOn {
    type Raw = jint;
    const RAW: &'static str = "I";
    const FLAT: &'static str = "level";
    fn from_raw(env: &mut JNIEnv, raw: jint) -> LevelOn {
        match raw {
            0 => LevelOn::Overworld,
            1 => LevelOn::Nether,
            2 => LevelOn::End,
            other => LevelOn::Named(
                env.call_static_method(
                    "dev/horizonlab/javamods/boot/Flat",
                    "levelName",
                    "(I)Ljava/lang/String;",
                    &[jni::objects::JValue::Int(other)],
                )
                .and_then(|name| name.l())
                .map(|name| crate::js(env, &JString::from(name)))
                .unwrap_or_default(),
            ),
        }
    }
}

/// `Direction`, by ordinal.
pub struct Dir(pub Direction);
impl Arg for Dir {
    type Raw = jint;
    const RAW: &'static str = "I";
    fn from_raw(_: &mut JNIEnv, raw: jint) -> Dir {
        use Direction::{Down, East, North, South, Up, West};
        Dir([Down, Up, North, South, West, East][raw.rem_euclid(6) as usize])
    }
}

/// Java's block state ids against Horizon's, which differ once mods add blocks.
pub struct StateMap {
    pub to_horizon: Vec<Option<BlockStateId>>,
    pub to_java: Vec<i32>,
    pub java_air: i32,
    /// By Horizon's id: the block is a mod's, so its behaviour runs in Java.
    pub owned: Vec<bool>,
    /// By Horizon's id: the state has a block entity, which may shape it.
    pub entity: Vec<bool>,
    /// By Horizon's id: bit 1 the block's `animateTick` runs in Java, bit 2 the fluid's.
    pub animate: Vec<u8>,
    /// A mod patches the animate loop itself, so Java runs the whole of it.
    pub animate_whole: bool,
    /// Any state or the whole loop runs in Java.
    pub animate_any: bool,
}
pub static STATES: OnceLock<StateMap> = OnceLock::new();

/// Whether Java runs any `animateTick`, of a picked state or the whole loop.
#[inline]
pub fn java_animates() -> bool {
    STATES.get().is_some_and(|m| m.animate_any)
}

/// Whether Java runs the whole `ClientLevel.animateTick` loop, which native's then skips.
#[inline]
pub fn java_animates_whole() -> bool {
    STATES.get().is_some_and(|m| m.animate_whole)
}

/// Which parts of this state's `animateTick` Java runs; see `StateMap::animate`.
#[inline]
pub fn java_animate(state: BlockStateId) -> u8 {
    STATES
        .get()
        .map_or(0, |m| m.animate.get(state.0 as usize).copied().unwrap_or(0))
}

/// Whether this state's behaviour runs in Java.
#[inline]
pub fn java_block(state: BlockStateId) -> bool {
    STATES
        .get()
        .is_some_and(|m| m.owned.get(state.0 as usize).copied().unwrap_or(false))
}

/// Whether this state has a block entity.
#[inline]
pub fn java_has_block_entity(state: BlockStateId) -> bool {
    STATES
        .get()
        .is_some_and(|m| m.entity.get(state.0 as usize).copied().unwrap_or(false))
}

/// `BlockState`; None for a Java state with no Horizon block, as when a mod's blocks weren't carried over.
pub struct State(pub Option<BlockStateId>);
impl Arg for State {
    type Raw = jint;
    const RAW: &'static str = "I";
    fn from_raw(_: &mut JNIEnv, raw: jint) -> State {
        State(
            STATES
                .get()
                .and_then(|m| m.to_horizon.get(raw as usize).copied().flatten()),
        )
    }
}
/// `StateMap::to_java` of a Horizon state Java has none for.
pub const UNMAPPED: i32 = -1;

impl Ret for BlockStateId {
    type Raw = jint;
    const RAW: &'static str = "I";
    fn into_raw(self) -> jint {
        STATES.get().map_or(UNMAPPED, |m| {
            m.to_java.get(self.0 as usize).copied().unwrap_or(UNMAPPED)
        })
    }
    fn into_raw_in(self, env: &mut JNIEnv) -> jint {
        let java = self.into_raw();
        if java == UNMAPPED {
            let _ = env.throw_new(
                "java/lang/IllegalStateException",
                format!("Horizon: block state {} has no Java state", self.0),
            );
        }
        java
    }
}

/// One entry serves one Java method from Rust: the class, the method and its descriptor, then a body on `&mut Game`; a leading `route` makes it the method's body for a route stub.
#[macro_export]
macro_rules! java_bridges {
    ($table:ident for $game:ty { $(
        $($route:ident)? $class:literal . $method:ident $desc:literal $(if $guard:literal)? =>
        fn $name:ident($g:ident $(, $arg:ident: $ty:ty)* $(,)?) -> $ret:ty $body:block
    )* }) => {
        $(
            extern "system" fn $name(
                env: $crate::jni::JNIEnv,
                _class: $crate::jni::objects::JClass
                $(, $arg: <$ty as $crate::bridge::Arg>::Raw)*
            ) -> <$ret as $crate::bridge::Ret>::Raw {
                $crate::bridge::enter::<$game, $ret>(env, stringify!($name), move |java, $g| {
                    $(let $arg = <$ty as $crate::bridge::Arg>::from_raw(java, $arg);)*
                    $body
                })
            }
        )*
        pub(super) fn $table() -> Vec<$crate::bridge::Bridge> {
            vec![$($crate::bridge::Bridge {
                class: $class,
                method: stringify!($method),
                desc: $desc,
                guard: { let guard = ""; $(let guard = $guard;)? guard },
                native: stringify!($name),
                args: vec![$(<$ty as $crate::bridge::Arg>::RAW),*],
                flats: vec![$(<$ty as $crate::bridge::Arg>::FLAT),*],
                ret: <$ret as $crate::bridge::Ret>::RAW,
                route: { let route = false; $(let _ = stringify!($route); let route = true;)? route },
                pointer: $name as *mut std::ffi::c_void,
            }),*]
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_without_a_game_hides_the_active_one_and_gives_it_back() {
        let mut game = 7u64;
        let at = std::ptr::from_mut(&mut game).cast::<()>();
        let (inside, hidden, after) = with_active(&mut game, || {
            let inside = ACTIVE.get();
            let hidden = without_active(|| ACTIVE.get());
            (inside, hidden, ACTIVE.get())
        });
        assert_eq!(inside, at);
        assert!(hidden.is_null());
        assert_eq!(after, at);
        assert!(ACTIVE.get().is_null());
    }

    fn entry(guard: &'static str) -> Bridge {
        Bridge {
            class: "net/minecraft/world/entity/Entity",
            method: "discard",
            desc: "()V",
            guard,
            native: "entity_discard",
            args: vec![Ent::RAW],
            flats: vec![Ent::FLAT],
            ret: <() as Ret>::RAW,
            route: false,
            pointer: std::ptr::null_mut(),
        }
    }

    #[test]
    fn a_predicate_guard_puts_the_native_on_its_owner_and_the_receiver_is_an_id() {
        let bridge = entry("@dev/horizonlab/javamods/boot/EntityViews.backed");
        assert_eq!(bridge.holder(), "dev/horizonlab/javamods/boot/EntityViews");
        assert_eq!(
            bridge.line(),
            "net/minecraft/world/entity/Entity\tdiscard\t()V\t@dev/horizonlab/javamods/boot/EntityViews.backed\thorizon$bridge$entity_discard\tI\tV\t"
        );
        assert_eq!(bridge.signature(), "(I)V");
    }

    #[test]
    fn a_source_reads_what_flat_writes() {
        let mut out = crate::wire::Out::default();
        out.text("minecraft:lightning_bolt");
        out.int(0);
        out.int(7);
        out.bool(true);
        out.bool(true);
        for v in [1.0, 2.0, 3.0] {
            out.double(v);
        }
        let source = Source::read(&out.0).unwrap();
        assert_eq!(source.kind, "minecraft:lightning_bolt");
        assert_eq!(
            (source.causing, source.direct, source.creative),
            (0, 7, true)
        );
        assert_eq!(source.at, Some([1.0, 2.0, 3.0]));
        assert_eq!(Source::read(&out.0[..5]), None);
    }

    #[test]
    fn a_class_guard_and_no_guard_keep_their_holders() {
        assert_eq!(entry("").holder(), "net/minecraft/world/entity/Entity");
        assert_eq!(entry("a/B").holder(), "a/B");
    }

    #[allow(unused_variables)]
    mod table {
        crate::java_bridges! { table for () {
            route "a/B".m "(I)I" if "a/G" => fn route_m(g, x: i32) -> i32 { x }
            "a/C".n "()V" => fn plain_n(g) -> () {}
            "a/L".get "(J)I" if "a/S" => fn level_get(g, on: super::super::LevelOn, pos: super::super::Pos) -> i32 { 0 }
        } }
        pub fn lines() -> Vec<String> {
            table().iter().map(super::super::Bridge::line).collect()
        }
    }

    #[test]
    fn a_route_entry_ends_its_line_with_route() {
        let lines = table::lines();
        assert_eq!(
            lines[0],
            "a/B\tm\t(I)I\ta/G\thorizon$bridge$route_m\tI\tI\troute"
        );
        assert_eq!(lines[1], "a/C\tn\t()V\t\thorizon$bridge$plain_n\t\tV\t");
    }

    #[test]
    fn a_level_receiver_goes_first_through_flat_level() {
        assert_eq!(
            table::lines()[2],
            "a/L\tget\t(J)I\ta/S\thorizon$bridge$level_get\tlevel:I,J\tI\t"
        );
    }

    #[test]
    fn early_is_taken_once() {
        assert!(!super::take_early());
        assert_eq!(super::early(5), 5);
        assert!(super::take_early());
        assert!(!super::take_early());
    }
}
