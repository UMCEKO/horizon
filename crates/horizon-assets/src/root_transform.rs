//! NeoForge's root `transform` of a model: an affine on all its geometry, applied before the blockstate's rotation.
use crate::json::*;
use crate::model::Direction;
use serde_json::value::RawValue;

/// Rows `[x, y, z, translation]` of the affine, in block units.
pub type RootTransform = [[f32; 4]; 3];

type Matrix = [[f32; 4]; 4];

const IDENTITY: Matrix = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

fn mul(a: &Matrix, b: &Matrix) -> Matrix {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..4).map(|k| a[r][k] * b[k][c]).sum()))
}

fn translation([x, y, z]: [f32; 3]) -> Matrix {
    let mut m = IDENTITY;
    m[0][3] = x;
    m[1][3] = y;
    m[2][3] = z;
    m
}

fn scale([x, y, z]: [f32; 3]) -> Matrix {
    let mut m = IDENTITY;
    m[0][0] = x;
    m[1][1] = y;
    m[2][2] = z;
    m
}

fn rotation([x, y, z, w]: [f32; 4]) -> Matrix {
    let n = (x * x + y * y + z * z + w * w).sqrt();
    let (x, y, z, w) = if n > 0.0 {
        (x / n, y / n, z / n, w / n)
    } else {
        (0.0, 0.0, 0.0, 1.0)
    };
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
            0.0,
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
            0.0,
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
            0.0,
        ],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn axis(name: &str, degrees: f32) -> Result<Matrix> {
    let (s, c) = (degrees.to_radians() / 2.0).sin_cos();
    rotation_of(match name {
        "x" => [s, 0.0, 0.0, c],
        "y" => [0.0, s, 0.0, c],
        "z" => [0.0, 0.0, s, c],
        _ => return Err(error("invalid rotation axis")),
    })
}

fn rotation_of(q: [f32; 4]) -> Result<Matrix> {
    Ok(rotation(q))
}

/// A rotation: a quaternion `[x, y, z, w]`, XYZ degrees `[x, y, z]`, `{"axis": degrees}`, or a list of those applied in order.
fn rotation_value(raw: &RawValue) -> Result<Matrix> {
    let text = raw.get().trim_start();
    if text.starts_with('{') {
        let mut m = IDENTITY;
        for (name, value) in ordered_object(raw)? {
            m = mul(&m, &axis(&name, float(value)?)?);
        }
        return Ok(m);
    }
    let items = array(raw)?;
    if items
        .iter()
        .all(|item| item.get().trim_start().starts_with('{'))
    {
        let mut m = IDENTITY;
        for item in items {
            m = mul(&m, &rotation_value(item)?);
        }
        return Ok(m);
    }
    match items.len() {
        4 => Ok(rotation(vector::<4>(raw)?)),
        3 => {
            let [x, y, z] = vector::<3>(raw)?;
            Ok(mul(&mul(&axis("x", x)?, &axis("y", y)?), &axis("z", z)?))
        }
        _ => Err(error("invalid rotation")),
    }
}

/// `"center"`, `"corner"`, `"opposing-corner"` or a point.
fn origin(raw: &RawValue) -> Result<[f32; 3]> {
    if raw.get().trim_start().starts_with('"') {
        return match primitive_string(raw)?.as_str() {
            "center" => Ok([0.5; 3]),
            "corner" => Ok([0.0; 3]),
            "opposing-corner" => Ok([1.0; 3]),
            _ => Err(error("invalid transform origin")),
        };
    }
    vector::<3>(raw)
}

/// A `transform` property: a 3x4 `matrix`, or translation, left rotation, scale and right rotation about an origin.
pub fn decode(raw: &RawValue) -> Result<RootTransform> {
    let o = object(raw)?;
    if let Some(matrix) = o.get("matrix") {
        let rows = array(matrix)?;
        if rows.len() != 3 {
            return Err(error("a transform matrix has three rows"));
        }
        return Ok([
            vector::<4>(rows[0])?,
            vector::<4>(rows[1])?,
            vector::<4>(rows[2])?,
        ]);
    }
    let get = |a: &str, b: &str| o.get(a).or_else(|| o.get(b)).copied();
    let t = o
        .get("translation")
        .map(|r| vector::<3>(r))
        .transpose()?
        .unwrap_or([0.0; 3]);
    let left = get("rotation", "left_rotation")
        .map(rotation_value)
        .transpose()?
        .unwrap_or(IDENTITY);
    let s = match o.get("scale") {
        Some(raw) if raw.get().trim_start().starts_with('[') => vector::<3>(raw)?,
        Some(raw) => [float(raw)?; 3],
        None => [1.0; 3],
    };
    let right = get("post_rotation", "right_rotation")
        .map(rotation_value)
        .transpose()?
        .unwrap_or(IDENTITY);
    let at = o
        .get("origin")
        .map(|r| origin(r))
        .transpose()?
        .unwrap_or([1.0; 3]);
    let core = mul(&mul(&mul(&translation(t), &left), &scale(s)), &right);
    let m = mul(&mul(&translation(at), &core), &translation(at.map(|c| -c)));
    Ok([m[0], m[1], m[2]])
}

pub fn position(m: &RootTransform, [x, y, z]: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[r][0] * x + m[r][1] * y + m[r][2] * z + m[r][3])
}

/// The face direction nearest to where the transform turns `direction`.
pub fn direction(m: &RootTransform, direction: Direction) -> Direction {
    let v = match direction {
        Direction::Down => [0.0, -1.0, 0.0],
        Direction::Up => [0.0, 1.0, 0.0],
        Direction::North => [0.0, 0.0, -1.0],
        Direction::South => [0.0, 0.0, 1.0],
        Direction::West => [-1.0, 0.0, 0.0],
        Direction::East => [1.0, 0.0, 0.0],
    };
    let [x, y, z]: [f32; 3] =
        std::array::from_fn(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2]);
    let mut best = (f32::MIN, direction);
    for (candidate, dot) in Direction::ALL.into_iter().zip([-y, y, -z, z, -x, x]) {
        if dot > best.0 {
            best = (dot, candidate);
        }
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> RootTransform {
        let raw: Box<RawValue> = serde_json::from_str(text).unwrap();
        decode(&raw).unwrap()
    }

    #[test]
    fn a_translation_moves_every_point_and_keeps_faces() {
        let m = parse(r#"{"translation":[0.5,0,0.5]}"#);
        assert_eq!(position(&m, [0.0, 1.0, 0.0]), [0.5, 1.0, 0.5]);
        assert_eq!(direction(&m, Direction::North), Direction::North);
    }

    #[test]
    fn rotation_and_scale_turn_about_the_origin() {
        let m = parse(r#"{"rotation":{"y":90},"origin":"center"}"#);
        let p = position(&m, [1.0, 0.5, 0.5]);
        assert!(
            p.iter()
                .zip([0.5, 0.5, 0.0])
                .all(|(a, b)| (a - b).abs() < 1e-5),
            "{p:?}"
        );
        assert_eq!(direction(&m, Direction::East), Direction::North);
        let s = parse(r#"{"scale":0.5,"origin":"corner"}"#);
        assert_eq!(position(&s, [1.0, 1.0, 1.0]), [0.5, 0.5, 0.5]);
        let opposing = parse(r#"{"scale":[0.5,0.5,0.5]}"#);
        assert_eq!(position(&opposing, [0.0, 0.0, 0.0]), [0.5, 0.5, 0.5]);
    }

    #[test]
    fn a_matrix_is_its_rows() {
        let m = parse(r#"{"matrix":[[1,0,0,2],[0,1,0,0],[0,0,1,0]]}"#);
        assert_eq!(position(&m, [1.0, 1.0, 1.0]), [3.0, 1.0, 1.0]);
    }
}
