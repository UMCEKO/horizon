//! The block sprite arrays' mip chains, made on the resource worker, and their upload to the GPU.
use super::array_layers;
use crate::sprites::SpriteLayout;
use wgpu::util::DeviceExt;

/// CPU texture uploads prepared by the resource worker. Consumed by renderer
/// construction so the temporary mip chains are released after GPU upload.
pub struct PreparedTextures {
    pub(super) sprites: Vec<(Vec<u8>, u32)>,
    /// The arrays `sprites` holds, one chain each.
    pub(super) layout: SpriteLayout,
    pub(super) forcefield: (Vec<u8>, u32),
    /// The LabPBR maps' mip chains (normals, specular), array after array as `layout` has them.
    pub(super) pbr: Option<(Vec<u8>, Vec<u8>)>,
}
/// The bytes of a mip chain of one square layer of `size`, down to a texel.
fn chain_bytes(size: u32) -> usize {
    (0..=size.trailing_zeros())
        .map(|l| ((size * size * 4) >> (2 * l)) as usize)
        .sum()
}
/// Every level of `layers` square RGBA layers of `size`, each the mean of four texels of the
/// one above, layer major: the LabPBR maps' mips (their channels are data, not colour).
fn box_mip_chain(pixels: &[u8], size: u32, layers: u32) -> Vec<u8> {
    let layer_bytes = (size * size * 4) as usize;
    let mut out = Vec::new();
    for layer in pixels.chunks_exact(layer_bytes).take(layers as usize) {
        let mut level = layer.to_vec();
        let mut side = size as usize;
        out.extend_from_slice(&level);
        while side > 1 {
            let half = side / 2;
            let mut next = Vec::with_capacity(half * half * 4);
            for y in 0..half {
                for x in 0..half {
                    for c in 0..4 {
                        let at = |dx: usize, dy: usize| {
                            u32::from(level[((2 * y + dy) * side + 2 * x + dx) * 4 + c])
                        };
                        next.push(((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8);
                    }
                }
            }
            out.extend_from_slice(&next);
            level = next;
            side = half;
        }
    }
    out
}
impl PreparedTextures {
    /// `pixels` holds `layout`'s arrays' layers, array after array, each at its array's size.
    pub fn new(
        pixels: &[u8],
        layout: &SpriteLayout,
        settings: &[crate::mipmap::Settings],
        forcefield: (&[u8], u32),
    ) -> Self {
        let mut start = 0;
        let sprites = layout
            .arrays
            .iter()
            .enumerate()
            .map(|(a, array)| {
                let (size, count) = (array.size, array.layers.max(1));
                let bytes = (count * size * size * 4) as usize;
                let own = &pixels[start.min(pixels.len())..];
                start += (array.layers * size * size * 4) as usize;
                let blank;
                let own = if own.len() < bytes {
                    blank = vec![0; bytes];
                    &blank[..]
                } else {
                    own
                };
                mip_chain(own, size, settings, a as u32 * layout.split, count)
            })
            .collect();
        Self {
            sprites,
            layout: layout.clone(),
            forcefield: mip_chain(forcefield.0, forcefield.1, &[], 0, 1),
            pbr: None,
        }
    }

    /// With the resource packs' LabPBR `normals` and `specular` maps, as `RenderAssets` holds them.
    pub fn with_pbr(mut self, normals: &[u8], specular: &[u8]) -> Self {
        if !normals.is_empty() {
            let chains = |maps: &[u8]| {
                let mut start = 0;
                let mut out = Vec::new();
                for array in &self.layout.arrays {
                    let size = array.pbr_size;
                    let bytes = (array.layers * size * size * 4) as usize;
                    out.extend(box_mip_chain(
                        &maps[start..start + bytes],
                        size,
                        array.layers,
                    ));
                    start += bytes;
                }
                out
            };
            self.pbr = Some((chains(normals), chains(specular)));
        }
        self
    }

    /// The chains, made before the adapter was known, cut by its `split` when the layout is uniform.
    pub(super) fn resplit(mut self, split: u32) -> Self {
        let Some(layout) = self.layout.resplit(split) else {
            return self;
        };
        if self.layout.split == split || self.layout.count() <= split.min(self.layout.split) {
            self.layout = layout;
            return self;
        }
        let levels = self.sprites[0].1;
        let all: Vec<u8> = self
            .sprites
            .drain(..)
            .flat_map(|(bytes, _)| bytes)
            .collect();
        // Every layer's mip levels are contiguous (layer major).
        let layer_bytes = chain_bytes(layout.arrays[0].size);
        self.sprites = all
            .chunks(layer_bytes * split as usize)
            .map(|chunk| (chunk.to_vec(), levels))
            .collect();
        self.layout = layout;
        self
    }
}

/// The LabPBR maps' arrays (normals, specular), one pair per array of `layout`, from their chains.
pub(super) fn pbr_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &SpriteLayout,
    (normals, specular): (Vec<u8>, Vec<u8>),
    gl: bool,
) -> Vec<[wgpu::Texture; 2]> {
    let mut start = 0;
    layout
        .arrays
        .iter()
        .map(|array| {
            let size = array.pbr_size;
            let layer_bytes = chain_bytes(size);
            let count = array_layers(array.layers.max(1), gl);
            let end = start + array.layers as usize * layer_bytes;
            let maps = [("LabPBR normals", &normals), ("LabPBR specular", &specular)].map(
                |(label, data)| {
                    let mut data = data[start..end].to_vec();
                    data.resize(layer_bytes * count as usize, 0);
                    device.create_texture_with_data(
                        queue,
                        &wgpu::TextureDescriptor {
                            label: Some(label),
                            size: wgpu::Extent3d {
                                width: size,
                                height: size,
                                depth_or_array_layers: count,
                            },
                            mip_level_count: size.trailing_zeros() + 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            usage: wgpu::TextureUsages::TEXTURE_BINDING
                                | wgpu::TextureUsages::COPY_SRC
                                | wgpu::TextureUsages::COPY_DST,
                            view_formats: &[],
                        },
                        wgpu::util::TextureDataOrder::LayerMajor,
                        &data,
                    )
                },
            );
            start = end;
            maps
        })
        .collect()
}

/// The block sprite arrays, one per array of `layout`, each at its own size, from their chains.
pub(super) fn sprite_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    chains: Vec<(Vec<u8>, u32)>,
    layout: &SpriteLayout,
    gl: bool,
) -> Vec<wgpu::Texture> {
    chains
        .into_iter()
        .zip(&layout.arrays)
        .map(|((mut mips, levels), array)| {
            let used = array.layers.max(1);
            let count = array_layers(used, gl);
            // The extra layers are empty.
            mips.resize(mips.len() / used as usize * count as usize, 0);
            device.create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("Block sprites"),
                    size: wgpu::Extent3d {
                        width: array.size,
                        height: array.size,
                        depth_or_array_layers: count,
                    },
                    mip_level_count: levels,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_DST
                        // Shader packs' atlas copies them (pack_atlas.rs).
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::STORAGE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &mips,
            )
        })
        .collect()
}

/// Every layer's mip levels, layer major (`first` is the first layer's
/// index into `settings`), made by `MipmapGenerator`.
pub(super) fn mip_chain(
    pixels: &[u8],
    size: u32,
    settings: &[crate::mipmap::Settings],
    first: u32,
    layers: u32,
) -> (Vec<u8>, u32) {
    let levels = size.trailing_zeros() + 1;
    let layer_bytes = (size * size * 4) as usize;
    // Exactly the chain's size: cached animation frames count their capacity.
    let chain_bytes: usize = (0..levels).map(|l| layer_bytes >> (2 * l)).sum();
    let mut out = Vec::with_capacity(chain_bytes * layers as usize);
    // Every layer makes its own mips from its own pixels, and a pack's array holds
    // thousands of them: the layers run on every thread, in chunks so a worker
    // allocates one chain per chunk rather than one per layer. The chunks are
    // appended in order, so the chain stays layer major.
    const LAYERS_PER_CHUNK: u32 = 16;
    let chunks: Vec<(u32, u32)> = (0..layers)
        .step_by(LAYERS_PER_CHUNK as usize)
        .map(|start| (start, (layers - start).min(LAYERS_PER_CHUNK)))
        .collect();
    let parts = crate::par::par_map(&chunks, |&(start, count)| {
        mip_chain_layers(
            &pixels[start as usize * layer_bytes..],
            size,
            settings,
            first + start,
            count,
            levels - 1,
            layer_bytes,
        )
    });
    for part in parts {
        out.extend_from_slice(&part);
    }
    debug_assert_eq!(out.len(), chain_bytes * layers as usize);
    (out, levels)
}

/// The mip chain of `layers` layers from `pixels` that start at layer `first`:
/// `layers`' base levels followed by their reductions, layer major.
fn mip_chain_layers(
    pixels: &[u8],
    size: u32,
    settings: &[crate::mipmap::Settings],
    first: u32,
    layers: u32,
    levels: u32,
    layer_bytes: usize,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        (0..=levels).map(|l| layer_bytes >> (2 * l)).sum::<usize>() * layers as usize,
    );
    for layer in 0..layers as usize {
        let base = crate::mipmap::Image {
            width: size,
            height: size,
            pixels: pixels[layer * layer_bytes..(layer + 1) * layer_bytes]
                .as_chunks::<4>()
                .0
                .to_vec(),
        };
        let s = settings
            .get(first as usize + layer)
            .copied()
            .unwrap_or_default();
        let has_transparent = s.has_transparent || base.pixels.iter().any(|p| p[3] == 0);
        for image in crate::mipmap::generate(
            base,
            levels,
            crate::mipmap::Settings {
                has_transparent,
                ..s
            },
        ) {
            out.extend(image.pixels.iter().flatten());
        }
    }
    out
}
