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
    /// The method runs in Java when the receiver isn't this class.
    pub guard: &'static str,
    pub native: &'static str,
    pub args: Vec<&'static str>,
    pub ret: &'static str,
    pub pointer: *mut c_void,
}
// Only a pointer to a function, which every thread may call.
unsafe impl Send for Bridge {}
unsafe impl Sync for Bridge {}

impl Bridge {
    /// One tab-separated line of the table the class processor reads.
    pub fn line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\thorizon$bridge${}\t{}\t{}",
            self.class,
            self.method,
            self.desc,
            self.guard,
            self.native,
            self.args.join(","),
            self.ret
        )
    }
    /// Where the native is declared: interfaces can't hold natives, so a guarded method's goes on the guard's class.
    fn holder(&self) -> &'static str {
        if self.guard.is_empty() {
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
    static ACTIVE: Cell<*mut ()> = const { Cell::new(std::ptr::null_mut()) };
}

/// Runs `f`, a call into Java, with `game` as what bridges on this thread act on.
pub fn with_active<G, R>(game: &mut G, f: impl FnOnce() -> R) -> R {
    let prior = ACTIVE.replace(std::ptr::from_mut(game).cast());
    let out = f();
    ACTIVE.set(prior);
    out
}

/// The body of a native: a Rust panic, or a call with no game behind it, becomes a Java exception.
pub fn enter<G, R: Ret>(
    mut env: JNIEnv,
    name: &str,
    f: impl FnOnce(&mut JNIEnv, &mut G) -> R,
) -> R::Raw {
    let out = catch_unwind(AssertUnwindSafe(|| {
        let game = ACTIVE.get().cast::<G>();
        // SAFETY: `with_active` holds the only `&mut G` for this call, and no one uses it until the call returns.
        (!game.is_null()).then(|| f(&mut env, unsafe { &mut *game }).into_raw())
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

/// A native's argument: `RAW` is its JNI type; `Flat.raw` flattens Java objects to it.
pub trait Arg: Sized {
    type Raw;
    const RAW: &'static str;
    fn from_raw(env: &mut JNIEnv, raw: Self::Raw) -> Self;
}
pub trait Ret {
    type Raw: Default;
    const RAW: &'static str;
    fn into_raw(self) -> Self::Raw;
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

/// `BlockPos`, by `BlockPos.asLong`.
pub struct Pos(pub BlockPos);
impl Arg for Pos {
    type Raw = jlong;
    const RAW: &'static str = "J";
    fn from_raw(_: &mut JNIEnv, raw: jlong) -> Pos {
        Pos(BlockPos::unpack(raw))
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
}
pub static STATES: OnceLock<StateMap> = OnceLock::new();

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
impl Ret for BlockStateId {
    type Raw = jint;
    const RAW: &'static str = "I";
    fn into_raw(self) -> jint {
        STATES
            .get()
            .map(|m| {
                m.to_java
                    .get(self.0 as usize)
                    .copied()
                    .unwrap_or(m.java_air)
            })
            .unwrap_or_default()
    }
}

/// One entry serves one Java method from Rust: the class, the method and its descriptor, then a body on `&mut Game`.
#[macro_export]
macro_rules! java_bridges {
    ($table:ident for $game:ty { $(
        $class:literal . $method:ident $desc:literal $(if $guard:literal)? =>
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
                ret: <$ret as $crate::bridge::Ret>::RAW,
                pointer: $name as *mut std::ffi::c_void,
            }),*]
        }
    };
}
