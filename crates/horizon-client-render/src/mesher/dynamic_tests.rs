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
    let render = assets.reload(&root).unwrap();
    let facts = BlockFacts::load(&root, assets.registry).unwrap();
    let visibility = crate::models::load_visibility(&root, assets.registry).unwrap();
    let tables = MeshTables::new(&assets, &render, &facts, visibility).unwrap();
    let biomes = Biomes::load(&root).unwrap();
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
            assets: &assets,
            render: &render,
            tables: &tables,
            biomes: &biomes,
            visibility: tables.visibility.clone(),
            zoom: BiomeZoom::new(0),
            blend_radius: 0,
            smooth_lighting: true,
            cutout_leaves: true,
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
    dynamic_models::put_geometry(1, Arc::from(vec![quad]));
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

    dynamic_models::place([5, 64, 9], None);
    let back = mesh();
    assert_eq!(back.solid.len() + back.translucent.len(), own_vertices);
    dynamic_models::set_dynamic_states([]);
    dynamic_models::clear();
}
