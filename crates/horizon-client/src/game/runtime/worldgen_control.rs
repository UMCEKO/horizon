//! `worldgen biomes|starts|chunks [JSON]`: the production generator's output, for tools/worldgen_compare.py.
use super::Game;
use horizon_client::Result;
use serde::Deserialize;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub(super) const HELP: &[&str] = &[
    "worldgen biomes|starts|chunks [JSON]  the overworld generator's biomes, structure starts or chunk hashes (tools/worldgen_compare.py)",
];

const BIOME_Y: [i32; 3] = [-32, 64, 160];
const FULL_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
struct Request {
    /// Side of the biome square, in blocks.
    square: i32,
    /// Chunks between two biome samples.
    step: i32,
    /// Side of the structure start square, in chunks.
    starts: i32,
    /// Side of the force-loaded square, in chunks.
    chunks: i32,
}
impl Default for Request {
    fn default() -> Self {
        Self {
            square: 2048,
            step: 4,
            starts: 64,
            chunks: 5,
        }
    }
}
impl Request {
    fn parse(text: &str) -> Result<Self> {
        let request: Self = if text.trim().is_empty() {
            Self::default()
        } else {
            serde_json::from_str(text)?
        };
        if request.square < 16 || request.step < 1 || request.starts < 1 || request.chunks < 3 {
            return Err("worldgen: square >= 16, step >= 1, starts >= 1 and chunks >= 3".into());
        }
        Ok(request)
    }
}

/// The first chunk of a square of `side` chunks centred on 0,0.
fn low(side: i32) -> i32 {
    -(side / 2)
}

/// The biome sample points: block x, y, z of each chunk's first quart column.
fn biome_points(request: Request) -> Vec<[i32; 3]> {
    let side = request.square / 16;
    let lo = low(side);
    let mut points = Vec::new();
    for cz in (lo..lo + side).step_by(request.step as usize) {
        for cx in (lo..lo + side).step_by(request.step as usize) {
            for y in BIOME_Y {
                points.push([cx * 16, y, cz * 16]);
            }
        }
    }
    points
}

fn box_json(b: horizon_core::bounding_box::BoundingBox) -> Value {
    json!([b.min[0], b.min[1], b.min[2], b.max[0], b.max[1], b.max[2]])
}

impl Game<'_> {
    pub(super) fn worldgen_control(&mut self, words: &[&str], line: &str) -> Result<Option<Value>> {
        if words.first().copied() != Some("worldgen") {
            return Ok(None);
        }
        self.require_world()?;
        let part = words.get(1).copied().ok_or("worldgen biomes|starts|chunks [JSON]")?;
        let rest = line
            .trim_start()
            .strip_prefix("worldgen")
            .and_then(|r| r.trim_start().strip_prefix(part))
            .unwrap_or("");
        let request = Request::parse(rest)?;
        let Some(server) = &self.integrated else {
            return Err("worldgen needs the single player server thread".into());
        };
        let value = match part {
            "biomes" => server
                .call(move |g| g.in_owner_level(|g| g.worldgen_biomes(request).map_err(|e| e.to_string())))
                .ok_or(super::integrated::NO_ANSWER)??,
            "starts" => server
                .call(move |g| g.in_owner_level(|g| g.worldgen_starts(request).map_err(|e| e.to_string())))
                .ok_or(super::integrated::NO_ANSWER)??,
            "chunks" => {
                let lo = low(request.chunks);
                let (from, to) = (lo * 16, (lo + request.chunks) * 16);
                let command = format!("forceload add {from} {from} {to} {to}");
                // A busy server thread can miss a call's window; the closure still runs there.
                let _ = server.call(move |g| g.in_owner_level(|g| g.run_command_quietly(&command)));
                let began = Instant::now();
                loop {
                    let missing = server.call(move |g| g.in_owner_level(|g| g.worldgen_missing(request)));
                    if missing == Some(0) {
                        break;
                    }
                    if began.elapsed() > FULL_TIMEOUT {
                        return Err(format!("worldgen: chunks still not full ({missing:?} missing)").into());
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
                loop {
                    let hashes = server.call(move |g| {
                        g.in_owner_level(|g| g.worldgen_chunk_hashes().map_err(|e| e.to_string()))
                    });
                    match hashes {
                        Some(done) => break done?,
                        None if began.elapsed() > FULL_TIMEOUT => {
                            return Err(super::integrated::NO_ANSWER.into());
                        }
                        None => {}
                    }
                }
            }
            _ => return Err("worldgen biomes|starts|chunks [JSON]".into()),
        };
        Ok(Some(value))
    }

    fn worldgen_biomes(&self, request: Request) -> Result<Value> {
        let terrain = self.end.locator().ok_or("worldgen: no generator in this level")?;
        let points = biome_points(request);
        let quarts: Vec<[i32; 3]> = points
            .iter()
            .map(|p| [p[0] >> 2, p[1] >> 2, p[2] >> 2])
            .collect();
        let biomes = terrain.noise_biomes(&quarts);
        let rows: Vec<Value> = points
            .iter()
            .zip(biomes)
            .map(|(p, b)| json!([p[0], p[1], p[2], b.unwrap_or_else(|| "?".into())]))
            .collect();
        Ok(Value::Array(rows))
    }

    fn worldgen_starts(&self, request: Request) -> Result<Value> {
        use horizon_core::nbt::Tag;
        let terrain = self.end.locator().ok_or("worldgen: no generator in this level")?;
        let lo = low(request.starts);
        let mut found: Vec<Value> = Vec::new();
        for cz in lo..lo + request.starts {
            for cx in lo..lo + request.starts {
                for start in terrain.starts_at(cx, cz) {
                    let pieces: Vec<Value> = start
                        .pieces
                        .iter()
                        .map(|p| {
                            let id = match p.save().as_ref().and_then(|c| c.get(&"id".into())) {
                                Some(Tag::String(id)) => id.to_string_lossy(),
                                _ => "?".to_owned(),
                            };
                            json!([id, box_json(p.bounding_box())])
                        })
                        .collect();
                    found.push(json!({
                        "structure": start.name,
                        "chunk": [start.chunk.x, start.chunk.z],
                        "box": box_json(start.bounding_box),
                        "pieces": pieces,
                    }));
                }
            }
        }
        found.sort_by_key(|s| (s["structure"].to_string(), s["chunk"].to_string()));
        Ok(Value::Array(found))
    }

    /// How many chunks of the force-loaded square are not in the world yet.
    fn worldgen_missing(&self, request: Request) -> usize {
        let lo = low(request.chunks);
        (lo..lo + request.chunks)
            .flat_map(|cz| (lo..lo + request.chunks).map(move |cx| (cx, cz)))
            .filter(|key| self.world.chunks.get(key).is_none())
            .count()
    }

    /// The centre chunk and its 8 neighbours: section hashes, heightmaps, block entities.
    fn worldgen_chunk_hashes(&self) -> Result<Value> {
        let registry = &self.pack.assets.registry;
        let facts = self.res.facts();
        let mut text: HashMap<u32, String> = HashMap::new();
        let mut kinds: HashMap<u32, [bool; 4]> = HashMap::new();
        let mut entity_types: HashMap<u32, Option<&'static str>> = HashMap::new();
        let mut out = Vec::new();
        for cz in -1..=1 {
            for cx in -1..=1 {
                let chunk = self
                    .world
                    .chunks
                    .get(&(cx, cz))
                    .ok_or_else(|| format!("worldgen: chunk {cx} {cz} is not loaded"))?;
                let min_y = chunk.min_y();
                let mut unpacked: Vec<Box<[u32; 4096]>> = Vec::new();
                let mut sections = Vec::new();
                for section in &chunk.sections {
                    let mut values = Box::new([0u32; 4096]);
                    section.unpack(&mut values);
                    let mut sha = Sha1::new();
                    for &v in values.iter() {
                        let state = text.entry(v).or_insert_with(|| {
                            let Some(s) = registry.state(horizon_world::registry::BlockStateId(v)) else {
                                return "?\n".to_owned();
                            };
                            let mut props: Vec<(&String, &String)> = s.properties.iter().collect();
                            props.sort();
                            if props.is_empty() {
                                format!("{}\n", s.name)
                            } else {
                                let props: Vec<String> =
                                    props.iter().map(|(k, v)| format!("{k}={v}")).collect();
                                format!("{}[{}]\n", s.name, props.join(","))
                            }
                        });
                        sha.update(state.as_bytes());
                    }
                    sections.push(sha.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>());
                    unpacked.push(values);
                }
                let mut maps = [[min_y; 256]; 4];
                for (m, map) in maps.iter_mut().enumerate() {
                    for (column, height) in map.iter_mut().enumerate() {
                        'down: for (s, values) in unpacked.iter().enumerate().rev() {
                            for dy in (0..16).rev() {
                                let v = values[dy << 8 | column];
                                let flags = kinds.entry(v).or_insert_with(|| {
                                    let id = horizon_world::registry::BlockStateId(v);
                                    let f = facts.get(id);
                                    let air = id == self.world.air;
                                    let leaves = registry
                                        .state(id)
                                        .is_some_and(|s| s.name.ends_with("_leaves"));
                                    let fluid = f.fluid != horizon_client::blocks::Fluid::None;
                                    let blocking = f.legacy_solid || fluid;
                                    [!air, f.legacy_solid, blocking, blocking && !leaves]
                                });
                                if flags[m] {
                                    *height = min_y + s as i32 * 16 + dy as i32 + 1;
                                    break 'down;
                                }
                            }
                        }
                    }
                }
                let mut entities: Vec<(i32, i32, i32, &str)> = Vec::new();
                for (s, values) in unpacked.iter().enumerate() {
                    for (i, &v) in values.iter().enumerate() {
                        let kind = *entity_types.entry(v).or_insert_with(|| {
                            let name = registry.state(horizon_world::registry::BlockStateId(v))?;
                            horizon_client::lan::chunks::block_entity_type(&name.name)
                        });
                        let Some(kind) = kind else { continue };
                        let pos = (
                            cx * 16 + (i & 15) as i32,
                            min_y + s as i32 * 16 + (i >> 8) as i32,
                            cz * 16 + ((i >> 4) & 15) as i32,
                        );
                        if self.world.block_entities.get(&pos).is_some() {
                            entities.push((pos.0, pos.1, pos.2, kind));
                        }
                    }
                }
                entities.sort();
                out.push(json!({
                    "x": cx,
                    "z": cz,
                    "sections": sections,
                    "heightmaps": {
                        "WORLD_SURFACE": maps[0].to_vec(),
                        "OCEAN_FLOOR": maps[1].to_vec(),
                        "MOTION_BLOCKING": maps[2].to_vec(),
                        "MOTION_BLOCKING_NO_LEAVES": maps[3].to_vec(),
                    },
                    "block_entities": entities.iter().map(|e| json!([e.0, e.1, e.2, e.3])).collect::<Vec<_>>(),
                }));
            }
        }
        Ok(Value::Array(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_request_is_the_defaults() {
        assert_eq!(Request::parse("  ").unwrap(), Request::default());
    }

    #[test]
    fn request_overrides_and_rejects_unknown_fields() {
        let r = Request::parse(r#"{"starts": 16, "chunks": 3}"#).unwrap();
        assert_eq!((r.starts, r.chunks, r.square), (16, 3, 2048));
        assert!(Request::parse(r#"{"bogus": 1}"#).is_err());
        assert!(Request::parse(r#"{"chunks": 1}"#).is_err());
    }

    #[test]
    fn biome_grid_is_centred_on_the_origin_every_step() {
        let points = biome_points(Request {
            square: 128,
            step: 4,
            ..Request::default()
        });
        // 8 chunks a side, every 4th: chunks -4 and 0 on each axis, three heights each.
        assert_eq!(points.len(), 2 * 2 * 3);
        assert_eq!(points[0], [-64, -32, -64]);
        assert_eq!(points[3], [0, -32, -64]);
        assert_eq!(points[11], [0, 160, 0]);
    }
}
