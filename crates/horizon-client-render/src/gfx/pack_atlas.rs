//! The block atlas of shader packs (spec 5.1, `atlasSize` in 5.2): packs sample one 2D atlas
//! with atlas coordinates, Horizon keeps block sprites in texture arrays. With a pack, every
//! layer is copied into a grid at its array's size, array after array from the top left, and
//! an animated layer again whenever its frame changes.

pub(super) struct PackAtlas {
    texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    /// Each sprite array's block of the grid: its sprites keep their own size.
    blocks: Vec<Block>,
    /// Layers per sprite array.
    split: u32,
}

/// One sprite array's part of the atlas: `columns` sprites of `sprite` pixels a row, from `top`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Block {
    sprite: u32,
    columns: u32,
    top: u32,
}

impl PackAtlas {
    /// Copies the layers of `arrays` (`split` layer ids per array, each array its own sprite
    /// size), array after array, each a grid as wide as the atlas; `None` when the atlas is
    /// larger than the device allows.
    pub fn new(
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        arrays: &[wgpu::Texture],
        split: u32,
    ) -> Option<Self> {
        let width = arrays
            .iter()
            .map(|a| (f64::from(a.depth_or_array_layers()).sqrt().ceil() as u32).max(1) * a.width())
            .max()?;
        let mut top = 0;
        let blocks: Vec<Block> = arrays
            .iter()
            .map(|a| {
                let sprite = a.width();
                let columns = (width / sprite).max(1);
                let block = Block {
                    sprite,
                    columns,
                    top,
                };
                top += a.depth_or_array_layers().div_ceil(columns) * sprite;
                block
            })
            .collect();
        let max = device.limits().max_texture_dimension_2d;
        if width > max || top > max {
            return None;
        }
        // Levels past the smallest sprite's single pixel would mix neighbours: stop there.
        let levels = arrays
            .iter()
            .map(|a| a.mip_level_count().min(a.width().ilog2() + 1))
            .min()?;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Shader pack block atlas"),
            size: wgpu::Extent3d {
                width,
                height: top,
                depth_or_array_layers: 1,
            },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let atlas = Self {
            texture,
            view,
            blocks,
            split,
        };
        let all = arrays
            .iter()
            .enumerate()
            .flat_map(|(a, t)| (0..t.depth_or_array_layers()).map(move |l| a as u32 * split + l));
        atlas.copy(encoder, arrays, all);
        Some(atlas)
    }

    /// The atlas's size in pixels (`atlasSize`).
    pub fn size(&self) -> [u32; 2] {
        [self.texture.width(), self.texture.height()]
    }

    /// Copies `layers` from the sprite arrays again, every level.
    pub fn copy(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        arrays: &[wgpu::Texture],
        layers: impl IntoIterator<Item = u32>,
    ) {
        for layer in layers {
            let a = (layer / self.split) as usize;
            let (Some(array), Some(block)) = (arrays.get(a), self.blocks.get(a)) else {
                continue;
            };
            let slot = layer % self.split;
            if slot >= array.depth_or_array_layers() {
                continue;
            }
            let (column, row) = (slot % block.columns, slot / block.columns);
            for level in 0..self.texture.mip_level_count() {
                let edge = block.sprite >> level;
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: array,
                        mip_level: level,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: slot,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.texture,
                        mip_level: level,
                        origin: wgpu::Origin3d {
                            x: column * edge,
                            y: (block.top >> level) + row * edge,
                            z: 0,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: edge,
                        height: edge,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
    }

    /// GLSL choosing, for `hz_a` (a layer's array), the value `pick` gives each block.
    fn per_block(&self, pick: impl Fn(&Block) -> String) -> String {
        let mut out = pick(self.blocks.last().expect("an atlas has a block"));
        for (a, block) in self.blocks.iter().enumerate().rev().skip(1) {
            out = format!("(hz_a == {a}u ? {} : {out})", pick(block));
        }
        out
    }
}

/// GLSL: the face normal and tangent of a flat-drawn vertex, from its quad's first three corners
/// (quads are four consecutive vertices of the page; position words 0-2, UV word 3).
const FACE: &str = "    uint hz_q = (uint(gl_VertexIndex) & ~3u) * 6u;\n    \
    vec3 hz_p0 = uintBitsToFloat(uvec3(hz_vertices[hz_q], hz_vertices[hz_q + 1u], hz_vertices[hz_q + 2u]));\n    \
    vec3 hz_p1 = uintBitsToFloat(uvec3(hz_vertices[hz_q + 6u], hz_vertices[hz_q + 7u], hz_vertices[hz_q + 8u]));\n    \
    vec3 hz_p2 = uintBitsToFloat(uvec3(hz_vertices[hz_q + 12u], hz_vertices[hz_q + 13u], hz_vertices[hz_q + 14u]));\n    \
    uvec3 hz_w = uvec3(hz_vertices[hz_q + 3u], hz_vertices[hz_q + 9u], hz_vertices[hz_q + 15u]);\n    \
    vec2 hz_t0 = vec2(hz_w.x & 65535u, hz_w.x >> 16u);\n    \
    vec2 hz_d1 = vec2(hz_w.y & 65535u, hz_w.y >> 16u) - hz_t0;\n    \
    vec2 hz_d2 = vec2(hz_w.z & 65535u, hz_w.z >> 16u) - hz_t0;\n    \
    vec3 hz_e1 = hz_p1 - hz_p0;\n    \
    vec3 hz_e2 = hz_p2 - hz_p0;\n    \
    vec3 hz_n = cross(hz_e1, hz_e2);\n    \
    if (dot(hz_n, hz_n) > 1e-12) {\n        \
        hz_Normal = normalize(hz_n);\n        \
        float hz_r = hz_d1.x * hz_d2.y - hz_d2.x * hz_d1.y;\n        \
        vec3 hz_t = (hz_e1 * hz_d2.y - hz_e2 * hz_d1.y) * sign(hz_r);\n        \
        vec3 hz_b = (hz_e2 * hz_d1.x - hz_e1 * hz_d2.x) * sign(hz_r);\n        \
        if (dot(hz_t, hz_t) > 1e-12) {\n            \
            hz_t = normalize(hz_t - hz_Normal * dot(hz_Normal, hz_t));\n            \
            hz_tangent = vec4(hz_t, dot(cross(hz_t, hz_Normal), hz_b) < 0.0 ? -1.0 : 1.0);\n        \
        }\n    \
    }\n";

impl PackAtlas {
    /// The vertex inputs of a gbuffers program drawing terrain (`TerrainVertex`, terrain.wgsl's
    /// `vs_terrain` or, for `flat` draws, `vs_terrain_flat`), in atlas coordinates.
    pub fn terrain_inputs(&self, flat: bool) -> horizon_shaders::GbuffersInputs {
        let mut declarations = String::from(
            "layout(location = 0) in vec3 hz_v_pos;\nlayout(location = 1) in uvec2 hz_v_uv;\n\
             layout(location = 2) in uint hz_v_layer;\nlayout(location = 3) in vec4 hz_v_color;\n\
             layout(location = 4) in uvec2 hz_v_light;\n",
        );
        let origin = if flat {
            declarations.push_str(
                "layout(set = 1, binding = 0) readonly buffer HzBlockSlots { uint hz_block_slots[]; };\n\
                 layout(set = 1, binding = 1) readonly buffer HzSlotOrigins { vec4 hz_slot_origins[]; };\n",
            );
            // `mc_Entity` of each sprite layer: id, then 1 for fluids or -1 (the game's table),
            // and the arena page's vertices (`TerrainVertex`, six words each).
            declarations.push_str(
                "layout(set = 3, binding = 0) readonly buffer HzBlockIds { int hz_block_ids[]; };\n\
                 layout(set = 3, binding = 1) readonly buffer HzVertices { uint hz_vertices[]; };\n",
            );
            "hz_slot_origins[hz_block_slots[uint(gl_VertexIndex) >> 8u]]"
        } else {
            declarations.push_str("layout(location = 5) in vec4 hz_v_origin;\n");
            "hz_v_origin"
        };
        let [width, height] = self.size();
        let split = self.split;
        let columns = self.per_block(|b| format!("{}u", b.columns));
        let top = self.per_block(|b| format!("{:?}", f64::from(b.top) / f64::from(b.sprite)));
        let scale = self.per_block(|b| {
            format!(
                "vec2({:?}, {:?})",
                b.sprite as f32 / width as f32,
                b.sprite as f32 / height as f32
            )
        });
        // UVs past a sprite (models up to 2, distant tiles) cannot repeat in an atlas: clamped.
        // The colour's alpha holds HDR's emission level (`mesher::EMISSION_BITS`): packs get 1.
        let body = format!(
            "    vec4 hz_origin = {origin};\n    \
             uint hz_layer = hz_v_layer & 32767u;\n    \
             uint hz_a = hz_layer / {split}u;\n    \
             uint hz_slot = hz_layer % {split}u;\n    \
             uint hz_columns = {columns};\n    \
             hz_cell = vec2(float(hz_slot % hz_columns), float(hz_slot / hz_columns) + {top});\n    \
             hz_scale = {scale};\n    \
             hz_Vertex = vec4(hz_v_pos + hz_origin.xyz, 1.0);\n    \
             hz_MultiTexCoord0 = vec4((hz_cell + clamp(vec2(hz_v_uv) * (hz_origin.w > 0.0 ? uintBitsToFloat(floatBitsToUint(hz_origin.w) & 0xff800000u) : fract(abs(hz_origin.w))), 0.0, 1.0)) * hz_scale, 0.0, 1.0);\n    \
             hz_MultiTexCoord1 = vec4(float(hz_v_light.y), float(hz_v_light.x), 0.0, 1.0);\n    \
             hz_MultiTexCoord2 = hz_MultiTexCoord1;\n    \
             hz_Color = vec4(hz_v_color.rgb, 1.0);\n    \
             hz_Normal = vec3(0.0, 1.0, 0.0);\n    \
             hz_tangent = vec4(1.0, 0.0, 0.0, 1.0);\n",
        );
        let mut body = body;
        if flat {
            body.push_str(FACE);
        }
        declarations.push_str("vec2 hz_cell; vec2 hz_scale; vec4 hz_tangent;\n");
        let entity = if flat {
            "vec4(float(hz_block_ids[2u * hz_layer]), float(hz_block_ids[2u * hz_layer + 1u]), 0.0, 0.0)"
        } else {
            "vec4(-1.0, -1.0, 0.0, 0.0)"
        };
        let attributes = [
            ("mc_Entity", entity),
            (
                "mc_midTexCoord",
                "vec4((hz_cell + 0.5) * hz_scale, 0.0, 1.0)",
            ),
            ("at_tangent", "hz_tangent"),
            ("vaPosition", "hz_Vertex.xyz"),
            ("vaUV0", "hz_MultiTexCoord0.xy"),
            ("vaUV2", "hz_MultiTexCoord1.xy"),
            ("vaColor", "hz_Color"),
            ("vaNormal", "hz_Normal"),
        ];
        horizon_shaders::GbuffersInputs {
            declarations,
            body,
            attributes: attributes
                .iter()
                .map(|&(n, v)| (n.to_owned(), v.to_owned()))
                .collect(),
            after: String::new(),
            alpha_test: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two arrays of 2x2 sprites, one layer split; each layer a colour of its own.
    #[test]
    fn layers_land_in_their_cells() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
            return;
        };
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let (split, layers, sprite) = (3u32, 5u32, 2u32);
        // Layer ids: the first array's 0..3, the second's 3..5.
        let arrays: Vec<wgpu::Texture> = (0..2)
            .map(|a| {
                let count = (layers - a * split).min(split);
                let pixels: Vec<u8> = (0..count)
                    .flat_map(|l| [(a * split + l) as u8 * 40, 0, 0, 255].repeat(4))
                    .collect();
                wgpu::util::DeviceExt::create_texture_with_data(
                    &device,
                    &queue,
                    &wgpu::TextureDescriptor {
                        label: None,
                        size: wgpu::Extent3d {
                            width: sprite,
                            height: sprite,
                            depth_or_array_layers: count,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                        view_formats: &[],
                    },
                    wgpu::util::TextureDataOrder::LayerMajor,
                    &pixels,
                )
            })
            .collect();
        let mut encoder = device.create_command_encoder(&Default::default());
        let atlas = PackAtlas::new(&device, &mut encoder, &arrays, split).unwrap();
        assert_eq!(atlas.size(), [4, 6]);
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 256 * 6,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            atlas.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &read,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: 4,
                height: 6,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let data = read.slice(..).get_mapped_range().unwrap();
        let red = |x: usize, y: usize| data[y * 256 + x * 4];
        // Two columns: the first array's three layers take rows 0 and 1, the second's two row 2.
        assert_eq!([red(0, 0), red(2, 0), red(1, 3), red(3, 3)], [0, 40, 80, 0]);
        assert_eq!([red(0, 4), red(3, 5)], [120, 160]);
    }
}
