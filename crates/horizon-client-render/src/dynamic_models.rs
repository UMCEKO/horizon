//! Geometry baked from a block entity's data (cable connections, inserted cells), cached per position.
use crate::{assets::RenderAssets, models::catalog::BakedFace};
use horizon_assets::{
    bake::baked_model_quad,
    model::{BakedLayer, BakedQuadData},
};
use horizon_world::registry::BlockStateId;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc, LazyLock, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
};

/// One interned geometry and its quads converted for the mesher, per the sprite table they were made for.
struct Geometry {
    quads: Arc<[BakedQuadData]>,
    faces: Mutex<Option<(SpriteTable, Arc<[BakedFace]>)>>,
}
/// Identifies a `RenderAssets` generation: its address and sprite count.
type SpriteTable = (usize, usize);
#[derive(Default)]
struct Registry {
    geometry: HashMap<i32, Arc<Geometry>>,
    placed: HashMap<[i32; 3], i32>,
}

static ANY_STATE: AtomicBool = AtomicBool::new(false);
static STATES: RwLock<Vec<bool>> = RwLock::new(Vec::new());
static REGISTRY: LazyLock<RwLock<Registry>> = LazyLock::new(RwLock::default);
#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Replaces the block states whose geometry may come from this registry.
pub fn set_dynamic_states(states: impl IntoIterator<Item = BlockStateId>) {
    let mut set = Vec::new();
    for state in states {
        let at = state.0 as usize;
        if set.len() <= at {
            set.resize(at + 1, false);
        }
        set[at] = true;
    }
    ANY_STATE.store(!set.is_empty(), Ordering::Release);
    *STATES.write().unwrap() = set;
    for geometry in REGISTRY.read().unwrap().geometry.values() {
        *geometry.faces.lock().unwrap() = None;
    }
}
/// Whether this state's geometry may come from [`at`]; runs for every block meshed.
pub fn is_dynamic(state: BlockStateId) -> bool {
    ANY_STATE.load(Ordering::Acquire)
        && STATES
            .read()
            .unwrap()
            .get(state.0 as usize)
            .copied()
            .unwrap_or(false)
}
/// Stores geometry under `key`; true when it replaced different quads (positions using the key change).
pub fn put_geometry(key: i32, quads: Arc<[BakedQuadData]>) -> bool {
    let mut registry = REGISTRY.write().unwrap();
    if registry
        .geometry
        .get(&key)
        .is_some_and(|old| *old.quads == *quads)
    {
        return false;
    }
    let replaced = registry.geometry.contains_key(&key);
    registry.geometry.insert(
        key,
        Arc::new(Geometry {
            quads,
            faces: Mutex::new(None),
        }),
    );
    replaced
}
/// Sets the geometry at a position (`None` removes it); true when that changed the position's key.
pub fn place(pos: [i32; 3], key: Option<i32>) -> bool {
    if key.is_none() && !REGISTRY.read().unwrap().placed.contains_key(&pos) {
        return false;
    }
    let mut registry = REGISTRY.write().unwrap();
    match key {
        Some(key) => registry.placed.insert(pos, key) != Some(key),
        None => registry.placed.remove(&pos).is_some(),
    }
}
/// The positions currently drawn with the geometry stored under `key`.
pub fn positions_of(key: i32) -> Vec<[i32; 3]> {
    let registry = REGISTRY.read().unwrap();
    registry
        .placed
        .iter()
        .filter(|(_, k)| **k == key)
        .map(|(pos, _)| *pos)
        .collect()
}
pub fn at(pos: [i32; 3]) -> Option<Arc<[BakedQuadData]>> {
    let registry = REGISTRY.read().unwrap();
    Some(
        registry
            .geometry
            .get(registry.placed.get(&pos)?)?
            .quads
            .clone(),
    )
}
/// The mesher's faces for the geometry at `pos`, converted once per geometry and sprite table.
pub fn faces_at(pos: [i32; 3], render: &RenderAssets) -> Option<Arc<[BakedFace]>> {
    let geometry = {
        let registry = REGISTRY.read().unwrap();
        registry.geometry.get(registry.placed.get(&pos)?)?.clone()
    };
    let table = (
        render as *const RenderAssets as usize,
        render.sprite_layers.len(),
    );
    let mut cached = geometry.faces.lock().unwrap();
    if let Some((made_for, faces)) = &*cached
        && *made_for == table
    {
        return Some(faces.clone());
    }
    let faces: Arc<[BakedFace]> = geometry
        .quads
        .iter()
        .map(|quad| to_face(quad, &render.sprite_layers, &render.translucent_layers))
        .collect();
    *cached = Some((table, faces.clone()));
    Some(faces)
}
/// Drops every position and geometry (the window left its world); the dynamic states stay.
pub fn clear() {
    *REGISTRY.write().unwrap() = Registry::default();
}

fn to_face(
    quad: &BakedQuadData,
    layers: &BTreeMap<String, u32>,
    translucent_layers: &[bool],
) -> BakedFace {
    let name = if quad.sprite.contains(':') {
        quad.sprite.clone()
    } else {
        format!("minecraft:{}", quad.sprite)
    };
    if !layers.contains_key(&name) {
        log::warn!("Horizon: a block entity's model uses {name}, which the block atlas lacks");
    }
    let layer = layers
        .get(&name)
        .or_else(|| layers.get("minecraft:missingno"))
        .copied()
        .unwrap_or(0);
    let model = baked_model_quad(quad);
    BakedFace {
        translucent: model.material.force_translucent
            || translucent_layers
                .get(layer as usize)
                .copied()
                .unwrap_or(false),
        opaque: quad.layer == BakedLayer::Solid,
        quad: model,
        layer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_assets::model::Direction;

    fn quad(sprite: &str, layer: BakedLayer) -> BakedQuadData {
        BakedQuadData {
            vertices: [
                [0.0, 0.0, 0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0, 1.0],
                [1.0, 0.0, 1.0, 1.0, 1.0],
                [1.0, 0.0, 0.0, 1.0, 0.0],
            ],
            sprite: sprite.to_owned(),
            face: Direction::Down,
            cull: Some(Direction::Down),
            tint: -1,
            shade: true,
            emission: 0,
            layer,
        }
    }

    #[test]
    fn positions_resolve_through_interned_geometry_and_report_changes() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        let quads: Arc<[BakedQuadData]> =
            Arc::from(vec![quad("ae2:block/cable", BakedLayer::Solid)]);
        assert!(!put_geometry(7, quads.clone()));
        assert!(at([1, 2, 3]).is_none());
        assert!(place([1, 2, 3], Some(7)));
        assert!(!place([1, 2, 3], Some(7)));
        assert!(place([-4, 70, 9], Some(7)));
        assert_eq!(at([1, 2, 3]).as_deref(), Some(&*quads));
        assert!(!put_geometry(7, quads.clone()));
        let other: Arc<[BakedQuadData]> =
            Arc::from(vec![quad("ae2:block/other", BakedLayer::Cutout)]);
        assert!(put_geometry(7, other.clone()));
        assert_eq!(at([-4, 70, 9]).as_deref(), Some(&*other));
        let mut users = positions_of(7);
        users.sort();
        assert_eq!(users, [[-4, 70, 9], [1, 2, 3]]);
        assert!(place([1, 2, 3], None));
        assert!(!place([1, 2, 3], None));
        assert!(at([1, 2, 3]).is_none());
        assert!(place([-4, 70, 9], Some(8)));
        assert!(at([-4, 70, 9]).is_none());
        clear();
        assert!(at([-4, 70, 9]).is_none());
        assert!(!place([-4, 70, 9], None));
    }

    #[test]
    fn dynamic_states_are_replaced_whole() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_dynamic_states([BlockStateId(3), BlockStateId(40)]);
        assert!(is_dynamic(BlockStateId(3)));
        assert!(is_dynamic(BlockStateId(40)));
        assert!(!is_dynamic(BlockStateId(4)));
        assert!(!is_dynamic(BlockStateId(5000)));
        set_dynamic_states([BlockStateId(4)]);
        assert!(!is_dynamic(BlockStateId(3)));
        assert!(is_dynamic(BlockStateId(4)));
        set_dynamic_states([]);
        assert!(!is_dynamic(BlockStateId(4)));
    }

    #[test]
    fn faces_take_the_atlas_layer_and_fall_back_to_the_missing_texture() {
        let layers = BTreeMap::from([
            ("ae2:block/cable".to_owned(), 5),
            ("minecraft:block/glass".to_owned(), 6),
            ("minecraft:missingno".to_owned(), 9),
        ]);
        let translucent = [false, false, false, false, false, false, true];
        let solid = to_face(
            &quad("ae2:block/cable", BakedLayer::Solid),
            &layers,
            &translucent,
        );
        assert_eq!(
            (solid.layer, solid.opaque, solid.translucent),
            (5, true, false)
        );
        assert_eq!(solid.quad.positions[2], [1.0, 0.0, 1.0]);
        assert_eq!(solid.quad.uv[1], [0.0, 1.0]);
        assert_eq!(solid.quad.cull, Some(Direction::Down));
        let glass = to_face(
            &quad("block/glass", BakedLayer::Cutout),
            &layers,
            &translucent,
        );
        assert_eq!(
            (glass.layer, glass.opaque, glass.translucent),
            (6, false, true)
        );
        let forced = to_face(
            &quad("ae2:block/cable", BakedLayer::Translucent),
            &layers,
            &translucent,
        );
        assert!(forced.translucent);
        let missing = to_face(
            &quad("ae2:block/nope", BakedLayer::Solid),
            &layers,
            &translucent,
        );
        assert_eq!(missing.layer, 9);
    }
}
