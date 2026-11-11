use super::*;

pub(super) const QUAD: u8 = 0;
pub(super) const TEXT: u8 = 1;
pub(super) const ITEM: u8 = 2;
pub(super) const BLUR: u8 = 3;

pub(super) type Corner = ([f32; 2], [f32; 2], u32);

/// What a recorded frame asked of the window beyond its pieces.
pub(super) struct Painted {
    pub(super) blur: bool,
}

/// A UV within 1e-4 of an integer is that integer.
pub(super) fn snapped(x: f32) -> f32 {
    let r = x.round();
    if (x - r).abs() < 1e-4 { r } else { x }
}

/// `[start, integer crossings.., end]` of a UV range from `a` to `b`, in that order, and the fractions they sit at.
pub(super) fn uv_cuts(a: f32, b: f32) -> Vec<(f32, f32)> {
    let (a, b) = (snapped(a), snapped(b));
    let mut cuts = vec![(0.0, a)];
    if a != b {
        let (lo, hi) = (a.min(b), a.max(b));
        let mut k = lo.floor() + 1.0;
        let mut inside = Vec::new();
        while k < hi {
            inside.push((((k - a) / (b - a)), k));
            k += 1.0;
        }
        inside.sort_by(|x, y| x.0.total_cmp(&y.0));
        cuts.extend(inside);
    }
    cuts.push((1.0, b));
    cuts
}

/// A quad whose UVs leave [0,1] as one quad a tile, each tile's UVs inside [0,1], as REPEAT would sample it.
pub(super) fn repeat_cells(corners: [Corner; 4]) -> Vec<[Corner; 4]> {
    let outside = |x: f32| {
        let x = snapped(x);
        !(0.0..=1.0).contains(&x)
    };
    if !corners.iter().any(|c| outside(c.1[0]) || outside(c.1[1])) {
        return vec![corners];
    }
    let across = uv_cuts(corners[0].1[0], corners[3].1[0]);
    let down = uv_cuts(corners[0].1[1], corners[1].1[1]);
    let mix = |s: f32, t: f32| -> Corner {
        let w = [(1.0 - s) * (1.0 - t), (1.0 - s) * t, s * t, s * (1.0 - t)];
        let sum = |f: &dyn Fn(&Corner) -> f32| {
            corners.iter().zip(w).map(|(c, w)| f(c) * w).sum::<f32>()
        };
        let channel = |shift: u32| {
            sum(&|c| ((c.2 >> shift) & 255) as f32)
                .round()
                .clamp(0.0, 255.0) as u32
        };
        (
            [sum(&|c| c.0[0]), sum(&|c| c.0[1])],
            [0.0, 0.0],
            channel(24) << 24 | channel(16) << 16 | channel(8) << 8 | channel(0),
        )
    };
    let mut cells = Vec::new();
    for s in across.windows(2) {
        let ku = s[0].1.min(s[1].1).floor();
        for t in down.windows(2) {
            let kv = t[0].1.min(t[1].1).floor();
            let at = |(si, ti): (usize, usize)| {
                let (s, t) = (s[si], t[ti]);
                let mut c = mix(s.0, t.0);
                c.1 = [s.1 - ku, t.1 - kv];
                c
            };
            cells.push([at((0, 0)), at((0, 1)), at((1, 1)), at((1, 0))]);
        }
    }
    cells
}

/// `Identifier` of a texture file to its name in Horizon's GUI atlas.
pub(super) fn atlas_key(id: &str) -> Option<String> {
    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
    let path = path.strip_prefix("textures/")?.strip_suffix(".png")?;
    Some(if namespace == "minecraft" {
        path.to_owned()
    } else {
        format!("{namespace}:{path}")
    })
}

pub(super) fn scissor(reply: &mut In) -> Option<Option<[f32; 4]>> {
    Some(if reply.bool()? {
        Some([
            reply.int()? as f32,
            reply.int()? as f32,
            reply.int()? as f32,
            reply.int()? as f32,
        ])
    } else {
        None
    })
}

pub(super) fn quad(batch: &mut HudBatch, atlas: &HudAtlas, reply: &mut In) -> Option<()> {
    let texture = reply.text()?;
    let kind = reply.byte()?;
    let wrap = reply.byte()? == 1;
    let mut corners = [([0f32; 2], [0f32; 2], 0u32); 4];
    for corner in &mut corners {
        *corner = (
            [reply.float()?, reply.float()?],
            [reply.float()?, reply.float()?],
            horizon_client::hud_layout::abgr(reply.int()? as u32),
        );
    }
    let clip = scissor(reply)?;
    let rect = if texture.is_empty() {
        atlas.sprites.get("horizon/white").copied()
    } else {
        atlas_key(texture).and_then(|key| atlas.sprites.get(&key).copied())
    };
    let Some(r) = rect else {
        thread_local!(static MISSING: RefCell<std::collections::HashSet<String>> = RefCell::default());
        if MISSING.with_borrow_mut(|m| m.insert(texture.to_owned())) {
            log::warn!("Horizon: a Java screen's texture {texture} isn't in the GUI atlas");
        }
        return Some(());
    };
    let start = batch.vertices.len();
    let cells = if wrap && !texture.is_empty() {
        repeat_cells(corners)
    } else {
        vec![corners]
    };
    for cell in cells {
        for i in [0, 1, 2, 0, 2, 3] {
            let (pos, uv, color) = cell[i];
            let uv = if texture.is_empty() {
                [(r.u0 + r.u1) / 2.0, (r.v0 + r.v1) / 2.0]
            } else {
                [r.u0 + uv[0] * (r.u1 - r.u0), r.v0 + uv[1] * (r.v1 - r.v0)]
            };
            batch.vertices.push(HudVertex {
                pos,
                uv,
                color,
                depth: 0.0,
                glint_uv: [0.0, 0.0],
                text: 0,
            });
        }
    }
    match kind {
        1 => batch.invert_since(start),
        2 => batch.blend_since(start, horizon_client::hud::Blend::Additive),
        _ => {}
    }
    if let Some([x0, y0, x1, y1]) = clip {
        batch.clip_since(start, x0, y0, x1, y1);
    }
    Some(())
}

pub(super) fn text(batch: &mut HudBatch, atlas: &HudAtlas, reply: &mut In) -> Option<()> {
    let (x, y, sx, sy) = (
        reply.float()?,
        reply.float()?,
        reply.float()?,
        reply.float()?,
    );
    let color = reply.int()? as u32;
    let shadow = reply.bool()?;
    let _background = reply.int()?;
    let mut text = Text::literal("");
    for _ in 0..reply.int()? {
        let run = reply.text()?;
        let rgb = reply.int()?;
        let flags = reply.byte()?;
        let font = reply.text()?;
        let on = |bit: u8| (flags & bit != 0).then_some(true);
        let style = Style {
            color: (rgb >= 0).then_some(rgb as u32),
            bold: on(1),
            italic: on(2),
            underlined: on(4),
            strikethrough: on(8),
            obfuscated: on(16),
            font: (!font.is_empty() && font != "minecraft:default").then(|| font.to_owned()),
            ..Style::default()
        };
        text = text.append(Text::styled(run, style));
    }
    let clip = scissor(reply)?;
    if color >> 24 == 0 {
        return Some(());
    }
    let start = batch.vertices.len();
    batch.text_styled(
        atlas,
        &text,
        x,
        y,
        horizon_client::hud_layout::abgr(color),
        shadow,
    );
    if sx != 1.0 || sy != 1.0 {
        batch.scale_since(start, x, y, sx, sy);
    }
    if let Some([x0, y0, x1, y1]) = clip {
        batch.clip_since(start, x0, y0, x1, y1);
    }
    Some(())
}

/// A recorded frame's pieces in call order; `item` paints an item entry. None when it ended in pieces.
pub(super) fn paint_frame(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    frame: &[u8],
    item: impl Fn(&mut HudBatch, &mut In) -> Option<()>,
) -> Option<Painted> {
    let mut reply = In(frame);
    let mut painted = Painted { blur: false };
    for _ in 0..reply.int().unwrap_or(0) {
        let ok = match reply.byte() {
            Some(QUAD) => quad(batch, atlas, &mut reply),
            Some(BLUR) => {
                painted.blur = true;
                Some(())
            }
            Some(TEXT) => text(batch, atlas, &mut reply),
            Some(ITEM) => item(batch, &mut reply),
            _ => None,
        };
        if ok.is_none() {
            log::warn!("Horizon: a Java frame ended in pieces");
            return None;
        }
    }
    Some(painted)
}


impl Game<'_> {
/// What a mod's item decorators recorded over this stack, in the slot at `(x, y)`.
pub(in crate::game) fn draw_java_item_decorations(
    &self,
    batch: &mut HudBatch,
    stack: &ItemStack,
    x: f32,
    y: f32,
) {
    let Some(frame) = super::super::java_models::java_decorations(stack) else {
        return;
    };
    let start = batch.vertices.len();
    let mut reply = In(&frame);
    for _ in 0..reply.int().unwrap_or(0) {
        let ok = match reply.byte() {
            Some(QUAD) => quad(batch, &self.gui.atlas, &mut reply),
            Some(TEXT) => text(batch, &self.gui.atlas, &mut reply),
            Some(ITEM) => self.java_screen_item(batch, &mut reply),
            _ => None,
        };
        if ok.is_none() {
            log::warn!("Horizon: an item decorator's frame ended in pieces");
            break;
        }
    }
    for vertex in &mut batch.vertices[start..] {
        vertex.pos[0] += x;
        vertex.pos[1] += y;
    }
}

/// The open Java screen asked for the menu blur this frame.
pub(in crate::game) fn java_screen_blurred(&self) -> bool {
    CLIENT.with_borrow(|c| c.blurred)
}

/// The open Java screen, drawn as it extracted itself this frame.
pub(in crate::game) fn draw_java_screen(&mut self, batch: &mut HudBatch) {
    CLIENT.with_borrow_mut(|c| c.blurred = false);
    let Some(host) = horizon_java_host::get() else {
        return;
    };
    self.java_publish_client_world();
    let mut out = Out::default();
    self.java_client_size(&mut out);
    out.double(f64::from(self.cursor.0));
    out.double(f64::from(self.cursor.1));
    out.float(0.0);
    let Some(reply) = host.request("client_frame", &out.0) else {
        return;
    };
    let painted = paint_frame(batch, &self.gui.atlas, &reply, |batch, reply| {
        self.java_screen_item(batch, reply)
    });
    CLIENT.with_borrow_mut(|c| c.blurred = painted.is_some_and(|p| p.blur));
}

pub(super) fn java_screen_item(&self, batch: &mut HudBatch, reply: &mut In) -> Option<()> {
    let (x, y, sx, sy) = (
        reply.float()?,
        reply.float()?,
        reply.float()?,
        reply.float()?,
    );
    let stack = java_mods::read_tag(reply.bytes()?)
        .and_then(|tag| horizon_client::save::item_from_nbt(&tag));
    let clip = scissor(reply)?;
    let Some(stack) = stack else {
        return Some(());
    };
    let start = batch.vertices.len();
    self.draw_item_model(batch, &stack, x, y);
    if sx != 1.0 || sy != 1.0 {
        batch.scale_since(start, x, y, sx, sy);
    }
    if let Some([x0, y0, x1, y1]) = clip {
        batch.clip_since(start, x0, y0, x1, y1);
    }
    Some(())
}
}
