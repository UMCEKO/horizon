//! The entity analysis of the installed mods, confirmed at boot (`entities.bin`, `entity-types.bin`): which entity methods route to Java, which events fire for which entities.
use std::collections::HashMap;
use std::sync::OnceLock;

pub const VERSION: u32 = 2;
/// Bit i of a reads mask is `GROUPS[i]`.
pub const GROUPS: [&str; 16] = [
    "identity",
    "position",
    "motion",
    "health",
    "effects",
    "equipment",
    "fire",
    "air",
    "flags",
    "name",
    "item",
    "riding",
    "removal",
    "species",
    "attributes",
    "ai",
];
const IDENTITY: u32 = 1;

/// NeoForge's hooks the entity loop calls, in the analysis's `HOOKS` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hook {
    EntityTickPre,
    EntityTickPost,
    MobDespawn,
    LivingBreathe,
    StartTracking,
    StopTracking,
}

const HOOK_NAMES: [(&str, Hook); 6] = [
    ("entity_tick_pre", Hook::EntityTickPre),
    ("entity_tick_post", Hook::EntityTickPost),
    ("mob_despawn", Hook::MobDespawn),
    ("living_breathe", Hook::LivingBreathe),
    ("start_tracking", Hook::StartTracking),
    ("stop_tracking", Hook::StopTracking),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MethodId(u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EventId(u32);

/// A mixin handler merged into a vanilla method: `exit` is false for HEAD, true for RETURN and TAIL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Handler {
    pub name: String,
    pub exit: bool,
    pub cancellable: bool,
}

/// What a loaded mod changes in one vanilla method of one entity type.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Route {
    pub edge: Vec<Handler>,
    pub body: bool,
    pub overridden: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Both,
    Server,
    Client,
}

#[derive(Debug, Default)]
struct Filter {
    side: Option<Side>,
    require: Vec<String>,
    exclude: Vec<String>,
    types: Vec<String>,
}

#[derive(Debug)]
struct Event {
    all: bool,
    filters: Vec<Filter>,
}

#[derive(Debug, Default)]
struct Class {
    reads: u32,
    edge: Vec<(u32, Vec<Handler>)>,
    body: Vec<u32>,
}

#[derive(Debug, Default)]
struct HookFacts {
    reads: u32,
    events: Vec<usize>,
}

#[derive(Debug, Default)]
struct TypeFacts {
    reads: u32,
    living: bool,
    item: bool,
    hook_server: u8,
    hook_client: u8,
    routes: HashMap<MethodId, Route>,
    server: Vec<u64>,
    client: Vec<u64>,
}

#[derive(Debug)]
pub struct EntityFacts {
    methods: Vec<String>,
    method_ids: HashMap<String, MethodId>,
    classes: HashMap<String, Class>,
    overrides: HashMap<String, Vec<u32>>,
    events: Vec<Event>,
    event_ids: HashMap<String, EventId>,
    caps: HashMap<String, Vec<String>>,
    attachments: Vec<String>,
    hooks: [Option<HookFacts>; 6],
    types: OnceLock<HashMap<String, TypeFacts>>,
    runtime_caps: OnceLock<HashMap<String, Vec<String>>>,
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        if self.0.len() < n {
            return Err("the file ended early".into());
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }

    fn int(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn count(&mut self) -> Result<usize, String> {
        let n = self.int()? as usize;
        if n > self.0.len() {
            return Err("a count runs past the file".into());
        }
        Ok(n)
    }

    fn text(&mut self) -> Result<String, String> {
        let n = self.count()?;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|e| e.to_string())
    }

    fn texts(&mut self) -> Result<Vec<String>, String> {
        let n = self.count()?;
        (0..n).map(|_| self.text()).collect()
    }
}

impl EntityFacts {
    /// Reads `entities.bin`.
    pub fn read(bytes: &[u8]) -> Result<Self, String> {
        let mut r = Reader(bytes);
        match r.int()? {
            VERSION => {}
            1 => return Err("re-run the analysis (entity facts format 2)".into()),
            _ => return Err("entities.bin has another version".into()),
        }
        let methods = r.texts()?;
        let method_ids = methods
            .iter()
            .enumerate()
            .map(|(i, m)| (m.clone(), MethodId(i as u32)))
            .collect();
        let mut classes = HashMap::new();
        for _ in 0..r.count()? {
            let name = r.text()?;
            let mut class = Class {
                reads: r.int()?,
                ..Class::default()
            };
            for _ in 0..r.count()? {
                let method = r.int()?;
                let mut handlers = Vec::new();
                for _ in 0..r.count()? {
                    handlers.push(Handler {
                        name: r.text()?,
                        exit: r.byte()? != 0,
                        cancellable: r.byte()? != 0,
                    });
                }
                class.edge.push((method, handlers));
            }
            for _ in 0..r.count()? {
                class.body.push(r.int()?);
            }
            classes.insert(name, class);
        }
        let mut overrides = HashMap::new();
        for _ in 0..r.count()? {
            let name = r.text()?;
            let mut list = Vec::new();
            for _ in 0..r.count()? {
                list.push(r.int()?);
            }
            overrides.insert(name, list);
        }
        let mut events = Vec::new();
        let mut event_ids = HashMap::new();
        for i in 0..r.count()? {
            event_ids.insert(r.text()?, EventId(i as u32));
            let all = r.byte()? != 0;
            let mut filters = Vec::new();
            for _ in 0..r.count()? {
                let side = match r.byte()? {
                    1 => Side::Server,
                    2 => Side::Client,
                    _ => Side::Both,
                };
                filters.push(Filter {
                    side: Some(side),
                    require: r.texts()?,
                    exclude: r.texts()?,
                    types: r.texts()?,
                });
            }
            events.push(Event { all, filters });
        }
        let mut caps = HashMap::new();
        for _ in 0..r.count()? {
            caps.insert(r.text()?, r.texts()?);
        }
        let attachments = r.texts()?;
        let mut hooks: [Option<HookFacts>; 6] = Default::default();
        for _ in 0..r.count()? {
            let name = r.text()?;
            let mut hook = HookFacts {
                reads: r.int()?,
                events: Vec::new(),
            };
            for _ in 0..r.count()? {
                let at = r.int()? as usize;
                if at >= events.len() {
                    return Err("entities.bin names an event it doesn't list".into());
                }
                hook.events.push(at);
            }
            if let Some((_, which)) = HOOK_NAMES.iter().find(|(n, _)| *n == name) {
                hooks[*which as usize] = Some(hook);
            }
        }
        if methods.iter().any(|m| m.is_empty())
            || classes
                .values()
                .flat_map(|c| c.edge.iter().map(|e| e.0).chain(c.body.iter().copied()))
                .chain(overrides.values().flatten().copied())
                .any(|m| m as usize >= methods.len())
        {
            return Err("entities.bin names a method it doesn't list".into());
        }
        Ok(Self {
            methods,
            method_ids,
            classes,
            overrides,
            events,
            event_ids,
            caps,
            attachments,
            hooks,
            types: OnceLock::new(),
            runtime_caps: OnceLock::new(),
        })
    }

    /// The id of a vanilla method by key, `net/minecraft/world/entity/item/ItemEntity#tick()V`; None when no loaded mod changes it.
    pub fn method(&self, key: &str) -> Option<MethodId> {
        self.method_ids.get(key).copied()
    }

    pub fn method_key(&self, id: MethodId) -> &str {
        &self.methods[id.0 as usize]
    }

    /// The id of an event by class name (slash form); None when no listener of it was found.
    pub fn event(&self, name: &str) -> Option<EventId> {
        self.event_ids.get(name).copied()
    }

    /// Whether any loaded mod listens to this event at all.
    pub fn listened(&self, event: EventId) -> bool {
        let e = &self.events[event.0 as usize];
        e.all || !e.filters.is_empty()
    }

    /// Reads `entity-types.bin` (class chains, then capabilities mods attached to types) and fixes every type's tables. Once.
    pub fn load_types(&self, bytes: &[u8]) -> Result<(), String> {
        let mut r = Reader(bytes);
        let mut chains = Vec::new();
        for _ in 0..r.count()? {
            chains.push((r.text()?, r.texts()?));
        }
        let mut extra = HashMap::new();
        for _ in 0..r.count()? {
            extra.insert(r.text()?, r.texts()?);
        }
        let mut types = HashMap::new();
        for (id, chain) in chains {
            types.insert(id.clone(), self.type_facts(&id, &chain));
        }
        self.types
            .set(types)
            .map_err(|_| "the entity types are already loaded".to_string())?;
        let _ = self.runtime_caps.set(extra);
        Ok(())
    }

    fn type_facts(&self, id: &str, chain: &[String]) -> TypeFacts {
        let mut facts = TypeFacts {
            reads: IDENTITY,
            living: chain
                .iter()
                .any(|c| c == "net/minecraft/world/entity/LivingEntity"),
            item: chain
                .iter()
                .any(|c| c == "net/minecraft/world/entity/item/ItemEntity"),
            ..TypeFacts::default()
        };
        for class in chain {
            if let Some(c) = self.classes.get(class) {
                facts.reads |= c.reads;
                for (method, handlers) in &c.edge {
                    let route = facts.routes.entry(MethodId(*method)).or_default();
                    for h in handlers {
                        if !route.edge.contains(h) {
                            route.edge.push(h.clone());
                        }
                    }
                }
                for method in &c.body {
                    facts.routes.entry(MethodId(*method)).or_default().body = true;
                }
            }
            for method in self.overrides.get(class).into_iter().flatten() {
                facts
                    .routes
                    .entry(MethodId(*method))
                    .or_default()
                    .overridden = true;
            }
        }
        let words = self.events.len().div_ceil(64);
        for (i, hook) in self.hooks.iter().enumerate() {
            let Some(hook) = hook else { continue };
            for client in [false, true] {
                if hook
                    .events
                    .iter()
                    .any(|e| self.events[*e].admits(id, chain, client))
                {
                    *(if client {
                        &mut facts.hook_client
                    } else {
                        &mut facts.hook_server
                    }) |= 1 << i;
                }
            }
        }
        facts.server = vec![0; words];
        facts.client = vec![0; words];
        for (i, event) in self.events.iter().enumerate() {
            for (client, bits) in [(false, &mut facts.server), (true, &mut facts.client)] {
                if event.admits(id, chain, client) {
                    bits[i / 64] |= 1 << (i % 64);
                }
            }
        }
        facts
    }

    /// What mods change in `method` for entities of this type; None when nothing routes.
    pub fn route(&self, type_id: &str, method: MethodId) -> Option<&Route> {
        self.types.get()?.get(type_id)?.routes.get(&method)
    }

    /// Whether `event` fires for an entity of this type on this side. A type whose class is unknown only passes filters that ask for no class.
    pub fn admits(&self, event: EventId, type_id: &str, client: bool) -> bool {
        match self.types.get().and_then(|t| t.get(type_id)) {
            Some(t) => {
                let bits = if client { &t.client } else { &t.server };
                bits.get(event.0 as usize / 64)
                    .is_some_and(|w| w >> (event.0 % 64) & 1 == 1)
            }
            None => self.events[event.0 as usize].admits(type_id, &[], client),
        }
    }

    /// The state groups a NeoForge hook reads, as a mask over `GROUPS`; 0 when the analysis has no such hook.
    pub fn hook_reads(&self, hook: Hook) -> u32 {
        self.hooks[hook as usize].as_ref().map_or(0, |h| h.reads)
    }

    /// Whether any event the hook posts fires for an entity of this type on this side; false for a hook the analysis lacks.
    pub fn hook_admits(&self, hook: Hook, type_id: &str, client: bool) -> bool {
        let Some(facts) = &self.hooks[hook as usize] else {
            return false;
        };
        match self.types.get().and_then(|t| t.get(type_id)) {
            Some(t) => {
                (if client { t.hook_client } else { t.hook_server }) >> hook as usize & 1 == 1
            }
            None => facts
                .events
                .iter()
                .any(|e| self.events[*e].admits(type_id, &[], client)),
        }
    }

    /// The state groups views of this type must carry, as a mask over `GROUPS`; identity always.
    pub fn reads(&self, type_id: &str) -> u32 {
        self.types
            .get()
            .and_then(|t| t.get(type_id))
            .map_or(IDENTITY, |t| t.reads)
    }

    /// Whether the type is a `LivingEntity` in Java, so its views have health, effects and equipment.
    pub fn living(&self, type_id: &str) -> bool {
        self.types
            .get()
            .and_then(|t| t.get(type_id))
            .is_some_and(|t| t.living)
    }

    /// Whether the type is an `ItemEntity` in Java, so its views have the item group.
    pub fn item(&self, type_id: &str) -> bool {
        self.types
            .get()
            .and_then(|t| t.get(type_id))
            .is_some_and(|t| t.item)
    }

    /// The capabilities mods register for an entity type: the analysis's, and once the types are loaded NeoForge's own providers.
    pub fn caps(&self, type_id: &str) -> Vec<String> {
        let mut out = self.caps.get(type_id).cloned().unwrap_or_default();
        out.extend(
            self.runtime_caps
                .get()
                .and_then(|c| c.get(type_id))
                .cloned()
                .unwrap_or_default(),
        );
        out.sort();
        out.dedup();
        out
    }

    /// Attachment types that serialize with their holder.
    pub fn serialized_attachments(&self) -> &[String] {
        &self.attachments
    }

    pub fn types_loaded(&self) -> bool {
        self.types.get().is_some()
    }
}

impl Event {
    fn admits(&self, type_id: &str, chain: &[String], client: bool) -> bool {
        self.all
            || self.filters.iter().any(|f| {
                let side = match f.side.unwrap_or(Side::Both) {
                    Side::Both => true,
                    Side::Server => !client,
                    Side::Client => client,
                };
                side && f.require.iter().all(|c| chain.contains(c))
                    && !f.exclude.iter().any(|c| chain.contains(c))
                    && (f.types.is_empty() || f.types.iter().any(|t| t == type_id))
            })
    }
}

/// The running host's facts, set once by the boot; None without analysis or without Java mods (nothing routes, nothing fires).
static FACTS: OnceLock<EntityFacts> = OnceLock::new();

pub(crate) fn set(facts: EntityFacts) {
    let _ = FACTS.set(facts);
}

pub fn get() -> Option<&'static EntityFacts> {
    FACTS.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Bin(Vec<u8>);

    impl Bin {
        fn int(&mut self, v: u32) -> &mut Self {
            self.0.extend(v.to_be_bytes());
            self
        }
        fn byte(&mut self, v: u8) -> &mut Self {
            self.0.push(v);
            self
        }
        fn text(&mut self, s: &str) -> &mut Self {
            self.int(s.len() as u32);
            self.0.extend(s.as_bytes());
            self
        }
    }

    const ITEM: &str = "net/minecraft/world/entity/item/ItemEntity";
    const ENTITY: &str = "net/minecraft/world/entity/Entity";
    const LIVING: &str = "net/minecraft/world/entity/LivingEntity";
    const PLAYER: &str = "net/minecraft/world/entity/player/Player";
    const TICK: &str = "net/minecraft/world/entity/item/ItemEntity#tick()V";

    fn sample() -> EntityFacts {
        let mut b = Bin::default();
        b.int(VERSION).int(2).text(TICK).text("a/b/Entity#tick()V");
        b.int(3);
        b.text(ENTITY).int(2 | 4).int(0).int(1).int(1);
        b.text(ITEM).int(1024).int(1).int(0).int(2);
        b.text("handleTick").byte(1).byte(0);
        b.text("early").byte(0).byte(1);
        b.int(0);
        b.text(LIVING).int(8).int(0).int(0);
        b.int(1).text("mod/Robit").int(1).int(0);
        b.int(2);
        b.text("tick").byte(0).int(1).byte(1);
        b.int(1).text(LIVING).int(1).text(PLAYER).int(0);
        b.text("leave").byte(1).int(0);
        b.int(1).text("minecraft:cow").int(1).text("a/Cap.X");
        b.int(1).text("a:att");
        b.int(2);
        b.text("living_breathe").int(128 | 256).int(2).int(1).int(0);
        b.text("unknown_hook").int(1).int(0);
        EntityFacts::read(&b.0).unwrap()
    }

    fn types() -> Vec<u8> {
        let mut b = Bin::default();
        b.int(3);
        b.text("minecraft:item").int(2).text(ITEM).text(ENTITY);
        b.text("minecraft:zombie")
            .int(3)
            .text("net/minecraft/world/entity/monster/Zombie")
            .text(LIVING)
            .text(ENTITY);
        b.text("minecraft:player")
            .int(3)
            .text(PLAYER)
            .text(LIVING)
            .text(ENTITY);
        b.int(1).text("minecraft:zombie").int(1).text("b/Cap.Y");
        b.0
    }

    #[test]
    fn no_boot_means_no_facts() {
        assert!(get().is_none());
    }

    #[test]
    fn a_hand_written_file_reads_back() {
        let facts = sample();
        assert_eq!(facts.method(TICK), Some(MethodId(0)));
        assert_eq!(facts.method("none"), None);
        assert_eq!(facts.method_key(MethodId(1)), "a/b/Entity#tick()V");
        assert!(facts.event("tick").is_some_and(|e| facts.listened(e)));
        assert_eq!(facts.caps("minecraft:cow"), ["a/Cap.X"]);
        assert!(facts.caps("minecraft:zombie").is_empty());
        assert_eq!(facts.serialized_attachments(), ["a:att"]);
    }

    #[test]
    fn a_class_chain_ors_reads_routes_and_overrides() {
        let facts = sample();
        facts.load_types(&types()).unwrap();
        assert_eq!(facts.reads("minecraft:item"), 1 | 2 | 4 | 1024);
        assert_eq!(facts.reads("minecraft:zombie"), 1 | 2 | 4 | 8);
        assert_eq!(facts.reads("minecraft:nothing"), 1);
        assert!(facts.living("minecraft:zombie") && !facts.living("minecraft:item"));
        assert!(facts.item("minecraft:item") && !facts.item("minecraft:zombie"));
        assert_eq!(facts.caps("minecraft:zombie"), ["b/Cap.Y"]);
        let route = facts.route("minecraft:item", MethodId(0)).unwrap();
        assert_eq!(
            route
                .edge
                .iter()
                .map(|h| (h.name.as_str(), h.exit, h.cancellable))
                .collect::<Vec<_>>(),
            [("handleTick", true, false), ("early", false, true)]
        );
        assert!(!route.body && !route.overridden);
        let on_entity = facts.route("minecraft:zombie", MethodId(1)).unwrap();
        assert!(on_entity.body);
        assert!(facts.route("minecraft:zombie", MethodId(0)).is_none());
    }

    #[test]
    fn an_override_on_a_mod_class_routes_its_own_types_only() {
        let facts = sample();
        let mut b = Bin::default();
        b.int(2);
        b.text("mod:robit")
            .int(3)
            .text("mod/Robit")
            .text(LIVING)
            .text(ENTITY);
        b.text("minecraft:zombie").int(2).text(LIVING).text(ENTITY);
        b.int(0);
        facts.load_types(&b.0).unwrap();
        assert!(facts.route("mod:robit", MethodId(0)).unwrap().overridden);
        assert!(facts.route("minecraft:zombie", MethodId(0)).is_none());
    }

    #[test]
    fn admits_applies_side_require_and_exclude() {
        let facts = sample();
        facts.load_types(&types()).unwrap();
        let tick = facts.event("tick").unwrap();
        assert!(facts.admits(tick, "minecraft:zombie", false));
        assert!(!facts.admits(tick, "minecraft:zombie", true));
        assert!(!facts.admits(tick, "minecraft:player", false));
        assert!(!facts.admits(tick, "minecraft:item", false));
        let leave = facts.event("leave").unwrap();
        assert!(facts.admits(leave, "minecraft:item", true));
        assert!(facts.admits(leave, "minecraft:player", false));
    }

    #[test]
    fn a_type_of_unknown_class_passes_only_filters_that_ask_for_none() {
        let facts = sample();
        facts.load_types(&types()).unwrap();
        assert!(!facts.admits(facts.event("tick").unwrap(), "mod:unknown", false));
        assert!(facts.admits(facts.event("leave").unwrap(), "mod:unknown", false));
    }

    #[test]
    fn a_hook_ors_its_events_and_unknown_or_missing_hooks_are_skipped() {
        let facts = sample();
        facts.load_types(&types()).unwrap();
        assert_eq!(facts.hook_reads(Hook::LivingBreathe), 128 | 256);
        assert_eq!(facts.hook_reads(Hook::MobDespawn), 0);
        assert!(facts.hook_admits(Hook::LivingBreathe, "minecraft:zombie", false));
        assert!(facts.hook_admits(Hook::LivingBreathe, "minecraft:zombie", true));
        assert!(facts.hook_admits(Hook::LivingBreathe, "minecraft:player", false));
        assert!(facts.hook_admits(Hook::LivingBreathe, "mod:unknown", false));
        assert!(!facts.hook_admits(Hook::MobDespawn, "minecraft:zombie", false));
    }

    #[test]
    fn version_one_is_refused_with_the_way_out() {
        let err = EntityFacts::read(&[0, 0, 0, 1]).unwrap_err();
        assert!(err.contains("format 2"));
    }

    #[test]
    fn types_load_once_and_a_short_or_wrong_file_is_refused() {
        let facts = sample();
        facts.load_types(&types()).unwrap();
        assert!(facts.load_types(&types()).is_err());
        assert!(EntityFacts::read(&[0, 0, 0, 3]).is_err());
        assert!(EntityFacts::read(&[0, 0, 0, 1, 0, 0, 0]).is_err());
        let mut b = Bin::default();
        b.int(VERSION)
            .int(0)
            .int(1)
            .text(ITEM)
            .int(0)
            .int(1)
            .int(5)
            .int(0)
            .int(0)
            .int(0)
            .int(0)
            .int(0)
            .int(0)
            .int(0)
            .int(0);
        assert!(EntityFacts::read(&b.0).unwrap_err().contains("method"));
    }
}
