//! Cuboid parent inheritance and texture-slot resolution.
use crate::{
    json::*,
    model::*,
    obj::{ObjModel, parse_mtl, parse_obj},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

pub const MISSING_MODEL: &str = "minecraft:builtin/missing";
/// `ItemModelGenerator.GENERATED_ITEM_MODEL_ID`.
pub const GENERATED_MODEL: &str = "minecraft:builtin/generated";
/// Composite children nest through parents that may name the composite again.
const MAX_LOADER_DEPTH: u8 = 8;

/// Reads asset files that models point at (`namespace:path` is `assets/namespace/path`).
pub trait AssetReader: Send + Sync {
    fn read_text(&self, location: &str) -> Option<String>;
}
/// Decoded models are immutable and their inherited geometry is shared.
pub struct ModelLibrary {
    models: BTreeMap<String, Arc<CuboidModel>>,
    missing: Arc<CuboidModel>,
    assets: Option<Arc<dyn AssetReader>>,
    /// Keyed by obj and mtl override; `None` is a file that failed, already warned about.
    objs: Mutex<BTreeMap<(String, Option<String>), Option<Arc<ObjModel>>>>,
}
/// What a model's loader adds to its own elements.
#[derive(Clone, Debug)]
pub enum ResolvedLoader {
    /// The shown children, resolved on their own, in order.
    Composite(Vec<ResolvedCuboidModel>),
    Obj {
        /// `None` when the file failed to load.
        model: Option<Arc<ObjModel>>,
        flip_v: bool,
        shade: bool,
        /// Objects the model chain's `visibility` hides.
        hidden: BTreeSet<String>,
    },
    /// Finished quads from `horizon:baked`.
    Baked(Arc<[BakedQuadData]>),
}
#[derive(Clone, Debug)]
pub struct ResolvedCuboidModel {
    pub ambient_occlusion: bool,
    pub gui_light: GuiLight,
    pub display: ItemTransforms,
    pub textures: BTreeMap<String, Material>,
    /// Cyclic and dangling aliases are absent from `textures`.
    pub unresolved_textures: BTreeSet<String>,
    geometry: Option<Arc<CuboidModel>>,
    generated: bool,
    loader: Option<ResolvedLoader>,
}
impl ResolvedCuboidModel {
    /// Geometry from a custom loader, baked after the elements.
    pub fn loader(&self) -> Option<&ResolvedLoader> {
        self.loader.as_ref()
    }
    /// Whether the top geometry is `builtin/generated`'s layer extrusion
    /// (no model closer to the leaf has elements of its own).
    pub fn uses_item_generator(&self) -> bool {
        self.generated
    }
    pub fn elements(&self) -> &[Option<Element>] {
        self.geometry
            .as_ref()
            .and_then(|m| m.elements.as_deref())
            .unwrap_or(&[])
    }
    /// Native slot lookup strips a single leading '#'. Non-reference names
    /// are still slot names, not direct sprite identifiers.
    pub fn material(&self, slot: &str) -> Result<Option<&Material>> {
        if slot.is_empty() {
            return Err(error("empty texture lookup"));
        }
        Ok(self.textures.get(slot.strip_prefix('#').unwrap_or(slot)))
    }
}
impl ModelLibrary {
    /// A library that also knows `builtin/generated`, the parent of every
    /// flat item model; the native client registers it as a special model.
    pub fn with_item_generator(mut models: BTreeMap<String, CuboidModel>) -> Result<Self> {
        models.insert(GENERATED_MODEL.to_owned(), Self::item_generator_model());
        Self::new(models)
    }
    /// Register the item built-in while reusing already decoded resources.
    pub fn with_shared_item_generator(
        mut models: BTreeMap<String, Arc<CuboidModel>>,
    ) -> Result<Self> {
        models.insert(
            GENERATED_MODEL.to_owned(),
            Arc::new(Self::item_generator_model()),
        );
        Self::from_shared(models)
    }
    fn item_generator_model() -> CuboidModel {
        CuboidModel {
            parent: None,
            loader: None,
            ambient_occlusion: None,
            gui_light: Some(GuiLight::Front),
            textures: BTreeMap::from([(
                "particle".to_owned(),
                TextureSlot::Reference("layer0".to_owned()),
            )]),
            elements: None,
            display: None,
            visibility: BTreeMap::new(),
        }
    }
    pub fn new(models: BTreeMap<String, CuboidModel>) -> Result<Self> {
        Self::from_models(
            models
                .into_iter()
                .map(|(name, model)| (name, Arc::new(model))),
        )
    }
    /// Build an immutable resolution view without copying model geometry.
    pub fn from_shared(models: BTreeMap<String, Arc<CuboidModel>>) -> Result<Self> {
        Self::from_models(models)
    }
    fn from_models(models: impl IntoIterator<Item = (String, Arc<CuboidModel>)>) -> Result<Self> {
        let mut decoded = BTreeMap::new();
        for (name, model) in models {
            if decoded.insert(identifier(&name)?, model).is_some() {
                return Err(error("duplicate normalized model identifier"));
            }
        }
        let missing = Arc::new(missing_model());
        // The native discovery registers this built-in before consulting resources.
        decoded.insert(MISSING_MODEL.to_owned(), missing.clone());
        Ok(Self {
            models: decoded,
            missing,
            assets: None,
            objs: Mutex::default(),
        })
    }
    /// Where `neoforge:obj` models find their files.
    pub fn with_assets(mut self, assets: Arc<dyn AssetReader>) -> Self {
        self.assets = Some(assets);
        self
    }
    /// Follow the complete parent chain. A missing resource uses the built-in
    /// model; a cyclic chain is rejected, as native discovery excludes it.
    /// The returned model can outlive this library without copying its elements.
    pub fn resolve(&self, name: &str) -> Result<ResolvedCuboidModel> {
        let name = identifier(name)?;
        self.resolve_chain(
            Vec::new(),
            Vec::new(),
            Some(name.clone()),
            &BTreeMap::new(),
            0,
            &name,
        )
    }
    /// `chain` and `names` start with models that are not in the library (a
    /// composite child); `next` is the parent to follow. `fallback` slots lose
    /// to any slot the chain defines.
    fn resolve_chain(
        &self,
        mut chain: Vec<Arc<CuboidModel>>,
        mut names: Vec<String>,
        mut next: Option<String>,
        fallback: &BTreeMap<String, TextureSlot>,
        depth: u8,
        label: &str,
    ) -> Result<ResolvedCuboidModel> {
        if depth > MAX_LOADER_DEPTH {
            return Err(error("composite models nest too deeply"));
        }
        let mut visited = BTreeSet::new();
        while let Some(name) = next.take() {
            if !visited.insert(name.clone()) {
                return Err(error("cyclic model parent"));
            }
            let model = self.models.get(&name).unwrap_or(&self.missing);
            chain.push(Arc::clone(model));
            names.push(name);
            next = model.parent.clone();
        }
        let ambient_occlusion = chain
            .iter()
            .find_map(|m| m.ambient_occlusion)
            .unwrap_or(true);
        let gui_light = chain
            .iter()
            .find_map(|m| m.gui_light)
            .unwrap_or(GuiLight::Side);
        let geometry = chain.iter().find(|m| m.elements.is_some()).map(Arc::clone);
        let generated = chain
            .iter()
            .zip(&names)
            .find(|(m, name)| m.elements.is_some() || *name == GENERATED_MODEL)
            .is_some_and(|(_, name)| name == GENERATED_MODEL);
        let display = ItemTransforms {
            contexts: std::array::from_fn(|index| {
                chain
                    .iter()
                    .filter_map(|m| m.display.as_ref())
                    .map(|d| &d.contexts[index])
                    .find(|t| **t != DisplayTransform::Inherit)
                    .cloned()
                    .unwrap_or(DisplayTransform::Inherit)
            }),
        };
        // Resolve aliases only after applying every child override. A reference
        // inherited from a parent binds to the child's final slot definition.
        let mut slots: BTreeMap<String, &TextureSlot> =
            fallback.iter().map(|(k, v)| (k.clone(), v)).collect();
        for model in chain.iter().rev() {
            slots.extend(model.textures.iter().map(|(k, v)| (k.clone(), v)));
        }
        let loader = match chain.iter().find_map(|m| m.loader.as_ref()) {
            Some(Loader::Composite {
                children,
                visibility,
            }) => {
                let inherited: BTreeMap<String, TextureSlot> = slots
                    .iter()
                    .map(|(k, v)| (k.clone(), (*v).clone()))
                    .collect();
                let mut resolved = Vec::new();
                for (child_name, child) in children {
                    if visibility.get(child_name) == Some(&false) {
                        continue;
                    }
                    resolved.push(self.resolve_chain(
                        vec![Arc::clone(child)],
                        vec![format!("{label}#{child_name}")],
                        child.parent.clone(),
                        &inherited,
                        depth + 1,
                        &format!("{label}#{child_name}"),
                    )?);
                }
                Some(ResolvedLoader::Composite(resolved))
            }
            Some(Loader::Obj {
                model,
                flip_v,
                shade_quads,
                mtl_override,
            }) => {
                let mut visibility = BTreeMap::new();
                for model in chain.iter().rev() {
                    visibility.extend(model.visibility.iter().map(|(k, v)| (k.clone(), *v)));
                }
                Some(ResolvedLoader::Obj {
                    model: self.load_obj(label, model, mtl_override.as_deref()),
                    flip_v: *flip_v,
                    shade: shade_quads.unwrap_or(true),
                    hidden: visibility
                        .into_iter()
                        .filter(|(_, v)| !v)
                        .map(|(k, _)| k)
                        .collect(),
                })
            }
            Some(Loader::Baked { quads }) => Some(ResolvedLoader::Baked(quads.as_slice().into())),
            Some(Loader::Unknown(_)) | None => None,
        };
        let mut textures = BTreeMap::<String, Material>::new();
        let mut unresolved_textures = BTreeSet::<String>::new();
        for start in slots.keys() {
            let mut current = start.as_str();
            let mut seen = BTreeSet::new();
            let mut path = Vec::new();
            let material = loop {
                if let Some(material) = textures.get(current) {
                    break Some(material.clone());
                }
                if unresolved_textures.contains(current) || !seen.insert(current) {
                    break None;
                }
                let Some(slot) = slots.get(current) else {
                    break None;
                };
                path.push(current);
                match slot {
                    TextureSlot::Reference(target) => current = target,
                    TextureSlot::Material(material) => break Some(material.clone()),
                }
            };
            // Cache the entire walk, including failures, so long alias chains
            // and cycles are traversed once rather than once per referencing slot.
            match material {
                Some(material) => {
                    for name in path {
                        textures.insert(name.to_owned(), material.clone());
                    }
                }
                None => {
                    unresolved_textures.extend(path.into_iter().map(str::to_owned));
                }
            }
        }
        Ok(ResolvedCuboidModel {
            ambient_occlusion,
            gui_light,
            display,
            textures,
            unresolved_textures,
            geometry,
            generated,
            loader,
        })
    }
    /// Parse an obj and its materials once; a failure warns once and yields no geometry.
    fn load_obj(&self, label: &str, model: &str, mtl: Option<&str>) -> Option<Arc<ObjModel>> {
        let key = (model.to_owned(), mtl.map(str::to_owned));
        let mut cache = self.objs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(found) = cache.get(&key) {
            return found.clone();
        }
        let loaded = self.read_obj(model, mtl);
        if let Err(message) = &loaded {
            log::warn!("Horizon: obj model {label} ({model}): {message}");
        }
        let loaded = loaded.ok().map(Arc::new);
        cache.insert(key, loaded.clone());
        loaded
    }
    fn read_obj(&self, model: &str, mtl: Option<&str>) -> std::result::Result<ObjModel, String> {
        let assets = self.assets.as_ref().ok_or("no asset source")?;
        let text = assets.read_text(model).ok_or("file not found")?;
        let parsed = parse_obj(&text)?;
        let mtl = match (mtl, &parsed.mtllib) {
            (Some(location), _) => Some(location.to_owned()),
            (None, Some(file)) if file.contains(':') => Some(file.clone()),
            (None, Some(file)) => {
                let (namespace, path) = model.split_once(':').unwrap_or(("minecraft", model));
                let directory = path.rsplit_once('/').map_or("", |(dir, _)| dir);
                Some(if directory.is_empty() {
                    format!("{namespace}:{file}")
                } else {
                    format!("{namespace}:{directory}/{file}")
                })
            }
            (None, None) => None,
        };
        let materials = match mtl {
            Some(location) => match assets.read_text(&location) {
                Some(text) => parse_mtl(&text),
                None => {
                    log::warn!("Horizon: obj material file {location} not found");
                    BTreeMap::new()
                }
            },
            None => BTreeMap::new(),
        };
        Ok(ObjModel {
            faces: parsed.faces,
            materials,
        })
    }
}

/// The native missing-model cube uses slot names without a leading '#'.
pub fn missing_model() -> CuboidModel {
    let faces = Direction::ALL
        .into_iter()
        .map(|direction| {
            (
                direction,
                Some(Face {
                    cull: Some(direction),
                    tint_index: -1,
                    texture: "missingno".to_owned(),
                    uv: Some([0.0, 0.0, 16.0, 16.0]),
                    rotation: 0,
                }),
            )
        })
        .collect();
    CuboidModel {
        parent: None,
        loader: None,
        ambient_occlusion: None,
        gui_light: None,
        elements: Some(vec![Some(Element {
            from: [0.0; 3],
            to: [16.0; 3],
            faces,
            rotation: None,
            shade_direction_override: None,
            light_emission: 0,
        })]),
        display: Some(ItemTransforms {
            contexts: std::array::from_fn(|_| DisplayTransform::Inherit),
        }),
        textures: BTreeMap::from([
            (
                "particle".to_owned(),
                TextureSlot::Reference("missingno".to_owned()),
            ),
            (
                "missingno".to_owned(),
                TextureSlot::Material(Material {
                    sprite: "minecraft:missingno".to_owned(),
                    force_translucent: false,
                }),
            ),
        ]),
        visibility: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inherited_alias_uses_child_material_and_geometry_outlives_library() {
        let parent = CuboidModel::from_json(r##"{"textures":{"side":"#all","all":"block/stone"},"elements":[{"from":[0,0,0],"to":[16,8,16],"faces":{"up":{"texture":"#side"}}}]}"##).unwrap();
        let child = CuboidModel::from_json(r#"{"parent":"horizon:parent","textures":{"all":{"sprite":"block/glass","force_translucent":true}}}"#).unwrap();
        let library = ModelLibrary::new(BTreeMap::from([
            ("horizon:parent".into(), parent),
            ("horizon:child".into(), child),
        ]))
        .unwrap();
        let model = library.resolve("horizon:child").unwrap();
        drop(library);
        assert_eq!(model.elements()[0].as_ref().unwrap().to, [16.0, 8.0, 16.0]);
        assert_eq!(
            model.material("#side").unwrap().unwrap(),
            &Material {
                sprite: "minecraft:block/glass".into(),
                force_translucent: true
            }
        );
    }
    #[test]
    fn absent_parents_fall_back_but_cycles_invalidate_descendants() {
        let a = CuboidModel::from_json(r#"{"parent":"horizon:b"}"#).unwrap();
        let b = CuboidModel::from_json(r#"{"parent":"horizon:a"}"#).unwrap();
        let child = CuboidModel::from_json(r#"{"parent":"horizon:a","elements":[]}"#).unwrap();
        let library = ModelLibrary::new(BTreeMap::from([
            ("horizon:a".into(), a),
            ("horizon:b".into(), b),
            ("horizon:child".into(), child),
        ]))
        .unwrap();
        assert!(library.resolve("horizon:child").is_err());
        let absent = library.resolve("horizon:absent").unwrap();
        assert_eq!(absent.elements().len(), 1);
        assert_eq!(
            absent.material("particle").unwrap().unwrap().sprite,
            "minecraft:missingno"
        );
    }
}
