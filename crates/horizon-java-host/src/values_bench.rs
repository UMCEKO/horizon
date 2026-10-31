//! Natives of `ValuesBench.java`: what crossing into Rust costs against the JIT for the pure value methods.
use crate::bridge::{Arg, Dir, Point, Pos};
use horizon_core::geometry::{Aabb, ShapeOp, Vec3, VoxelShape, trig};
use horizon_core::noise::SimplexNoise;
use horizon_core::random::XoroshiroRandom;
use jni::JNIEnv;
use jni::objects::{JBooleanArray, JClass, JDoubleArray};
use jni::sys::{jboolean, jboolean as jbool, jdouble, jdoubleArray, jfloat, jint, jlong};
use std::cell::RefCell;

const BENCH: &str = "dev/horizonlab/javamods/boot/ValuesBench";

/// Called from `n_link` for the bench class, whose loader the JVM's `FindClass` can't see.
pub fn register(env: &mut JNIEnv, target: &JClass, name: &str) {
    if name != BENCH {
        return;
    }
    macro_rules! native {
        ($($java:literal, $sig:literal, $f:expr;)*) => {
            [$(jni::NativeMethod { name: $java.into(), sig: $sig.into(), fn_ptr: $f as *mut std::ffi::c_void }),*]
        };
    }
    let methods = native! {
        "nop", "(I)I", nop;
        "vec3Add", "([D[D)[D", vec3_add;
        "vec3DistSq", "(DDDDDD)D", vec3_dist_sq;
        "aabbMove", "([D[D)[D", aabb_move;
        "atan2", "(DD)D", atan2;
        "clamp", "(DDD)D", clamp;
        "lerp", "(DDD)D", lerp;
        "wrapDegrees", "(F)F", wrap_degrees;
        "vec3Length", "(DDD)D", vec3_length;
        "aabbIntersects", "(DDDDDDDDDDDD)Z", aabb_intersects;
        "blockRelative", "(JII)J", block_relative;
        "shapeNew", "([D[D[D[Z)J", shape_new;
        "joinNotEmpty", "(JJI)I", join_not_empty;
        "joinNotEmptyArrays", "([D[D[D[Z[D[D[D[ZI)I", join_not_empty_arrays;
        "joinNotEmptyMixed", "(J[D[D[D[ZI)I", join_not_empty_mixed;
        "joinUnoptimized", "(JJI)[D", join_unoptimized;
        "optimize", "(J)[D", optimize;
        "takeCells", "()[Z", take_cells;
        "clip", "(J[D[DJ)[D", clip;
        "simplexNew", "(JDDD)J", simplex_new;
        "simplexTable", "(J)[D", simplex_table;
        "simplex2", "(JDD)F", simplex2;
        "simplex3", "(JDDD)F", simplex3;
    };
    let _ = env.register_native_methods(target, &methods);
}

type Out = jdoubleArray;

fn doubles_out(env: &mut JNIEnv, values: &[f64]) -> Out {
    match env.new_double_array(values.len() as i32) {
        Ok(array) => {
            let _ = env.set_double_array_region(&array, 0, values);
            array.into_raw()
        }
        Err(_) => std::ptr::null_mut(),
    }
}

fn point(env: &mut JNIEnv, raw: &JDoubleArray) -> Vec3 {
    let Point([x, y, z]) = Point::from_raw(env, raw.as_raw());
    Vec3::new(x, y, z)
}

extern "system" fn nop(_env: JNIEnv, _class: JClass, x: jint) -> jint {
    x
}

extern "system" fn vec3_add(
    mut env: JNIEnv,
    _class: JClass,
    a: JDoubleArray,
    b: JDoubleArray,
) -> Out {
    let sum = point(&mut env, &a).add_vector(point(&mut env, &b));
    doubles_out(&mut env, &sum.components())
}

extern "system" fn vec3_dist_sq(
    _env: JNIEnv,
    _class: JClass,
    ax: jdouble,
    ay: jdouble,
    az: jdouble,
    bx: jdouble,
    by: jdouble,
    bz: jdouble,
) -> jdouble {
    Vec3::new(ax, ay, az).distance_squared(Vec3::new(bx, by, bz))
}

extern "system" fn aabb_move(
    mut env: JNIEnv,
    _class: JClass,
    aabb: JDoubleArray,
    by: JDoubleArray,
) -> Out {
    let mut b = [0.0; 6];
    let _ = env.get_double_array_region(&aabb, 0, &mut b);
    let offset = point(&mut env, &by);
    let moved = Aabb::new(Vec3::new(b[0], b[1], b[2]), Vec3::new(b[3], b[4], b[5])).move_by(offset);
    let (lo, hi) = (moved.min_position(), moved.max_position());
    doubles_out(&mut env, &[lo.x, lo.y, lo.z, hi.x, hi.y, hi.z])
}

extern "system" fn clamp(
    _env: JNIEnv,
    _class: JClass,
    v: jdouble,
    lo: jdouble,
    hi: jdouble,
) -> jdouble {
    if v < lo { lo } else { v.min(hi) }
}

extern "system" fn lerp(
    _env: JNIEnv,
    _class: JClass,
    a: jdouble,
    p: jdouble,
    q: jdouble,
) -> jdouble {
    p + a * (q - p)
}

extern "system" fn wrap_degrees(_env: JNIEnv, _class: JClass, angle: jfloat) -> jfloat {
    let mut wrapped = angle % 360.0;
    if wrapped >= 180.0 {
        wrapped -= 360.0;
    }
    if wrapped < -180.0 {
        wrapped += 360.0;
    }
    wrapped
}

extern "system" fn vec3_length(
    _env: JNIEnv,
    _class: JClass,
    x: jdouble,
    y: jdouble,
    z: jdouble,
) -> jdouble {
    Vec3::new(x, y, z).length()
}

#[allow(clippy::too_many_arguments)]
extern "system" fn aabb_intersects(
    _env: JNIEnv,
    _class: JClass,
    a0: jdouble,
    a1: jdouble,
    a2: jdouble,
    a3: jdouble,
    a4: jdouble,
    a5: jdouble,
    b0: jdouble,
    b1: jdouble,
    b2: jdouble,
    b3: jdouble,
    b4: jdouble,
    b5: jdouble,
) -> jboolean {
    let a = Aabb::new(Vec3::new(a0, a1, a2), Vec3::new(a3, a4, a5));
    let b = Aabb::new(Vec3::new(b0, b1, b2), Vec3::new(b3, b4, b5));
    a.intersects(b) as jboolean
}

extern "system" fn atan2(_env: JNIEnv, _class: JClass, y: jdouble, x: jdouble) -> jdouble {
    trig::atan2(y, x)
}

extern "system" fn block_relative(
    mut env: JNIEnv,
    _class: JClass,
    pos: jlong,
    dir: jint,
    steps: jint,
) -> jlong {
    let Pos(pos) = Pos::from_raw(&mut env, pos);
    let Dir(dir) = Dir::from_raw(&mut env, dir);
    let [dx, dy, dz] = dir.normal();
    pos.offset(dx * steps, dy * steps, dz * steps).pack()
}

fn doubles(env: &mut JNIEnv, array: &JDoubleArray) -> Vec<f64> {
    let mut out = vec![0.0; env.get_array_length(array).unwrap_or(0) as usize];
    let _ = env.get_double_array_region(array, 0, &mut out);
    out
}

fn build(
    env: &mut JNIEnv,
    x: &JDoubleArray,
    y: &JDoubleArray,
    z: &JDoubleArray,
    cells: &JBooleanArray,
) -> Option<VoxelShape> {
    let coords = [doubles(env, x), doubles(env, y), doubles(env, z)];
    let mut occupied = vec![0u8; env.get_array_length(cells).ok()? as usize];
    env.get_boolean_array_region(cells, 0, &mut occupied).ok()?;
    if occupied.is_empty() {
        return Some(VoxelShape::empty());
    }
    let occupied: Vec<bool> = occupied.into_iter().map(|c| c != 0).collect();
    VoxelShape::from_grid(coords, &occupied).ok()
}

extern "system" fn shape_new(
    mut env: JNIEnv,
    _class: JClass,
    x: JDoubleArray,
    y: JDoubleArray,
    z: JDoubleArray,
    cells: JBooleanArray,
) -> jlong {
    build(&mut env, &x, &y, &z, &cells).map_or(0, |s| Box::into_raw(Box::new(s)) as jlong)
}

fn shape<'a>(handle: jlong) -> &'a VoxelShape {
    // SAFETY: handles come from `shape_new` and are never freed during a run.
    unsafe { &*(handle as *const VoxelShape) }
}

fn op(truth: jint) -> ShapeOp {
    ShapeOp::from_truth_table(truth as u8)
}

extern "system" fn join_not_empty(
    _env: JNIEnv,
    _class: JClass,
    a: jlong,
    b: jlong,
    truth: jint,
) -> jint {
    match shape(a).join_is_not_empty(shape(b), op(truth)) {
        Ok(v) => v as jint,
        Err(_) => -1,
    }
}

#[allow(clippy::too_many_arguments)]
extern "system" fn join_not_empty_arrays(
    mut env: JNIEnv,
    _class: JClass,
    ax: JDoubleArray,
    ay: JDoubleArray,
    az: JDoubleArray,
    ac: JBooleanArray,
    bx: JDoubleArray,
    by: JDoubleArray,
    bz: JDoubleArray,
    bc: JBooleanArray,
    truth: jint,
) -> jint {
    let (Some(a), Some(b)) = (
        build(&mut env, &ax, &ay, &az, &ac),
        build(&mut env, &bx, &by, &bz, &bc),
    ) else {
        return -1;
    };
    match a.join_is_not_empty(&b, op(truth)) {
        Ok(v) => v as jint,
        Err(_) => -1,
    }
}

extern "system" fn join_not_empty_mixed(
    mut env: JNIEnv,
    _class: JClass,
    a: jlong,
    bx: JDoubleArray,
    by: JDoubleArray,
    bz: JDoubleArray,
    bc: JBooleanArray,
    truth: jint,
) -> jint {
    let Some(b) = build(&mut env, &bx, &by, &bz, &bc) else {
        return -1;
    };
    match shape(a).join_is_not_empty(&b, op(truth)) {
        Ok(v) => v as jint,
        Err(_) => -1,
    }
}

thread_local! {
    static CELLS: RefCell<Vec<bool>> = const { RefCell::new(Vec::new()) };
}

/// Sizes, then the three coordinate lists; the occupancy waits for `takeCells`.
fn shape_out(
    env: &mut JNIEnv,
    result: Result<VoxelShape, horizon_core::geometry::ShapeError>,
) -> Out {
    let Ok(result) = result else {
        return std::ptr::null_mut();
    };
    let sizes = result.sizes();
    let mut flat: Vec<f64> = sizes.iter().map(|s| *s as f64).collect();
    for axis in horizon_core::geometry::Axis::ALL {
        flat.extend_from_slice(result.coordinates(axis));
    }
    CELLS.with(|cells| {
        let mut cells = cells.borrow_mut();
        cells.clear();
        for x in 0..sizes[0] {
            for y in 0..sizes[1] {
                for z in 0..sizes[2] {
                    cells.push(result.is_full(x as i32, y as i32, z as i32));
                }
            }
        }
    });
    doubles_out(env, &flat)
}

extern "system" fn join_unoptimized(
    mut env: JNIEnv,
    _class: JClass,
    a: jlong,
    b: jlong,
    truth: jint,
) -> Out {
    shape_out(&mut env, shape(a).join_unoptimized(shape(b), op(truth)))
}

extern "system" fn optimize(mut env: JNIEnv, _class: JClass, a: jlong) -> Out {
    shape_out(&mut env, shape(a).optimized())
}

extern "system" fn take_cells(env: JNIEnv, _class: JClass) -> jni::sys::jbooleanArray {
    CELLS.with(|cells| {
        let cells = cells.borrow();
        let bytes: Vec<jboolean> = cells.iter().map(|c| *c as jbool).collect();
        match env.new_boolean_array(bytes.len() as i32) {
            Ok(array) => {
                let _ = env.set_boolean_array_region(&array, 0, &bytes);
                array.into_raw()
            }
            Err(_) => std::ptr::null_mut(),
        }
    })
}

/// x, y, z, direction ordinal, inside; null on a miss.
extern "system" fn clip(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    from: JDoubleArray,
    to: JDoubleArray,
    pos: jlong,
) -> Out {
    let (from, to) = (point(&mut env, &from), point(&mut env, &to));
    let Pos(block) = Pos::from_raw(&mut env, pos);
    match shape(handle).clip(from, to, block) {
        Some(hit) => doubles_out(
            &mut env,
            &[
                hit.location.x,
                hit.location.y,
                hit.location.z,
                hit.direction as u8 as f64,
                hit.inside as u8 as f64,
            ],
        ),
        None => std::ptr::null_mut(),
    }
}

/// The permutations come from the seed; the offsets come from the Java object, which Horizon's draws would round differently.
type Noise = (SimplexNoise, [f64; 3]);

extern "system" fn simplex_new(
    _env: JNIEnv,
    _class: JClass,
    seed: jlong,
    ox: jdouble,
    oy: jdouble,
    oz: jdouble,
) -> jlong {
    let noise = SimplexNoise::with_skipped_offsets(&mut XoroshiroRandom::new(seed), true);
    Box::into_raw(Box::new((noise, [ox, oy, oz]))) as jlong
}

fn noise<'a>(handle: jlong) -> &'a Noise {
    // SAFETY: handles come from `simplex_new` and are never freed during a run.
    unsafe { &*(handle as *const Noise) }
}

extern "system" fn simplex_table(mut env: JNIEnv, _class: JClass, h: jlong) -> Out {
    let table = noise(h).0.table();
    let mut flat: Vec<f64> = noise(h).1.to_vec();
    flat.extend(table.permutations().iter().map(|p| f64::from(*p)));
    doubles_out(&mut env, &flat)
}

extern "system" fn simplex2(
    _env: JNIEnv,
    _class: JClass,
    h: jlong,
    x: jdouble,
    y: jdouble,
) -> jfloat {
    let (n, o) = noise(h);
    n.get2(x + o[0], y + o[1])
}

extern "system" fn simplex3(
    _env: JNIEnv,
    _class: JClass,
    h: jlong,
    x: jdouble,
    y: jdouble,
    z: jdouble,
) -> jfloat {
    let (n, o) = noise(h);
    n.get3(x + o[0], y + o[1], z + o[2])
}
