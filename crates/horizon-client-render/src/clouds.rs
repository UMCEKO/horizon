//! Cloud visibility and flat/fancy cell geometry (`CloudRenderer`).
use std::sync::{Arc, mpsc};

/// `EnvironmentAttributes.CLOUD_HEIGHT`'s default, and the overworld's `visual/cloud_height`.
pub const CLOUD_HEIGHT: f64 = 192.33;

/// Whether the cloud color has any packed alpha (vanilla gates the clouds on it).
pub fn has_color(alpha: f32) -> bool {
    (alpha.clamp(0.0, 1.0) * 255.0) as u8 > 0
}

/// `LevelRenderer`: skip OFF clouds and colors with no packed alpha.
pub fn visible(mode: &str, alpha: f32) -> bool {
    mode != "false" && has_color(alpha)
}

/// `buildFlatCell`: bottom plane, in both windings for the culled cloud pipeline.
pub fn flat_cell(x: f32, z: f32, bottom: f32) -> [[[f32; 3]; 4]; 2] {
    let p = [
        [x + 12.0, bottom, z],
        [x + 12.0, bottom, z + 12.0],
        [x, bottom, z + 12.0],
        [x, bottom, z],
    ];
    [p, [p[3], p[2], p[1], p[0]]]
}

/// Direction order is DOWN, UP, NORTH, SOUTH, WEST, EAST; inward faces use bits 6..12.
pub fn face_mask(x: i32, z: i32, bottom: f32, empty: [bool; 4]) -> u16 {
    let mut mask = 0;
    if bottom <= 0.0 {
        mask |= 1 << 1;
    }
    if bottom + 4.0 >= 0.0 {
        mask |= 1;
    }
    for (i, visible) in [z > 0, z < 0, x > 0, x < 0].into_iter().enumerate() {
        if empty[i] && visible {
            mask |= 1 << (i + 2);
        }
    }
    if x.abs() <= 1 && z.abs() <= 1 {
        mask |= 63 << 6;
    }
    mask
}

/// Exterior faces first, then reversed interior faces, in vanilla emission order.
pub fn faces(mask: u16) -> impl Iterator<Item = (u8, bool)> {
    [1, 0, 2, 3, 4, 5]
        .into_iter()
        .filter(move |d| mask & (1 << d) != 0)
        .map(|d| (d, false))
        .chain(
            (0..6)
                .filter(move |d| mask & (1 << (d + 6)) != 0)
                .map(|d| (d, true)),
        )
}

/// `CloudRenderer.prepare`: the cell radius for the Cloud Distance option
/// (in chunks), independent of the render distance.
pub fn radius_cells(cloud_range_chunks: i32) -> i32 {
    ((cloud_range_chunks * 16) as f32 / 12.0).ceil() as i32
}

/// Cells the mesh's origin snaps to. The mesh is a window on the cloud pattern around the
/// camera's cell, so it used to be rebuilt, and the whole array re-uploaded, on every cell
/// crossed: 6.5 MB, about 11 ms at the default 128-chunk range, the worst frame of a flight.
/// Anchoring the origin to a coarser grid rebuilds it once every `SNAP_CELLS` cells instead,
/// and `Uniform::offset` carries the remaining shift. The ring of cells the shift can leave
/// uncovered at the far side lies past the cloud fog end, where the cloud fade has already
/// taken the alpha to 0 (`entity.wgsl`'s `fs_entity`), so the extra cells cost only the
/// vertices they add.
const SNAP_CELLS: i64 = 4;

#[derive(Clone)]
pub struct CellIter {
    radius: i32,
    ring: i32,
    x: i32,
    pending_z: Option<i32>,
}

impl CellIter {
    pub fn new(radius: i32) -> Self {
        Self {
            radius,
            ring: 0,
            x: 0,
            pending_z: None,
        }
    }
}

impl Iterator for CellIter {
    type Item = (i32, i32);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(z) = self.pending_z.take() {
            let x = self.x;
            self.x += 1;
            return Some((x, z));
        }
        let r2 = self.radius * self.radius;
        while self.ring <= 2 * self.radius {
            let min_abs = self.ring - self.radius;
            while self.x <= self.ring {
                if min_abs > 0 && self.x > -min_abs && self.x < min_abs {
                    self.x = min_abs;
                }
                let x = self.x;
                let z = self.ring - x.abs();
                if z <= self.radius && x * x + z * z <= r2 {
                    if z != 0 {
                        self.pending_z = Some(z);
                        return Some((x, -z));
                    } else {
                        self.x += 1;
                        return Some((x, 0));
                    }
                }
                self.x += 1;
            }
            self.ring += 1;
            self.x = -self.ring;
        }
        None
    }
}

/// CloudRenderer builds cells in expanding Manhattan rings, nearest first.
pub fn cells(radius: i32) -> CellIter {
    CellIter::new(radius)
}

/// Per-frame cloud inputs. Resources are immutable for the renderer's lifetime:
/// resource-pack reloads replace the renderer and its cache together.
pub struct Input<'a> {
    pub texture: &'a (u32, u32, Vec<bool>),
    pub uv: [f32; 2],
    pub camera: glam::DVec3,
    pub time: i64,
    pub partial_tick: f64,
    pub range: i32,
    pub fast: bool,
    pub color: [f32; 4],
}

type Texture = (u32, u32, Vec<bool>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MeshKey {
    cell_x: i64,
    cell_z: i64,
    radius: i32,
    fast: bool,
    // The two vertical face tests include both surfaces at the boundaries.
    top: bool,
    bottom: bool,
    // The camera's cell within the snapped origin's, for the 3x3 cells whose inward
    // faces are drawn. Only read while the camera is inside the layer.
    sub_x: i32,
    sub_z: i32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Uniform {
    offset: [f32; 4],
    colors: [[f32; 4]; 4],
}

/// How far the clouds have drifted, in ticks: the game time within one trip across the cloud
/// texture (`width` cells of 400 ticks), plus the partial tick (`CloudRenderer.prepare`).
pub fn drift(time: i64, partial_tick: f64, width: u32) -> f64 {
    time.rem_euclid(i64::from(width) * 400) as f64 + partial_tick
}

impl Input<'_> {
    fn state(&self) -> (MeshKey, Uniform) {
        let (w, h, _) = self.texture;
        let offset = drift(self.time, self.partial_tick, *w);
        let mut x = self.camera.x + offset * 0.030_000_001;
        let mut z = self.camera.z + 3.96;
        let (tw, th) = (f64::from(*w) * 12.0, f64::from(*h) * 12.0);
        x -= (x / tw).floor() * tw;
        z -= (z / th).floor() * th;
        let cell_x = (x / 12.0).floor() as i64 / SNAP_CELLS * SNAP_CELLS;
        let cell_z = (z / 12.0).floor() as i64 / SNAP_CELLS * SNAP_CELLS;
        let bottom = (CLOUD_HEIGHT - self.camera.y) as f32;
        // The inward faces of the 3x3 cells around the camera are only seen from inside the
        // layer; outside it they are back-facing, so the mesh need not follow the camera there.
        let inside_layer = bottom <= 0.0 && bottom + 4.0 >= 0.0;
        let (sub_x, sub_z) = if inside_layer {
            (
                (x / 12.0).floor() as i32 - cell_x as i32,
                (z / 12.0).floor() as i32 - cell_z as i32,
            )
        } else {
            (0, 0)
        };
        let key = MeshKey {
            cell_x,
            cell_z,
            // One more ring than the range, for the cells the snapped origin can uncover.
            radius: radius_cells(self.range) + SNAP_CELLS as i32,
            fast: self.fast,
            top: bottom <= 0.0,
            bottom: bottom + 4.0 >= 0.0,
            sub_x,
            sub_z,
        };
        let colors = [1.0, 0.7, 0.8, 0.9].map(|shade| {
            let [r, g, b, a] = self.color;
            [r * shade, g * shade, b * shade, a]
                .map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8 as f32 / 255.0)
        });
        (
            key,
            Uniform {
                offset: [
                    -(x - cell_x as f64 * 12.0) as f32,
                    bottom,
                    -(z - cell_z as f64 * 12.0) as f32,
                    0.0,
                ],
                colors,
            },
        )
    }
}

/// How close to a cell edge (in blocks) the camera must be before the next cell's mesh is prefetched.
const PREFETCH_MARGIN: f32 = 3.0;

struct Request {
    key: MeshKey,
    bottom: f32,
    uv: [f32; 2],
    texture: Arc<Texture>,
    cells: Arc<Vec<(i32, i32)>>,
    out: Vec<crate::mesher::Vertex>,
}

struct Built {
    key: MeshKey,
    uv: [f32; 2],
    texture: Arc<Texture>,
    vertices: Vec<crate::mesher::Vertex>,
}

struct Prefetch {
    requests: mpsc::Sender<Request>,
    results: mpsc::Receiver<Built>,
    in_flight: bool,
}

impl Prefetch {
    fn spawn() -> Option<Self> {
        let (requests, request_rx) = mpsc::channel::<Request>();
        let (result_tx, results) = mpsc::channel();
        std::thread::Builder::new()
            .name("Cloud mesh".into())
            .spawn(move || {
                while let Ok(mut r) = request_rx.recv() {
                    build(&r.texture, &r.key, &r.cells, r.bottom, r.uv, &mut r.out);
                    let built = Built {
                        key: r.key,
                        uv: r.uv,
                        texture: r.texture,
                        vertices: r.out,
                    };
                    if result_tx.send(built).is_err() {
                        break;
                    }
                }
            })
            .ok()?;
        Some(Self {
            requests,
            results,
            in_flight: false,
        })
    }
}

/// The cloud cells around a snapped origin, rebuilt when the key changes: once every
/// [`SNAP_CELLS`] cells the camera crosses, and every cell while the camera is inside the
/// layer, where the inward faces follow it. The renderer copies the vertices to the GPU on
/// a rebuild, which is why the origin snaps at all.
#[derive(Default)]
pub(crate) struct Mesh {
    key: Option<MeshKey>,
    cells: Arc<Vec<(i32, i32)>>,
    cached_radius: Option<i32>,
    pub vertices: Vec<crate::mesher::Vertex>,
    texture: Option<Arc<Texture>>,
    prefetch: Option<Prefetch>,
    prefetch_off: bool,
    ready: Option<Built>,
    spare: Vec<crate::mesher::Vertex>,
    last_pos: Option<(f64, f64)>,
    #[cfg(test)]
    hits: u32,
}

/// The key after the camera's next cell crossing (the nearer edge in time), from its place in
/// the snapped origin's window and its last motion.
fn predicted_key(
    key: &MeshKey,
    (fx, fz): (f32, f32),
    (dx, dz): (f64, f64),
    (w, h): (u32, u32),
) -> MeshKey {
    let cell = |f: f32| (f / 12.0).floor() as i64;
    let toward = |f: f32, d: f64| {
        let in_cell = f - cell(f) as f32 * 12.0;
        let (step, edge) = if d > 0.0 || (d == 0.0 && in_cell >= 6.0) {
            (1, 12.0 - f64::from(in_cell))
        } else {
            (-1, f64::from(in_cell))
        };
        (edge < f64::from(PREFETCH_MARGIN)).then_some((step, edge / d.abs().max(1e-9)))
    };
    let (mut sx, mut sz) = (0, 0);
    match (toward(fx, dx), toward(fz, dz)) {
        (Some((x, tx)), Some((z, tz))) => {
            if (tx - tz).abs() < 1.0 {
                (sx, sz) = (x, z);
            } else if tx < tz {
                sx = x;
            } else {
                sz = z;
            }
        }
        (Some((x, _)), None) => sx = x,
        (None, Some((z, _))) => sz = z,
        (None, None) => {}
    }
    let inside_layer = key.top && key.bottom;
    let snapped = |origin: i64, f: f32, step: i64, period: u32| {
        let next = (origin + cell(f) + step).rem_euclid(i64::from(period));
        let snap = next / SNAP_CELLS * SNAP_CELLS;
        (snap, if inside_layer { (next - snap) as i32 } else { 0 })
    };
    let (cell_x, sub_x) = snapped(key.cell_x, fx, sx, w);
    let (cell_z, sub_z) = snapped(key.cell_z, fz, sz, h);
    MeshKey {
        cell_x,
        cell_z,
        sub_x,
        sub_z,
        ..*key
    }
}

#[inline(always)]
fn push_quad(
    vertices: &mut Vec<crate::mesher::Vertex>,
    p: [[f32; 3]; 4],
    shade: u32,
    normal: [i8; 4],
    uv: [f32; 2],
) {
    for pos in p {
        vertices.push(crate::mesher::Vertex {
            normal,
            pos,
            uv,
            layer: 4,
            color: u32::MAX,
            light: shade,
            glint_uv: [0.0, 0.0],
        });
    }
}

#[inline(always)]
fn emit_face(
    vertices: &mut Vec<crate::mesher::Vertex>,
    direction: usize,
    inside: bool,
    x0: f32,
    z0: f32,
    uv: [f32; 2],
) {
    let x1 = x0 + 12.0;
    let z1 = z0 + 12.0;
    const Y0: f32 = 0.0;
    const Y1: f32 = 4.0;
    let (p, shade, normal) = match direction {
        0 => (
            [[x1, Y0, z0], [x1, Y0, z1], [x0, Y0, z1], [x0, Y0, z0]],
            1,
            [0, -127, 0, 0],
        ),
        1 => (
            [[x0, Y1, z0], [x0, Y1, z1], [x1, Y1, z1], [x1, Y1, z0]],
            0,
            [0, 127, 0, 0],
        ),
        2 => (
            [[x0, Y0, z0], [x0, Y1, z0], [x1, Y1, z0], [x1, Y0, z0]],
            2,
            [0, 0, -127, 0],
        ),
        3 => (
            [[x1, Y0, z1], [x1, Y1, z1], [x0, Y1, z1], [x0, Y0, z1]],
            2,
            [0, 0, 127, 0],
        ),
        4 => (
            [[x0, Y0, z1], [x0, Y1, z1], [x0, Y1, z0], [x0, Y0, z0]],
            3,
            [-127, 0, 0, 0],
        ),
        _ => (
            [[x1, Y0, z0], [x1, Y1, z0], [x1, Y1, z1], [x1, Y0, z1]],
            3,
            [127, 0, 0, 0],
        ),
    };
    if inside {
        push_quad(
            vertices,
            [p[3], p[2], p[1], p[0]],
            shade,
            normal.map(|c| -c),
            uv,
        );
    } else {
        push_quad(vertices, p, shade, normal, uv);
    }
}

#[inline(always)]
fn emit_faces(
    vertices: &mut Vec<crate::mesher::Vertex>,
    mask: u16,
    x0: f32,
    z0: f32,
    uv: [f32; 2],
) {
    for d in [1, 0, 2, 3, 4, 5] {
        if mask & (1 << d) != 0 {
            emit_face(vertices, d, false, x0, z0, uv);
        }
    }
    if mask >> 6 != 0 {
        for d in 0..6 {
            if mask & (1 << (d + 6)) != 0 {
                emit_face(vertices, d, true, x0, z0, uv);
            }
        }
    }
}

#[inline(always)]
fn emit_flat_cell(vertices: &mut Vec<crate::mesher::Vertex>, x0: f32, z0: f32, uv: [f32; 2]) {
    let x1 = x0 + 12.0;
    let z1 = z0 + 12.0;
    let p = [[x1, 0.0, z0], [x1, 0.0, z1], [x0, 0.0, z1], [x0, 0.0, z0]];
    push_quad(vertices, p, 0, [0; 4], uv);
    push_quad(vertices, [p[3], p[2], p[1], p[0]], 0, [0; 4], uv);
}

/// Appends every cell's faces for `key`; the output depends on nothing else.
fn build(
    texture: &Texture,
    key: &MeshKey,
    cells: &[(i32, i32)],
    bottom: f32,
    uv: [f32; 2],
    out: &mut Vec<crate::mesher::Vertex>,
) {
    out.clear();
    let (w, h, tex_cells) = texture;
    let (w_i64, h_i64) = (i64::from(*w), i64::from(*h));
    let filled = |cx: i64, cz: i64| {
        tex_cells[(cz.rem_euclid(h_i64) * w_i64 + cx.rem_euclid(w_i64)) as usize]
    };

    for &(dx, dz) in cells {
        let (dx_i64, dz_i64) = (i64::from(dx), i64::from(dz));
        let (cx, cz) = (key.cell_x + dx_i64, key.cell_z + dz_i64);
        if !filled(cx, cz) {
            continue;
        }
        let x0 = dx as f32 * 12.0;
        let z0 = dz as f32 * 12.0;

        if key.fast {
            emit_flat_cell(out, x0, z0, uv);
            continue;
        }

        let mask = face_mask(
            dx - key.sub_x,
            dz - key.sub_z,
            bottom,
            [
                !filled(cx, cz - 1),
                !filled(cx, cz + 1),
                !filled(cx - 1, cz),
                !filled(cx + 1, cz),
            ],
        );
        emit_faces(out, mask, x0, z0, uv);
    }
}

impl Mesh {
    /// Returns true only when vertices must be uploaded. Sub-cell motion and
    /// color changes update only the uniform, as in CloudRenderer.prepare.
    pub fn prepare(&mut self, input: &Input<'_>) -> (bool, Uniform) {
        let (key, uniform) = input.state();
        let motion = self.track(input, &key, &uniform);
        self.collect();
        if self.key.as_ref() != Some(&key) {
            if self.cached_radius != Some(key.radius) {
                self.cells = Arc::new(self::cells(key.radius).collect());
                self.cached_radius = Some(key.radius);
            }
            match self.ready.take() {
                Some(mut built)
                    if built.key == key
                        && built.uv == input.uv
                        && self
                            .texture
                            .as_ref()
                            .is_some_and(|t| Arc::ptr_eq(t, &built.texture)) =>
                {
                    std::mem::swap(&mut self.vertices, &mut built.vertices);
                    self.spare = built.vertices;
                    #[cfg(test)]
                    {
                        self.hits += 1;
                    }
                }
                stale => {
                    if let Some(stale) = stale {
                        self.spare = stale.vertices;
                    }
                    build(
                        input.texture,
                        &key,
                        &self.cells,
                        uniform.offset[1],
                        input.uv,
                        &mut self.vertices,
                    );
                }
            }
            self.key = Some(key);
            self.request_next(input, &key, &uniform, motion);
            return (true, uniform);
        }
        self.request_next(input, &key, &uniform, motion);
        (false, uniform)
    }

    /// The camera's wrapped cloud-space motion since the previous frame.
    fn track(&mut self, input: &Input<'_>, key: &MeshKey, uniform: &Uniform) -> (f64, f64) {
        let (w, h, _) = input.texture;
        let pos = (
            key.cell_x as f64 * 12.0 - f64::from(uniform.offset[0]),
            key.cell_z as f64 * 12.0 - f64::from(uniform.offset[2]),
        );
        let wrapped = |d: f64, period: f64| d - (d / period).round() * period;
        let motion = self.last_pos.map_or((0.0, 0.0), |last| {
            (
                wrapped(pos.0 - last.0, f64::from(*w) * 12.0),
                wrapped(pos.1 - last.1, f64::from(*h) * 12.0),
            )
        });
        self.last_pos = Some(pos);
        motion
    }

    fn collect(&mut self) {
        let Some(prefetch) = &mut self.prefetch else {
            return;
        };
        match prefetch.results.try_recv() {
            Ok(built) => {
                prefetch.in_flight = false;
                if let Some(old) = self.ready.replace(built) {
                    self.spare = old.vertices;
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.prefetch = None;
                self.prefetch_off = true;
            }
        }
    }

    fn request_next(
        &mut self,
        input: &Input<'_>,
        key: &MeshKey,
        uniform: &Uniform,
        motion: (f64, f64),
    ) {
        if self.prefetch_off || self.prefetch.as_ref().is_some_and(|p| p.in_flight) {
            return;
        }
        let (w, h, _) = input.texture;
        let next = predicted_key(
            key,
            (-uniform.offset[0], -uniform.offset[2]),
            motion,
            (*w, *h),
        );
        if next == *key || self.ready.as_ref().is_some_and(|b| b.key == next) {
            return;
        }
        if self.prefetch.is_none() {
            self.prefetch = Prefetch::spawn();
            self.prefetch_off = self.prefetch.is_none();
        }
        let Some(prefetch) = &mut self.prefetch else {
            return;
        };
        let texture = match &self.texture {
            Some(t) if **t == *input.texture => t.clone(),
            _ => {
                let t = Arc::new(input.texture.clone());
                self.texture = Some(t.clone());
                t
            }
        };
        let request = Request {
            key: next,
            bottom: uniform.offset[1],
            uv: input.uv,
            texture,
            cells: self.cells.clone(),
            out: std::mem::take(&mut self.spare),
        };
        if prefetch.requests.send(request).is_ok() {
            prefetch.in_flight = true;
        } else {
            self.prefetch = None;
            self.prefetch_off = true;
        }
    }

    #[cfg(test)]
    fn without_prefetch() -> Self {
        Self {
            prefetch_off: true,
            ..Self::default()
        }
    }

    #[cfg(test)]
    fn settle(&mut self) {
        if let Some(prefetch) = &mut self.prefetch
            && prefetch.in_flight
            && let Ok(built) = prefetch.results.recv()
        {
            prefetch.in_flight = false;
            self.ready = Some(built);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_mesh_tracks_geometry_but_not_subcell_motion_or_color() {
        let texture = (8, 8, (0..64).map(|i| i % 3 != 0).collect());
        let mut input = Input {
            texture: &texture,
            uv: [0.5; 2],
            camera: glam::DVec3::new(1.0, 100.0, 1.0),
            time: 0,
            partial_tick: 0.0,
            range: 2,
            fast: false,
            color: [1.0; 4],
        };
        let mut mesh = Mesh::default();
        assert!(mesh.prepare(&input).0);
        let vertices = bytemuck::cast_slice::<_, u8>(&mesh.vertices).to_vec();
        let pointer = mesh.vertices.as_ptr();
        for tick in 0..100 {
            input.time = tick;
            input.partial_tick = 0.5;
            input.camera.x = 2.0;
            input.camera.y = 101.0;
            input.color = [0.8, 0.7, 0.6, 0.5];
            let (rebuilt, uniform) = mesh.prepare(&input);
            assert!(!rebuilt);
            assert_eq!(mesh.vertices.as_ptr(), pointer);
            assert_eq!(bytemuck::cast_slice::<_, u8>(&mesh.vertices), vertices);
            assert_eq!(uniform.offset[1], (192.33 - input.camera.y) as f32);
            assert_eq!(
                uniform.colors[1],
                [0.8 * 0.7, 0.7 * 0.7, 0.6 * 0.7, 0.5].map(|v| (v * 255.0) as u8 as f32 / 255.0)
            );
            for color in uniform.colors {
                assert_eq!(color[3], 127.0 / 255.0);
            }
        }
        // One block into the second snap cell: every case below moves the snapped origin, or
        // another part of the key, so each one rebuilds.
        let far_cell = SNAP_CELLS as f64 * 12.0 + 1.0;
        for (x, y, z, range, fast) in [
            (far_cell, 101.0, 1.0, 2, false),
            (far_cell, 101.0, -5.0, 2, false),
            (far_cell, 192.33, -5.0, 2, false),
            (far_cell, 196.34, -5.0, 2, false),
            (far_cell, 196.34, -5.0, 3, false),
            (far_cell, 196.34, -5.0, 3, true),
        ] {
            input.camera = glam::DVec3::new(x, y, z);
            input.range = range;
            input.fast = fast;
            assert!(mesh.prepare(&input).0);
            let mut fresh = Mesh::default();
            fresh.prepare(&input);
            assert_eq!(
                bytemuck::cast_slice::<_, u8>(&mesh.vertices),
                bytemuck::cast_slice::<_, u8>(&fresh.vertices)
            );
            assert!(!mesh.prepare(&input).0);
        }
        // Drift alone crosses the snapped origin, even with a stationary camera.
        input.camera = glam::DVec3::new(1.0, 101.0, 1.0);
        input.time = 0;
        input.partial_tick = 0.0;
        assert!(mesh.prepare(&input).0);
        assert!(!mesh.prepare(&input).0);
        // 400 ticks of drift move the clouds 12 blocks: one whole snap cell.
        input.time = SNAP_CELLS * 400;
        assert!(mesh.prepare(&input).0);
        // Resource reload constructs a new renderer and cache, including empty packs.
        let mut reloaded = Mesh::default();
        let empty = (1, 1, vec![false]);
        input.texture = &empty;
        assert!(reloaded.prepare(&input).0);
        assert!(reloaded.vertices.is_empty());
        assert!(!reloaded.prepare(&input).0);
    }

    fn frame_input(texture: &Texture, camera: [f64; 3], time: i64) -> Input<'_> {
        Input {
            texture,
            uv: [0.5; 2],
            camera: glam::DVec3::from_array(camera),
            time,
            partial_tick: 0.25,
            range: 3,
            fast: false,
            color: [1.0; 4],
        }
    }

    /// Runs the frames through a synchronous mesh and a prefetching one, which must agree on every frame.
    fn assert_same_as_synchronous(
        texture: &Texture,
        frames: &[([f64; 3], i64, bool)],
    ) -> (u32, u32) {
        let mut sync = Mesh::without_prefetch();
        let mut prefetched = Mesh::default();
        let mut rebuilds = 0;
        for (n, &(camera, time, fast)) in frames.iter().enumerate() {
            let mut input = frame_input(texture, camera, time);
            input.fast = fast;
            let expected = sync.prepare(&input);
            let actual = prefetched.prepare(&input);
            assert_eq!(expected.0, actual.0, "frame {n}");
            assert_eq!(
                bytemuck::bytes_of(&expected.1),
                bytemuck::bytes_of(&actual.1),
                "frame {n}"
            );
            assert!(
                bytemuck::cast_slice::<_, u8>(&sync.vertices)
                    == bytemuck::cast_slice::<_, u8>(&prefetched.vertices),
                "frame {n}"
            );
            rebuilds += u32::from(expected.0);
            prefetched.settle();
        }
        (rebuilds, prefetched.hits)
    }

    fn texture() -> Texture {
        (16, 16, (0..256).map(|i| (i * 7 + i / 5) % 3 != 0).collect())
    }

    fn flight(
        from: [f64; 3],
        step: [f64; 3],
        frames: usize,
        time: i64,
    ) -> Vec<([f64; 3], i64, bool)> {
        (0..frames)
            .map(|n| {
                let n = n as f64;
                (
                    [
                        from[0] + step[0] * n,
                        from[1] + step[1] * n,
                        from[2] + step[2] * n,
                    ],
                    time + n as i64,
                    false,
                )
            })
            .collect()
    }

    #[test]
    fn prefetched_meshes_equal_synchronous_ones_in_straight_flights() {
        let texture = texture();
        for step in [
            [0.7, 0.0, 0.0],
            [0.0, 0.0, -0.9],
            [0.6, 0.0, 0.5],
            [-0.8, 0.0, 0.3],
        ] {
            let frames = flight([20.0, 100.0, 30.0], step, 200, 0);
            let (rebuilds, hits) = assert_same_as_synchronous(&texture, &frames);
            assert!(rebuilds >= 6);
            assert!(hits + 1 >= rebuilds, "{step:?}: {hits} hits of {rebuilds}");
        }
    }

    #[test]
    fn prefetched_meshes_equal_synchronous_ones_when_the_camera_turns() {
        let texture = texture();
        let mut frames = flight([5.0, 100.0, 5.0], [0.8, 0.0, 0.0], 80, 0);
        frames.extend(flight([69.0, 100.0, 5.0], [0.0, 0.0, 0.8], 80, 80));
        frames.extend(flight([69.0, 100.0, 69.0], [-0.8, 0.0, 0.4], 80, 160));
        assert_same_as_synchronous(&texture, &frames);
    }

    #[test]
    fn prefetched_meshes_equal_synchronous_ones_while_clouds_drift_past_a_still_camera() {
        let texture = texture();
        let frames = flight([3.0, 100.0, 3.0], [0.0; 3], 1200, 0)
            .into_iter()
            .map(|(camera, time, fast)| (camera, time * 4, fast))
            .collect::<Vec<_>>();
        let (rebuilds, hits) = assert_same_as_synchronous(&texture, &frames);
        assert!(rebuilds >= 3);
        assert!(hits + 1 >= rebuilds);
    }

    #[test]
    fn prefetched_meshes_equal_synchronous_ones_across_the_texture_wrap() {
        let texture = texture();
        let frames = flight([-60.0, 100.0, 4.0], [0.9, 0.0, 0.0], 160, 0);
        let (rebuilds, hits) = assert_same_as_synchronous(&texture, &frames);
        assert!(hits + 1 >= rebuilds);
    }

    #[test]
    fn prefetched_meshes_equal_synchronous_ones_through_layer_crossings_teleports_and_mode_changes()
    {
        let texture = texture();
        let mut frames = flight([30.0, 180.0, 30.0], [0.4, 0.25, 0.1], 120, 0);
        frames.extend(flight([500.0, 100.0, -300.0], [0.5, 0.0, 0.5], 40, 120));
        frames.extend(
            flight([10.0, 100.0, 10.0], [0.5, 0.0, 0.0], 40, 160)
                .into_iter()
                .map(|(c, t, _)| (c, t, true)),
        );
        frames.extend(flight([30.0, 195.0, 10.0], [0.5, -0.3, 0.0], 60, 200));
        assert_same_as_synchronous(&texture, &frames);
    }

    #[test]
    fn a_prefetched_mesh_is_not_used_after_the_uv_changes() {
        let texture = texture();
        let mut sync = Mesh::without_prefetch();
        let mut prefetched = Mesh::default();
        let mut input = frame_input(&texture, [20.0, 100.0, 30.0], 0);
        for n in 0..40 {
            input.camera.x += 0.7;
            input.time += 1;
            if n == 20 {
                input.uv = [0.25, 0.75];
            }
            let expected = sync.prepare(&input).0;
            assert_eq!(prefetched.prepare(&input).0, expected);
            assert!(
                bytemuck::cast_slice::<_, u8>(&sync.vertices)
                    == bytemuck::cast_slice::<_, u8>(&prefetched.vertices)
            );
            prefetched.settle();
        }
    }

    #[test]
    fn a_snap_cell_of_motion_keeps_the_mesh_at_the_same_world_cells() {
        let texture = (8, 8, (0..64).map(|i| i % 3 != 0).collect());
        let mut input = Input {
            texture: &texture,
            uv: [0.5; 2],
            camera: glam::DVec3::new(1.0, 100.0, 1.0),
            time: 0,
            partial_tick: 0.0,
            range: 2,
            fast: false,
            color: [1.0; 4],
        };
        // One snap cell, in blocks.
        let cell = SNAP_CELLS as f64 * 12.0;
        let mut mesh = Mesh::default();
        let (rebuilt, uniform) = mesh.prepare(&input);
        assert!(rebuilt);
        // The shader draws a vertex at `camera + position + offset`, so the world cells the
        // cloud covers are `camera` plus this: the same ones anywhere inside a snap cell.
        assert_eq!(input.camera.x as f32 + uniform.offset[0], 0.0);
        for x in [1.5, cell * 0.15, cell * 0.5, cell * 0.75, cell - 1.5] {
            input.camera.x = x;
            let (rebuilt, uniform) = mesh.prepare(&input);
            assert!(!rebuilt, "rebuilt at {x}");
            assert_eq!(input.camera.x as f32 + uniform.offset[0], 0.0);
            // The trailing origin still covers the whole range past the camera.
            let far = mesh
                .vertices
                .iter()
                .map(|v| v.pos[0] + uniform.offset[0])
                .fold(f32::MIN, f32::max);
            assert!(far >= 32.0, "clouds end {far} blocks ahead at {x}");
        }
        // The next snap cell takes the origin with it.
        input.camera.x = cell;
        let (rebuilt, uniform) = mesh.prepare(&input);
        assert!(rebuilt);
        assert_eq!(input.camera.x as f32 + uniform.offset[0], cell as f32);
    }

    #[test]
    fn only_inside_the_layer_does_the_mesh_follow_the_camera_between_snap_cells() {
        let texture = (8, 8, (0..64).map(|i| i % 3 != 0).collect());
        let mut input = Input {
            texture: &texture,
            uv: [0.5; 2],
            camera: glam::DVec3::new(1.0, 101.0, 1.0),
            time: 0,
            partial_tick: 0.0,
            range: 2,
            fast: false,
            color: [1.0; 4],
        };
        let mut mesh = Mesh::default();
        assert!(mesh.prepare(&input).0);
        // Below the layer, cells crossed inside one snap cell change nothing visible.
        for (x, z) in [(11.9, 11.9), (24.0, 12.0), (35.5, 23.9)] {
            input.camera.x = x;
            input.camera.z = z;
            assert!(
                !mesh.prepare(&input).0,
                "rebuilt at {x},{z} below the layer"
            );
        }
        // Inside it, the inward faces around the camera do move with its own cell.
        input.camera.y = 194.0;
        assert!(mesh.prepare(&input).0);
        input.camera.x = 12.0;
        assert!(mesh.prepare(&input).0);
        input.camera.y = 101.0;
        assert!(mesh.prepare(&input).0);
    }

    #[test]
    fn settings_and_packed_alpha_gate_clouds() {
        for mode in ["false", "fast", "true"] {
            assert_eq!(visible(mode, 1.0), mode != "false");
            assert!(!visible(mode, 0.0));
            assert!(!visible(mode, 0.5 / 255.0));
        }
        assert!(visible("fast", 1.0 / 255.0));
    }

    #[test]
    fn fast_cells_are_flat_at_the_bottom_and_visible_from_both_sides() {
        for bottom in [-10.0, 0.0, 10.0] {
            let [down, up] = flat_cell(-3.0, 7.0, bottom);
            assert_eq!(
                down,
                [
                    [9.0, bottom, 7.0],
                    [9.0, bottom, 19.0],
                    [-3.0, bottom, 19.0],
                    [-3.0, bottom, 7.0],
                ]
            );
            assert_eq!(up, [down[3], down[2], down[1], down[0]]);
            for (quad, sign) in [(down, -1.0), (up, 1.0)] {
                let [a, b, c] = [quad[0], quad[1], quad[2]].map(glam::Vec3::from_array);
                assert!((b - a).cross(c - a).y * sign > 0.0);
            }
        }
    }

    #[test]
    fn cells_are_nearest_first_and_unique() {
        let cells: Vec<_> = cells(2).collect();
        assert_eq!(cells[0], (0, 0));
        assert_eq!(cells.len(), 13);
        assert!(
            cells
                .windows(2)
                .all(|w| w[0].0.abs() + w[0].1.abs() <= w[1].0.abs() + w[1].1.abs())
        );
        assert_eq!(
            cells.iter().collect::<std::collections::HashSet<_>>().len(),
            cells.len()
        );
    }
    #[test]
    fn only_camera_facing_exterior_faces() {
        assert_eq!(face_mask(2, 2, 10.0, [true; 4]), 1 | 4 | 16);
        assert_eq!(face_mask(-2, -2, -5.0, [true; 4]), 2 | 8 | 32);
        assert_eq!(face_mask(2, 2, 0.0, [false; 4]), 3);
    }
    #[test]
    fn cloud_distance_sets_the_radius() {
        assert_eq!(radius_cells(2), 3);
        assert_eq!(radius_cells(12), 16);
        assert_eq!(radius_cells(128), 171);
    }
    #[test]
    fn connected_cells_have_no_exterior_walls() {
        assert_eq!(face_mask(2, 2, 10.0, [false; 4]), 1);
    }
    #[test]
    fn nearby_cells_keep_inward_faces_for_camera_inside_clouds() {
        assert_eq!(face_mask(0, 0, 0.0, [false; 4]), 3 | (63 << 6));
    }
}
