//! Heavy vanilla shape work served from Horizon's parity-tested geometry.
use horizon_core::geometry::{Direction, VoxelShape};
use horizon_world::block_support::SupportType;
use jni::JNIEnv;
use jni::objects::{JBooleanArray, JClass, JDoubleArray};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

type Key = ([Vec<u64>; 3], Vec<u8>);
static MEMO: LazyLock<Mutex<HashMap<Key, i32>>> = LazyLock::new(Default::default);

const SUPPORTS: [SupportType; 3] = [SupportType::Full, SupportType::Center, SupportType::Rigid];

/// Three bits per direction in Java's ordinals: direction * 3 + support type.
fn flags(coords: [Vec<f64>; 3], occupied: &[bool]) -> Option<i32> {
    let shape = if occupied.is_empty() {
        VoxelShape::empty()
    } else {
        VoxelShape::from_grid(coords, occupied).ok()?
    };
    let mut flags = 0;
    for direction in Direction::ALL {
        for (i, support) in SUPPORTS.into_iter().enumerate() {
            if support.is_supporting(&shape, direction).ok()? {
                flags |= 1 << (direction as i32 * 3 + i as i32);
            }
        }
    }
    Some(flags)
}

fn doubles(env: &mut JNIEnv, array: &JDoubleArray) -> Option<Vec<f64>> {
    let mut out = vec![0.0; env.get_array_length(array).ok()? as usize];
    env.get_double_array_region(array, 0, &mut out).ok()?;
    Some(out)
}

fn read(
    env: &mut JNIEnv,
    x: &JDoubleArray,
    y: &JDoubleArray,
    z: &JDoubleArray,
    occupied: &JBooleanArray,
) -> Option<i32> {
    let coords = [doubles(env, x)?, doubles(env, y)?, doubles(env, z)?];
    let mut cells = vec![0u8; env.get_array_length(occupied).ok()? as usize];
    env.get_boolean_array_region(occupied, 0, &mut cells).ok()?;
    let key = (
        coords
            .each_ref()
            .map(|c| c.iter().map(|v| v.to_bits()).collect()),
        cells,
    );
    if let Some(known) = MEMO.lock().ok()?.get(&key) {
        return Some(*known);
    }
    let full: Vec<bool> = key.1.iter().map(|c| *c != 0).collect();
    let result = flags(coords, &full)?;
    MEMO.lock().ok()?.insert(key, result);
    Some(result)
}

pub extern "system" fn n_shape_flags(
    mut env: JNIEnv,
    _class: JClass,
    x: JDoubleArray,
    y: JDoubleArray,
    z: JDoubleArray,
    occupied: JBooleanArray,
) -> i32 {
    read(&mut env, &x, &y, &z, &occupied).unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_block_supports_every_type_on_every_face() {
        assert_eq!(
            flags([vec![0.0, 1.0], vec![0.0, 1.0], vec![0.0, 1.0]], &[true]),
            Some((1 << 18) - 1)
        );
    }

    #[test]
    fn an_empty_shape_supports_nothing() {
        assert_eq!(flags([vec![0.0], vec![0.0], vec![0.0]], &[]), Some(0));
    }

    #[test]
    fn a_bottom_slab_is_full_only_below_and_centers_on_top_fail() {
        let flags = flags([vec![0.0, 1.0], vec![0.0, 0.5], vec![0.0, 1.0]], &[true]).unwrap();
        let down = Direction::Down as i32 * 3;
        assert_eq!(flags >> down & 0b111, 0b111);
        assert_eq!(flags >> (Direction::Up as i32 * 3) & 0b111, 0);
    }
}
