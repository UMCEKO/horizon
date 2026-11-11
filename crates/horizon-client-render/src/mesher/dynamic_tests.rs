use super::*;
use horizon_assets::model::{BakedLayer, BakedQuadData, Direction};

/// A block whose geometry a mod bakes meshes the registry's quads, and its static model again once removed.
#[test]
fn a_dynamic_block_meshes_the_registry_quads_instead_of_its_model() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/horizon-client/26.3");
    if !root.join("horizon-resources.json").exists() {
        return;
    }
    let _guard = dynamic_models::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let assets = Assets::load(&root).unwrap();
    let render = assets
        .reload(&horizon_assets::resource_source::ResourceRoot::directory(
            root.to_path_buf(),
        ))
        .unwrap();
    let facts = BlockFacts::load(&root, assets.registry).unwrap();
    let visibility = crate::models::load_visibility(&root, assets.registry).unwrap();
    let tables = MeshTables::new(&assets, &render, &facts, visibility, None).unwrap();
    let biomes = Biomes::load(&horizon_assets::resource_source::ResourceRoot::directory(
        root.to_path_buf(),
    ))
    .unwrap();
    let torch = assets.registry.default_state("minecraft:torch").unwrap();

    let mut chunk = Chunk::empty(0, 0);
    chunk.set(5, 64, 9, torch.0);
    let sy = (64 - chunk.min_y()) >> 4;
    let local_y = ((64 - chunk.min_y()) & 15) as f32;
    let mut region = [[None; 3]; 3];
    region[1][1] = Some(&chunk);
    let mesh = || {
        let mut mesher = Mesher {
            nether_shading: false,
            shader_pack_lighting: false,
            separate_ao: false,
            shader_pack: false,
            assets: &assets,
            render: &render,
            tables: &tables,
            biomes: &biomes,
            visibility: tables.visibility.clone(),
            zoom: BiomeZoom::new(0),
            blend_radius: 0,
            smooth_lighting: true,
            cutout_leaves: true,
            opaque_ice: false,
            translucent_layers: &[],
            leaf_culling: crate::graphics::LeafCullingMode::Off,
            hidden_states: &[],
        };
        mesher.mesh(&region, sy)
    };

    dynamic_models::clear();
    let own = mesh();
    let own_vertices = own.solid.len() + own.translucent.len();
    assert!(own_vertices > 0);

    dynamic_models::set_dynamic_states([torch]);
    let unplaced = mesh();
    assert_eq!(
        unplaced.solid.len() + unplaced.translucent.len(),
        own_vertices
    );
    let quad = BakedQuadData {
        vertices: [
            [0.0, 0.5, 0.0, 0.0, 0.0],
            [0.0, 0.5, 1.0, 0.0, 1.0],
            [1.0, 0.5, 1.0, 1.0, 1.0],
            [1.0, 0.5, 0.0, 1.0, 0.0],
        ],
        sprite: "minecraft:block/stone".to_owned(),
        face: Direction::Up,
        cull: None,
        tint: -1,
        shade: true,
        emission: 0,
        layer: BakedLayer::Solid,
    };
    dynamic_models::put_geometry(1, Arc::from(vec![quad]), Arc::from([]));
    dynamic_models::place([5, 64, 9], Some(1));
    let dynamic = mesh();
    assert_eq!(dynamic.solid.len() + dynamic.translucent.len(), 4);
    let stone = render.sprite_layers["minecraft:block/stone"];
    let corner = dynamic
        .solid
        .iter()
        .chain(&dynamic.translucent)
        .find(|v| v.pos == [5.0, local_y + 0.5, 9.0])
        .expect("the quad's first corner");
    assert_eq!(corner.layer, stone as u16 | OPAQUE_LAYER);

    assert_ne!(corner.color & 0xFF_FF00, 0);
    let tinted = dynamic_models::at([5, 64, 9]).unwrap();
    dynamic_models::put_geometry(1, tinted, Arc::from([-65536]));
    let red = mesh();
    let corner = red
        .solid
        .iter()
        .chain(&red.translucent)
        .find(|v| v.pos == [5.0, local_y + 0.5, 9.0])
        .expect("the quad's first corner");
    assert_eq!(corner.color & 0xFF_FF00, 0);

    dynamic_models::place([5, 64, 9], None);
    let back = mesh();
    assert_eq!(back.solid.len() + back.translucent.len(), own_vertices);
    dynamic_models::set_dynamic_states([]);
    dynamic_models::clear();
}

/// Meshing a section of blocks drawn from block entity geometry costs about what their native models do.
#[test]
#[ignore = "a timing comparison: cargo test --release ... -- --ignored --nocapture"]
fn dynamic_geometry_meshes_as_fast_as_native_models() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/horizon-client/26.3");
    if !root.join("horizon-resources.json").exists() {
        return;
    }
    let _guard = dynamic_models::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let assets = Assets::load(&root).unwrap();
    let render = assets
        .reload(&horizon_assets::resource_source::ResourceRoot::directory(
            root.to_path_buf(),
        ))
        .unwrap();
    let facts = BlockFacts::load(&root, assets.registry).unwrap();
    let visibility = crate::models::load_visibility(&root, assets.registry).unwrap();
    let tables = MeshTables::new(&assets, &render, &facts, visibility, None).unwrap();
    let biomes = Biomes::load(&horizon_assets::resource_source::ResourceRoot::directory(
        root.to_path_buf(),
    ))
    .unwrap();
    let stone = assets.registry.default_state("minecraft:stone").unwrap();
    let mut chunk = Chunk::empty(0, 0);
    let mut placed = Vec::new();
    for x in 0..16 {
        for y in 64..80 {
            for z in 0..16 {
                if (x + y + z) % 2 == 0 {
                    chunk.set(x, y, z, stone.0);
                    placed.push([x, y, z]);
                }
            }
        }
    }
    let sy = (64 - chunk.min_y()) >> 4;
    let mut region = [[None; 3]; 3];
    region[1][1] = Some(&chunk);
    let mesh = || {
        let mut mesher = Mesher {
            nether_shading: false,
            shader_pack_lighting: false,
            separate_ao: false,
            shader_pack: false,
            assets: &assets,
            render: &render,
            tables: &tables,
            biomes: &biomes,
            visibility: tables.visibility.clone(),
            zoom: BiomeZoom::new(0),
            blend_radius: 0,
            smooth_lighting: true,
            cutout_leaves: true,
            opaque_ice: false,
            translucent_layers: &[],
            leaf_culling: crate::graphics::LeafCullingMode::Off,
            hidden_states: &[],
        };
        mesher.mesh(&region, sy)
    };
    let time = |label: &str| {
        let first = mesh();
        let start = std::time::Instant::now();
        for _ in 0..40 {
            std::hint::black_box(mesh());
        }
        let each = start.elapsed() / 40;
        eprintln!(
            "{label}: {each:?} a section, {} vertices",
            first.solid.len() + first.translucent.len()
        );
        each
    };
    dynamic_models::clear();
    dynamic_models::set_dynamic_states([]);
    let native = time("native models");
    let face = |face: Direction, corners: [[f32; 3]; 4]| BakedQuadData {
        vertices: [0, 1, 2, 3].map(|i| {
            let [x, y, z] = corners[i];
            [x, y, z, [0.0, 0.0, 1.0, 1.0][i], [0.0, 1.0, 1.0, 0.0][i]]
        }),
        sprite: "minecraft:block/stone".to_owned(),
        face,
        cull: Some(face),
        tint: -1,
        shade: true,
        emission: 0,
        layer: BakedLayer::Solid,
    };
    let cube = vec![
        face(
            Direction::Down,
            [[0., 0., 1.], [0., 0., 0.], [1., 0., 0.], [1., 0., 1.]],
        ),
        face(
            Direction::Up,
            [[0., 1., 0.], [0., 1., 1.], [1., 1., 1.], [1., 1., 0.]],
        ),
        face(
            Direction::North,
            [[1., 1., 0.], [1., 0., 0.], [0., 0., 0.], [0., 1., 0.]],
        ),
        face(
            Direction::South,
            [[0., 1., 1.], [0., 0., 1.], [1., 0., 1.], [1., 1., 1.]],
        ),
        face(
            Direction::West,
            [[0., 1., 0.], [0., 0., 0.], [0., 0., 1.], [0., 1., 1.]],
        ),
        face(
            Direction::East,
            [[1., 1., 1.], [1., 0., 1.], [1., 0., 0.], [1., 1., 0.]],
        ),
    ];
    dynamic_models::put_geometry(1, Arc::from(cube), Arc::from([]));
    for pos in &placed {
        dynamic_models::place(*pos, Some(1));
    }
    dynamic_models::set_dynamic_states([stone]);
    let dynamic = time("block entity geometry");
    dynamic_models::set_dynamic_states([]);
    dynamic_models::clear();
    assert!(
        dynamic.as_secs_f64() < native.as_secs_f64() * 1.5,
        "{dynamic:?} against {native:?}"
    );
}
