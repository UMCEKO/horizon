//! The worldgen seams Java runs for a world, decided once at its start from the mod-owned types its export lists.
use super::generator::GeneratorError;
use super::overrides::ExportOwners;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, PoisonError};

/// One part of chunk generation that can run in Java (W10 DESIGN 3.1).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Seam {
    /// The BIOMES stage: the level stem's biome source.
    Biomes,
    /// The whole TERRAIN stage.
    Terrain,
    /// The noise fill: the noise settings' router.
    Fill,
    /// The material surface: the noise settings' material rule.
    Surface,
    /// The structures' terrain adaptation (`Beardifier`).
    Beard,
    /// `spawnOriginalMobs`.
    Spawn,
    /// One configured carver, by id.
    Carver(String),
    /// One placed feature, by id.
    Feature(String),
    /// One structure's starts, by structure id.
    Start(String),
    /// One structure's pieces placed in a chunk, by structure id.
    Place(String),
}

impl Seam {
    /// The generation stage a chunk fails in while the seam's Java side is missing.
    pub fn stage(&self) -> &'static str {
        match self {
            Self::Biomes => "biomes",
            Self::Terrain => "terrain",
            Self::Fill => "noise fill",
            Self::Surface => "surface",
            Self::Beard => "beardifier",
            Self::Spawn => "spawn",
            Self::Carver(_) => "carvers",
            Self::Feature(_) => "features",
            Self::Start(_) => "structure starts",
            Self::Place(_) => "structure placement",
        }
    }
    /// The W10 spec that runs the seam in Java.
    fn spec(&self) -> &'static str {
        match self {
            Self::Biomes => "SPEC-8",
            Self::Start(_) => "SPEC-9",
            Self::Terrain | Self::Fill | Self::Surface | Self::Beard | Self::Carver(_) => "SPEC-10",
            Self::Feature(_) | Self::Place(_) => "SPEC-11",
            Self::Spawn => "SPEC-12",
        }
    }
}

impl std::fmt::Display for Seam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Carver(id) => write!(f, "carver {id}"),
            Self::Feature(id) => write!(f, "feature {id}"),
            Self::Start(id) => write!(f, "structure start {id}"),
            Self::Place(id) => write!(f, "structure placement {id}"),
            other => f.write_str(other.stage()),
        }
    }
}

/// A seam as the export's `routed` block names it: a whole stage, or every entry of one registered type.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MethodSeam {
    Biomes,
    Fill,
    Surface,
    Terrain,
    Beard,
    Spawn,
    /// Every configured carver of this carver type.
    Carver(String),
    /// Every placed feature whose configured feature has this feature type.
    Feature(String),
    /// Every structure of this structure type.
    Start(String),
    /// The same, for its placement.
    Place(String),
}

impl std::str::FromStr for MethodSeam {
    type Err = String;

    fn from_str(key: &str) -> Result<Self, String> {
        let unknown = || format!("routed seam {key} is unknown");
        Ok(match key.split_once(':') {
            None => match key {
                "biomes" => Self::Biomes,
                "fill" => Self::Fill,
                "surface" => Self::Surface,
                "terrain" => Self::Terrain,
                "beard" => Self::Beard,
                "spawn" => Self::Spawn,
                _ => return Err(unknown()),
            },
            Some((_, "")) => return Err(unknown()),
            Some(("carver", ty)) => Self::Carver(normalize(ty)),
            Some(("feature", ty)) => Self::Feature(normalize(ty)),
            Some(("start", ty)) => Self::Start(normalize(ty)),
            Some(("place", ty)) => Self::Place(normalize(ty)),
            Some(_) => return Err(unknown()),
        })
    }
}

/// Bundles read here, by the name `GeneratorData` gives them.
const PLACED: &str = "placed_features";
const FEATURES: &str = "features";
const STRUCTURES: &str = "structures";
const SETS: &str = "structure_sets";
const POOLS: &str = "template_pools";
const PROCESSORS: &str = "processor_lists";
const CARVERS: &str = "carvers";
const SETTINGS: &str = super::overrides::NOISE_SETTINGS;
const RULES: &str = "rules";
const CONDITIONS: &str = "conditions";
const DENSITIES: &str = "densities";
const DIMENSIONS: &str = super::overrides::DIMENSIONS;

/// The bundles an entry of `bundle` names other entries of.
fn follows(bundle: &str) -> &'static [&'static str] {
    match bundle {
        PLACED | FEATURES => &[FEATURES, PLACED],
        STRUCTURES => &[POOLS],
        POOLS => &[POOLS, PROCESSORS, PLACED],
        SETTINGS => &[RULES, DENSITIES],
        RULES => &[RULES, CONDITIONS],
        CONDITIONS => &[CONDITIONS, DENSITIES],
        DENSITIES => &[DENSITIES],
        _ => &[],
    }
}

/// The type registries a type string (`<registry>:<key>`) names, as the seams they feed.
fn type_seams(ty: &str) -> &'static [Seam] {
    if ty.starts_with("unregistered:") {
        &[Seam::Fill, Seam::Surface]
    } else if ty.contains("worldgen/density_function:") {
        &[Seam::Fill]
    } else if ty.contains("worldgen/material_rule:") || ty.contains("worldgen/material_condition:")
    {
        &[Seam::Surface]
    } else {
        &[]
    }
}

/// The bundles an export's entries are read from: bundle name → id → JSON text.
pub type Bundles<'a> = BTreeMap<&'a str, &'a BTreeMap<String, String>>;

/// Which seams Java runs for a world, and why.
#[derive(Debug, Default)]
pub struct WorldgenRoutes {
    java: BTreeSet<Seam>,
    /// The mod-owned types and patched methods (`owner#name(desc)`) behind each seam, for the log.
    why: BTreeMap<Seam, Vec<String>>,
    /// Entries native leaves undecoded because a routed seam owns them, by bundle.
    skipped: BTreeMap<&'static str, BTreeSet<String>>,
    /// Seams whose missing Java side was logged.
    logged: Mutex<BTreeSet<Seam>>,
}

impl WorldgenRoutes {
    /// The routes of the export whose owners are `owners`, over the merged bundles `bundles`.
    pub fn from_export(owners: &ExportOwners, bundles: &Bundles<'_>) -> Self {
        let mut routes = Self::default();
        routes.add_methods(owners.routed(), bundles);
        if owners.with_mod_types() > 0 {
            routes.add_products(owners, bundles);
        }
        if routes.routes(&Seam::Terrain)
            && (routes.routes(&Seam::Fill) || routes.routes(&Seam::Surface))
        {
            log::info!(
                "Horizon: Java runs the whole terrain stage, so its noise fill and surface seams are part of it"
            );
        }
        routes
    }

    /// The method half: seams whose vanilla closure holds a method a loaded mod patches, typed seams spread over their type's entries.
    fn add_methods(&mut self, routed: &BTreeMap<MethodSeam, Vec<String>>, bundles: &Bundles<'_>) {
        for (seam, keys) in routed {
            let keys: BTreeSet<String> = keys.iter().cloned().collect();
            match seam {
                MethodSeam::Biomes => self.add(Seam::Biomes, keys),
                MethodSeam::Fill => self.add(Seam::Fill, keys),
                MethodSeam::Surface => self.add(Seam::Surface, keys),
                MethodSeam::Terrain => self.add(Seam::Terrain, keys),
                MethodSeam::Beard => self.add(Seam::Beard, keys),
                MethodSeam::Spawn => self.add(Seam::Spawn, keys),
                MethodSeam::Carver(ty) => {
                    for id in of_type(bundles, CARVERS, ty) {
                        self.add(Seam::Carver(id), keys.clone());
                    }
                }
                MethodSeam::Feature(ty) => {
                    for id in ids(bundles, PLACED) {
                        if placed_type(bundles, &id).as_deref() == Some(ty.as_str()) {
                            self.add(Seam::Feature(id), keys.clone());
                        }
                    }
                }
                MethodSeam::Start(ty) => {
                    for id in of_type(bundles, STRUCTURES, ty) {
                        self.add(Seam::Start(id), keys.clone());
                    }
                }
                MethodSeam::Place(ty) => {
                    for id in of_type(bundles, STRUCTURES, ty) {
                        self.add(Seam::Place(id), keys.clone());
                    }
                }
            }
        }
    }

    /// The product half: seams whose entries hold a mod-owned type.
    fn add_products(&mut self, owners: &ExportOwners, bundles: &Bundles<'_>) {
        let mut reach = Reach {
            owners,
            bundles,
            done: BTreeMap::new(),
            active: BTreeSet::new(),
        };
        for id in ids(bundles, PLACED) {
            let types = reach.types(PLACED, &id);
            self.add(Seam::Feature(id), types);
        }
        let mut structures: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for id in ids(bundles, STRUCTURES) {
            structures.insert(id.clone(), reach.types(STRUCTURES, &id));
        }
        // A set whose placement a mod owns routes every structure in it.
        for id in ids(bundles, SETS) {
            let own = owners.mod_types(SETS, &id);
            if own.is_empty() {
                continue;
            }
            let Some(set) = parse(bundles, SETS, &id) else {
                continue;
            };
            for entry in set["structures"].as_array().into_iter().flatten() {
                if let Some(structure) = entry["structure"].as_str() {
                    let types = structures.entry(normalize(structure)).or_default();
                    types.extend(own.iter().cloned());
                }
            }
        }
        for (id, types) in structures {
            self.add(Seam::Start(id.clone()), types.clone());
            self.add(Seam::Place(id), types);
        }
        for (bundle, id, types) in owners.owned() {
            if bundle == CARVERS {
                self.add(Seam::Carver(id.to_owned()), types.iter().cloned().collect());
            }
        }
        for id in ids(bundles, DIMENSIONS) {
            self.add_stem(&mut reach, &id);
        }
        for id in ids(bundles, SETTINGS) {
            let types = reach.types(SETTINGS, &id);
            self.add_typed(types);
        }
        if self.routes(&Seam::Surface) {
            for bundle in [RULES, CONDITIONS] {
                let skipped: BTreeSet<String> = ids(bundles, bundle)
                    .into_iter()
                    .filter(|id| !reach.types(bundle, id).is_empty())
                    .collect();
                if !skipped.is_empty() {
                    self.skipped.insert(bundle, skipped);
                }
            }
        }
    }

    /// A level stem: its biome source, generator and noise settings.
    fn add_stem(&mut self, reach: &mut Reach<'_>, id: &str) {
        let own = reach.owners.mod_types(DIMENSIONS, id);
        let mut biomes: Vec<String> = Vec::new();
        for ty in own {
            if ty.contains("worldgen/chunk_generator:") {
                self.add(Seam::Terrain, BTreeSet::from([ty.clone()]));
                biomes.push(ty.clone());
            } else if ty.contains("worldgen/biome_source:") || ty.starts_with("unregistered:") {
                biomes.push(ty.clone());
            }
        }
        let stem = parse(reach.bundles, DIMENSIONS, id);
        let generator = stem.as_ref().map(|stem| &stem["generator"]);
        // A multi-noise parameter list only a mod registers.
        if let Some(preset) = generator.and_then(|g| g["biome_source"]["preset"].as_str())
            && !normalize(preset).starts_with("minecraft:")
        {
            biomes.push(format!(
                "worldgen/multi_noise_biome_source_parameter_list:{preset}"
            ));
        }
        self.add(Seam::Biomes, biomes.into_iter().collect());
        self.add_typed(own.iter().cloned().collect());
        if let Some(settings) = generator.and_then(|g| g["settings"].as_str()) {
            let types = reach.types(SETTINGS, &normalize(settings));
            self.add_typed(types);
        }
    }

    /// Types sorted into the seams their registries feed.
    fn add_typed(&mut self, types: BTreeSet<String>) {
        for ty in types {
            for seam in type_seams(&ty) {
                self.add(seam.clone(), BTreeSet::from([ty.clone()]));
            }
        }
    }

    fn add(&mut self, seam: Seam, types: BTreeSet<String>) {
        if types.is_empty() {
            return;
        }
        let why = self.why.entry(seam.clone()).or_default();
        why.extend(types);
        why.sort();
        why.dedup();
        self.java.insert(seam);
    }

    /// Whether Java runs `seam` for this world.
    pub fn routes(&self, seam: &Seam) -> bool {
        self.java.contains(seam)
    }

    /// Whether Java runs nothing.
    pub fn is_empty(&self) -> bool {
        self.java.is_empty()
    }

    /// The routed seams, in order.
    pub fn seams(&self) -> impl Iterator<Item = &Seam> {
        self.java.iter()
    }

    /// The mod-owned types and patched methods behind `seam`.
    pub fn why(&self, seam: &Seam) -> &[String] {
        self.why.get(seam).map_or(&[], Vec::as_slice)
    }

    /// `entries` without those a routed seam owns, which native doesn't decode.
    pub fn decodable(
        &self,
        bundle: &str,
        entries: &BTreeMap<String, String>,
    ) -> BTreeMap<String, String> {
        match self.skipped.get(bundle) {
            Some(skipped) => entries
                .iter()
                .filter(|(id, _)| !skipped.contains(*id))
                .map(|(id, json)| (id.clone(), json.clone()))
                .collect(),
            None => entries.clone(),
        }
    }

    /// Whether native leaves `id` of `bundle` undecoded for a routed seam.
    pub fn skips(&self, bundle: &str, id: &str) -> bool {
        self.skipped
            .get(bundle)
            .is_some_and(|skipped| skipped.contains(id))
    }

    /// The error a chunk fails with while `seam`'s Java side isn't built; logged once per seam.
    pub fn needs_java(&self, seam: &Seam) -> GeneratorError {
        let message = format!("{seam} needs Java worldgen (W10 {})", seam.spec());
        let first = self
            .logged
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(seam.clone());
        if first {
            log::error!("Horizon: chunks fail generation: {message}");
        }
        GeneratorError::Stage(seam.stage(), message)
    }

    /// The routed seams for the log: whole stages by name, the rest counted with a few ids.
    pub fn summary(&self) -> String {
        if self.java.is_empty() {
            return "nothing".to_owned();
        }
        const NAMED: usize = 5;
        let mut parts = Vec::new();
        let mut groups: BTreeMap<&'static str, Vec<&str>> = BTreeMap::new();
        for seam in &self.java {
            match seam {
                Seam::Carver(id) => groups.entry("carvers").or_default().push(id),
                Seam::Feature(id) => groups.entry("features").or_default().push(id),
                Seam::Start(id) => groups.entry("structure starts").or_default().push(id),
                Seam::Place(id) => groups.entry("structure placement").or_default().push(id),
                whole => parts.push(format!("{whole} ({})", self.why(whole).join(", "))),
            }
        }
        for (kind, ids) in groups {
            let mut named = ids
                .iter()
                .take(NAMED)
                .copied()
                .collect::<Vec<_>>()
                .join(", ");
            if ids.len() > NAMED {
                named.push_str(&format!(" and {} more", ids.len() - NAMED));
            }
            parts.push(format!("{} {kind} ({named})", ids.len()));
        }
        parts.join("; ")
    }
}

/// `ns:path` for an id that may leave out `minecraft:`.
fn normalize(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

fn ids(bundles: &Bundles<'_>, bundle: &str) -> Vec<String> {
    bundles
        .get(bundle)
        .map_or_else(Vec::new, |entries| entries.keys().cloned().collect())
}

fn parse(bundles: &Bundles<'_>, bundle: &str, id: &str) -> Option<Value> {
    serde_json::from_str(bundles.get(bundle)?.get(id)?).ok()
}

fn type_of(entry: &Value) -> Option<String> {
    entry["type"].as_str().map(normalize)
}

/// The entries of `bundle` whose `type` is `ty`.
fn of_type(bundles: &Bundles<'_>, bundle: &str, ty: &str) -> Vec<String> {
    ids(bundles, bundle)
        .into_iter()
        .filter(|id| {
            parse(bundles, bundle, id)
                .and_then(|e| type_of(&e))
                .as_deref()
                == Some(ty)
        })
        .collect()
}

/// A placed feature's feature type: its configured feature's, named or inline.
fn placed_type(bundles: &Bundles<'_>, id: &str) -> Option<String> {
    let placed = parse(bundles, PLACED, id)?;
    match &placed["feature"] {
        Value::String(name) => type_of(&parse(bundles, FEATURES, &normalize(name))?),
        inline => type_of(inline),
    }
}

/// The mod-owned types each entry holds or reaches through the entries it names.
struct Reach<'a> {
    owners: &'a ExportOwners,
    bundles: &'a Bundles<'a>,
    done: BTreeMap<(&'static str, String), BTreeSet<String>>,
    active: BTreeSet<(&'static str, String)>,
}

impl Reach<'_> {
    fn types(&mut self, bundle: &'static str, id: &str) -> BTreeSet<String> {
        let key = (bundle, id.to_owned());
        if let Some(done) = self.done.get(&key) {
            return done.clone();
        }
        if !self.active.insert(key.clone()) {
            return BTreeSet::new();
        }
        let mut types: BTreeSet<String> =
            self.owners.mod_types(bundle, id).iter().cloned().collect();
        if let Some(json) = parse(self.bundles, bundle, id) {
            let mut names = Vec::new();
            strings(&json, &mut names);
            for name in names {
                let name = normalize(name.strip_prefix('#').unwrap_or(&name));
                for &next in follows(bundle) {
                    let named = self
                        .bundles
                        .get(next)
                        .is_some_and(|entries| entries.contains_key(&name));
                    if named {
                        types.extend(self.types(next, &name));
                    }
                }
            }
        }
        self.active.remove(&key);
        self.done.insert(key, types.clone());
        types
    }
}

/// Every string in a JSON tree, the ids an entry names among them.
fn strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(s.clone()),
        Value::Array(items) => items.iter().for_each(|v| strings(v, out)),
        Value::Object(fields) => fields.values().for_each(|v| strings(v, out)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::overrides::WorldgenOverrides;
    use super::*;

    fn bundle(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(id, json)| (id.to_string(), json.to_string()))
            .collect()
    }

    #[test]
    fn owned_entries_and_what_names_them_route_their_seams() {
        let placed = bundle(&[
            (
                "bop:flowers",
                r#"{"feature":"bop:flower_patch","placement":[]}"#,
            ),
            (
                "minecraft:trees",
                r#"{"feature":"minecraft:oak","placement":[]}"#,
            ),
        ]);
        let features = bundle(&[
            ("bop:flower_patch", r#"{"type":"bop:patch","config":{}}"#),
            ("minecraft:oak", r#"{"type":"minecraft:tree","config":{}}"#),
        ]);
        let carvers = bundle(&[("mod:canyon", r#"{"type":"mod:canyon"}"#)]);
        let mut owners = ExportOwners::default();
        owners.set_mod_types(
            FEATURES,
            "bop:flower_patch",
            vec!["minecraft:worldgen/feature:bop:patch".into()],
        );
        owners.set_mod_types(
            CARVERS,
            "mod:canyon",
            vec!["minecraft:worldgen/carver:mod:canyon".into()],
        );
        let bundles = Bundles::from([
            (PLACED, &placed),
            (FEATURES, &features),
            (CARVERS, &carvers),
        ]);
        let routes = WorldgenRoutes::from_export(&owners, &bundles);
        assert!(routes.routes(&Seam::Feature("bop:flowers".into())));
        assert!(!routes.routes(&Seam::Feature("minecraft:trees".into())));
        assert!(routes.routes(&Seam::Carver("mod:canyon".into())));
        assert!(!routes.routes(&Seam::Surface));
        assert_eq!(
            routes.why(&Seam::Feature("bop:flowers".into())),
            ["minecraft:worldgen/feature:bop:patch"]
        );
    }

    #[test]
    fn a_structure_whose_pool_has_a_mod_processor_routes_start_and_place() {
        let structures = bundle(&[
            (
                "ae2:meteor",
                r#"{"type":"minecraft:jigsaw","start_pool":"ae2:meteor/start"}"#,
            ),
            ("minecraft:igloo", r#"{"type":"minecraft:igloo"}"#),
        ]);
        let pools = bundle(&[(
            "ae2:meteor/start",
            r#"{"elements":[{"weight":1,"element":{"element_type":"minecraft:single_pool_element","location":"ae2:meteor","processors":"ae2:crater"}}],"fallback":"minecraft:empty"}"#,
        )]);
        let processors = bundle(&[(
            "ae2:crater",
            r#"{"processors":[{"processor_type":"ae2:crater"}]}"#,
        )]);
        let mut owners = ExportOwners::default();
        owners.set_mod_types(
            PROCESSORS,
            "ae2:crater",
            vec!["minecraft:worldgen/structure_processor:ae2:crater".into()],
        );
        let bundles = Bundles::from([
            (STRUCTURES, &structures),
            (POOLS, &pools),
            (PROCESSORS, &processors),
        ]);
        let routes = WorldgenRoutes::from_export(&owners, &bundles);
        assert!(routes.routes(&Seam::Start("ae2:meteor".into())));
        assert!(routes.routes(&Seam::Place("ae2:meteor".into())));
        assert!(!routes.routes(&Seam::Start("minecraft:igloo".into())));
    }

    #[test]
    fn a_mod_set_placement_routes_every_structure_of_the_set() {
        let structures = bundle(&[
            ("mod:tower", r#"{"type":"minecraft:jigsaw"}"#),
            ("mod:hut", r#"{"type":"minecraft:jigsaw"}"#),
        ]);
        let sets = bundle(&[(
            "mod:buildings",
            r#"{"structures":[{"structure":"mod:tower","weight":1},{"structure":"mod:hut","weight":1}],"placement":{"type":"mod:grid"}}"#,
        )]);
        let mut owners = ExportOwners::default();
        owners.set_mod_types(
            SETS,
            "mod:buildings",
            vec!["minecraft:worldgen/structure_placement:mod:grid".into()],
        );
        let bundles = Bundles::from([(STRUCTURES, &structures), (SETS, &sets)]);
        let routes = WorldgenRoutes::from_export(&owners, &bundles);
        for id in ["mod:tower", "mod:hut"] {
            assert!(routes.routes(&Seam::Start(id.into())));
        }
    }

    #[test]
    fn a_mod_material_rule_routes_the_surface_and_skips_its_rules() {
        let settings = bundle(&[(
            "minecraft:overworld",
            r#"{"material_rule":"minecraft:overworld","noise_router":{}}"#,
        )]);
        let rules = bundle(&[
            (
                "minecraft:overworld",
                r#"{"type":"minecraft:sequence","sequence":["tb:regions"]}"#,
            ),
            ("tb:regions", r#"{"type":"tb:namespaced"}"#),
            ("minecraft:nether", r#"{"type":"minecraft:block"}"#),
        ]);
        let mut owners = ExportOwners::default();
        owners.set_mod_types(
            RULES,
            "tb:regions",
            vec!["minecraft:worldgen/material_rule:tb:namespaced".into()],
        );
        let bundles = Bundles::from([(SETTINGS, &settings), (RULES, &rules)]);
        let routes = WorldgenRoutes::from_export(&owners, &bundles);
        assert!(routes.routes(&Seam::Surface));
        assert!(!routes.routes(&Seam::Fill));
        let left = routes.decodable(RULES, &rules);
        assert_eq!(left.keys().collect::<Vec<_>>(), ["minecraft:nether"]);
        assert!(routes.summary().starts_with("surface ("));
    }

    #[test]
    fn a_mod_preset_routes_the_biomes() {
        let dimensions = bundle(&[(
            "minecraft:overworld",
            r#"{"type":"minecraft:overworld","generator":{"type":"minecraft:noise","settings":"minecraft:overworld","biome_source":{"type":"minecraft:multi_noise","preset":"mod:regions"}}}"#,
        )]);
        let mut owners = ExportOwners::default();
        owners.set_mod_types(DIMENSIONS, "other:place", vec!["unregistered:x.Y".into()]);
        let bundles = Bundles::from([(DIMENSIONS, &dimensions)]);
        let routes = WorldgenRoutes::from_export(&owners, &bundles);
        assert!(routes.routes(&Seam::Biomes));
        assert_eq!(routes.seams().count(), 1);
    }

    #[test]
    fn a_routed_block_turns_typed_seams_into_id_seams() {
        let export = r#"{"format":1,"entries":{},"tags":{},"owners":{},"warnings":[],"routed":{
            "start:minecraft:jigsaw":["net/minecraft/world/level/levelgen/structure/pools/JigsawPlacement$Placer#tryPlacingChildren()V"],
            "place:mineshaft":["net/minecraft/world/level/levelgen/structure/StructurePiece#placeBlock()V"],
            "feature:minecraft:tree":["net/minecraft/world/level/levelgen/feature/TreeFeature#place()Z"],
            "terrain":["net/minecraft/world/level/levelgen/NoiseBasedChunkGenerator#buildTerrain()V"]}}"#;
        let (_, owners) = WorldgenOverrides::from_java_export(export).unwrap();
        let structures = bundle(&[
            ("minecraft:village_plains", r#"{"type":"minecraft:jigsaw"}"#),
            ("mod:tower", r#"{"type":"jigsaw"}"#),
            ("minecraft:mineshaft", r#"{"type":"minecraft:mineshaft"}"#),
        ]);
        let placed = bundle(&[
            (
                "minecraft:oak",
                r#"{"feature":"minecraft:oak","placement":[]}"#,
            ),
            (
                "mod:inline_tree",
                r#"{"feature":{"type":"minecraft:tree","config":{}},"placement":[]}"#,
            ),
            (
                "minecraft:ore_iron",
                r#"{"feature":"minecraft:ore_iron","placement":[]}"#,
            ),
        ]);
        let features = bundle(&[
            ("minecraft:oak", r#"{"type":"minecraft:tree","config":{}}"#),
            (
                "minecraft:ore_iron",
                r#"{"type":"minecraft:ore","config":{}}"#,
            ),
        ]);
        let bundles = Bundles::from([
            (STRUCTURES, &structures),
            (PLACED, &placed),
            (FEATURES, &features),
        ]);
        let routes = WorldgenRoutes::from_export(&owners, &bundles);
        let seams: Vec<&Seam> = routes.seams().collect();
        assert_eq!(
            seams,
            [
                &Seam::Terrain,
                &Seam::Feature("minecraft:oak".into()),
                &Seam::Feature("mod:inline_tree".into()),
                &Seam::Start("minecraft:village_plains".into()),
                &Seam::Start("mod:tower".into()),
                &Seam::Place("minecraft:mineshaft".into()),
            ]
        );
        assert_eq!(
            routes.why(&Seam::Place("minecraft:mineshaft".into())),
            ["net/minecraft/world/level/levelgen/structure/StructurePiece#placeBlock()V"]
        );
    }

    #[test]
    fn an_unknown_routed_seam_is_a_warning() {
        let export = r#"{"format":1,"entries":{},"tags":{},"owners":{},"warnings":[],"routed":{"light":["a#b()V"],"start:":["a#c()V"]}}"#;
        let (_, owners) = WorldgenOverrides::from_java_export(export).unwrap();
        assert!(owners.routed().is_empty());
        assert_eq!(owners.warnings.len(), 2);
    }

    #[test]
    fn an_export_without_mod_types_routes_nothing() {
        let placed = bundle(&[("minecraft:trees", r#"{"feature":"minecraft:oak"}"#)]);
        let bundles = Bundles::from([(PLACED, &placed)]);
        let routes = WorldgenRoutes::from_export(&ExportOwners::default(), &bundles);
        assert!(routes.is_empty());
        assert_eq!(routes.summary(), "nothing");
    }
}
