// game-module-cfg: feature java-mods
// game-module-visibility: pub(crate)
//! The Java layer as a trait mod per `java_listen_*` family: its subscriptions are rows of ari's installed subscriber table, built from the cached census.

use super::Game;
use horizon_java_host::wire::{In, Out};
use mod_api::{HookId, Listener, ModEntry};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

include!(concat!(env!("OUT_DIR"), "/java_listener_families.rs"));

/// The trait-mod id the families are installed under, so a panic in one names the Java layer.
pub(crate) const ID: &str = "horizon_java";

/// One `java_listen_*` file's listener and every hook it handles.
pub(super) struct Family {
    pub(super) hooks: &'static [HookId],
    pub(super) listener: Box<dyn mod_api::Hooks>,
}

/// NeoForge's `EventPriority`, in its ordinal order: the census's `phases` bit of each is `1 << ordinal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum EventPriority {
    Highest,
    High,
    Normal,
    Low,
    Lowest,
}

/// The one table from NeoForge's priorities to ari's (higher runs first); at an equal priority Java's families run before native mods, as X1 installed them.
const PRIORITIES: [(EventPriority, i32); 5] = [
    (EventPriority::Highest, 200),
    (EventPriority::High, 100),
    (EventPriority::Normal, 0),
    (EventPriority::Low, -100),
    (EventPriority::Lowest, -200),
];

/// What a census found of one event's Java listeners.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Heard {
    /// A bit per [`EventPriority`] with a listener.
    phases: u8,
    /// Whether a listener has `receiveCanceled`.
    receives_cancelled: bool,
}

impl Heard {
    /// A listener at every phase in `phases`, none receiving cancelled events.
    fn at(phases: &[EventPriority]) -> Self {
        Heard {
            phases: phases
                .iter()
                .fold(0, |bits, phase| bits | 1 << *phase as u8),
            receives_cancelled: false,
        }
    }

    /// Where a post sits among native mods: at its earliest listener's phase, since one post runs every phase.
    fn priority(self) -> i32 {
        PRIORITIES
            .iter()
            .find(|(phase, _)| self.phases & (1 << *phase as u8) != 0)
            .map_or(0, |(_, priority)| *priority)
    }

    fn join(self, other: Heard) -> Heard {
        Heard {
            phases: self.phases | other.phases,
            receives_cancelled: self.receives_cancelled || other.receives_cancelled,
        }
    }
}

/// The listened catalog events by name.
type Census = BTreeMap<String, Heard>;

/// The census this launch subscribed with: `None` when no cached one matched the run's jars.
static SUBSCRIBED_WITH: OnceLock<Option<Census>> = OnceLock::new();
/// Where the census is cached between launches.
static CACHE: OnceLock<PathBuf> = OnceLock::new();
/// Set once a server start took the census: the boot's, if it answers later, is older.
static SERVER_CENSUS: AtomicBool = AtomicBool::new(false);

/// The NeoForge events the catalog posts at `hook`.
fn events_of(hook: HookId) -> impl Iterator<Item = &'static str> {
    mod_api::neoforge_events::EVENTS
        .iter()
        .filter(move |(_, id)| *id as usize == hook as usize)
        .map(|(event, _)| *event)
}

/// What a census lists of `hook`'s events, joined; `None` when it lists none.
fn heard_of(census: &Census, hook: HookId) -> Option<Heard> {
    events_of(hook)
        .filter_map(|event| census.get(event).copied())
        .reduce(Heard::join)
}

/// A family's subscriptions, computed once: its hooks and their `#[listener]` rows.
pub(super) struct Subscriptions {
    hooks: OnceLock<Vec<Option<HookId>>>,
    listeners: OnceLock<Vec<Listener>>,
}

impl Subscriptions {
    pub(super) const fn new() -> Self {
        Subscriptions {
            hooks: OnceLock::new(),
            listeners: OnceLock::new(),
        }
    }
}

/// The rows `hooks` get from `census`: no census subscribes every hook at NORMAL, told of cancelled events where the family carries them.
fn subscriptions(
    census: Option<&Census>,
    hooks: &[HookId],
    carries_cancel: bool,
) -> (Vec<Option<HookId>>, Vec<Listener>) {
    let unknown = Heard {
        receives_cancelled: true,
        ..Heard::at(&[EventPriority::Normal])
    };
    hooks
        .iter()
        .filter_map(|hook| {
            let heard = census.map_or(Some(unknown), |census| heard_of(census, *hook))?;
            let listener = Listener {
                hook: Some(*hook),
                priority: heard.priority(),
                receive_cancelled: carries_cancel && heard.receives_cancelled,
            };
            Some((Some(*hook), listener))
        })
        .unzip()
}

fn census_rows(
    hooks: &[HookId],
    cell: &'static Subscriptions,
    carries_cancel: bool,
) -> (&'static [Option<HookId>], &'static [Listener]) {
    let fill = || {
        let census = SUBSCRIBED_WITH.get().and_then(Option::as_ref);
        subscriptions(census, hooks, carries_cancel)
    };
    let mut rows = None;
    let subscribed = cell.hooks.get_or_init(|| {
        let (subscribed, listeners) = fill();
        rows = Some(listeners);
        subscribed
    });
    let listeners = cell
        .listeners
        .get_or_init(|| rows.take().unwrap_or_else(|| fill().1));
    (subscribed, listeners)
}

/// A family's `Hooks::subscribed`: its hooks the census this launch read lists.
pub(super) fn subscribed(
    hooks: &[HookId],
    cell: &'static Subscriptions,
    carries_cancel: bool,
) -> &'static [Option<HookId>] {
    census_rows(hooks, cell, carries_cancel).0
}

/// A family's `Hooks::listeners`: each subscribed hook's priority and receive-cancelled, as a trait mod's `#[listener]` gives them.
pub(super) fn listeners(
    hooks: &[HookId],
    cell: &'static Subscriptions,
    carries_cancel: bool,
) -> &'static [Listener] {
    census_rows(hooks, cell, carries_cancel).1
}

/// Per hook, by index: how many times a family crossed to Java for its event.
fn crossings() -> &'static [AtomicU64] {
    static TABLE: OnceLock<Box<[AtomicU64]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let count = mod_api::neoforge_events::EVENTS
            .iter()
            .map(|(_, hook)| *hook as usize + 1)
            .max()
            .unwrap_or(0);
        (0..count).map(|_| AtomicU64::new(0)).collect()
    })
}

/// A family is about to cross to Java for `hook`'s event.
pub(super) fn crossed(hook: HookId) {
    if let Some(count) = crossings().get(hook as usize) {
        count.fetch_add(1, Ordering::Relaxed);
    }
}

/// The crossings since the last take, by event class, for the profile's `event_crossings=`.
pub(super) fn take_crossings() -> Vec<(&'static str, u64)> {
    let mut taken = Vec::new();
    for (hook, count) in crossings().iter().enumerate() {
        let n = count.swap(0, Ordering::Relaxed);
        if n == 0 {
            continue;
        }
        if let Some((event, _)) = mod_api::neoforge_events::EVENTS
            .iter()
            .find(|(_, id)| *id as usize == hook)
        {
            taken.push((*event, n));
        }
    }
    taken
}

/// Switches each family hook's row of ari's table on where `census` lists its event, off elsewhere; `None` switches them all off.
fn switch(census: Option<&Census>) {
    for family in FAMILIES.iter().map(|family| family()) {
        for hook in family.hooks {
            let on = census.is_some_and(|census| heard_of(census, *hook).is_some());
            mod_api::__private::set_hook_enabled(ID, *hook, on);
        }
    }
}

/// The catalog's events a family posts from a hook it handles.
fn sited() -> std::collections::BTreeSet<&'static str> {
    FAMILIES
        .iter()
        .flat_map(|family| family().hooks)
        .flat_map(|hook| events_of(*hook))
        .collect()
}

/// The census request: `loud`, then every catalog event once with whether a family posts it.
fn census_request(loud: bool, sited: &std::collections::BTreeSet<&str>) -> Vec<u8> {
    let names: std::collections::BTreeSet<&str> = mod_api::neoforge_events::EVENTS
        .iter()
        .map(|(event, _)| *event)
        .collect();
    let mut out = Out::default();
    out.bool(loud);
    out.int(names.len() as i32);
    for name in names {
        out.text(name);
        out.bool(sited.contains(name));
    }
    out.0
}

/// The families with a hook to listen to, as trait mods ahead of the native ones; none without Java mods.
pub(crate) fn entries(game_dir: &Path) -> Vec<ModEntry> {
    let Some(run) = horizon_java_host::run_dir().map(Path::to_path_buf) else {
        return Vec::new();
    };
    let cache = std::path::absolute(game_dir)
        .unwrap_or_else(|_| game_dir.to_owned())
        .join(".horizon-cache/java-mods/listeners.txt");
    let census = read_census(&cache, &census_key(&run));
    if census.is_none() {
        log::info!(
            "Horizon: no listener census of these Java mods yet: every hook a Java family handles is subscribed this launch, switched by their boot's census"
        );
    }
    let _ = CACHE.set(cache);
    let _ = SUBSCRIBED_WITH.set(census);
    FAMILIES
        .iter()
        .map(|family| family().listener)
        .filter(|listener| !listener.subscribed().is_empty())
        .map(|listener| ModEntry::new(ID, listener))
        .collect()
}

/// What the cached census is valid for: the run's JVM arguments and its mod jars.
fn census_key(run: &Path) -> String {
    let mut hash = Sha256::new();
    hash.update(std::fs::read(run.join("horizon-java.args")).unwrap_or_default());
    let mut jars: Vec<_> = std::fs::read_dir(run.join("mods"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            let modified = meta
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?;
            Some((entry.file_name(), meta.len(), modified.as_nanos()))
        })
        .collect();
    jars.sort();
    for (name, len, modified) in jars {
        hash.update(name.as_encoded_bytes());
        hash.update(len.to_le_bytes());
        hash.update(modified.to_le_bytes());
    }
    format!("{:x}", hash.finalize())
}

/// One cached line, `name phases receives`; a bare name (the older cache) is a NORMAL listener.
fn census_line(line: &str) -> Option<(String, Heard)> {
    let mut words = line.split_whitespace();
    let name = words.next()?.to_owned();
    let heard = match (words.next(), words.next()) {
        (Some(phases), Some(receives)) => Heard {
            phases: phases.parse().ok()?,
            receives_cancelled: receives == "1",
        },
        _ => Heard::at(&[EventPriority::Normal]),
    };
    Some((name, heard))
}

/// The cached census, when it was taken from the same run as `key`.
fn read_census(path: &Path, key: &str) -> Option<Census> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    (lines.next()? == format!("key {key}")).then(|| lines.filter_map(census_line).collect())
}

/// The cache's text of `census`, for the run `key`.
fn census_text(key: &str, census: &Census) -> String {
    let mut text = format!("key {key}\n");
    for (event, heard) in census {
        let receives = u8::from(heard.receives_cancelled);
        text.push_str(&format!("{event} {} {receives}\n", heard.phases));
    }
    text
}

/// Asks the booted mods which events they listen to, once their boot is done, and keeps the answer for the next launch.
pub(super) fn take_census() {
    if horizon_java_host::run_dir().is_none() {
        return;
    }
    // Until the boot's census answers, no family crosses: the JVM has nothing to post to.
    switch(None);
    let spawned = std::thread::Builder::new()
        .name("Java listener census".into())
        .spawn(|| census(false));
    if let Err(e) = spawned {
        log::error!("Horizon: the Java listener census did not start: {e}");
    }
}

/// The census again at a server start, logged per event: mods that subscribe while the server starts are heard from here on.
pub(super) fn census_at_server_start() {
    SERVER_CENSUS.store(true, Ordering::Relaxed);
    census(true);
}

/// Asks the mods which catalog events they listen to, then switches the families' rows and caches the answer; `loud` has Java log each event.
fn census(loud: bool) {
    let (Some(run), Some(host)) = (
        horizon_java_host::run_dir().map(Path::to_path_buf),
        horizon_java_host::get(),
    ) else {
        return;
    };
    let request = census_request(loud, &sited());
    let reply =
        horizon_java_host::bridge::without_active(|| host.request("listener_census", &request));
    let Some(live) = reply.as_deref().and_then(read_heard) else {
        log::error!(
            "Horizon: the Java mods' listener census did not answer; no hook posts to them"
        );
        return;
    };
    static APPLYING: Mutex<()> = Mutex::new(());
    let _applying = APPLYING.lock().unwrap_or_else(|e| e.into_inner());
    if !loud && SERVER_CENSUS.load(Ordering::Relaxed) {
        return;
    }
    report(&live);
    if let Some(cache) = CACHE.get() {
        let written = cache
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(cache, census_text(&census_key(&run), &live)));
        if let Err(e) = written {
            log::warn!(
                "Horizon: the Java listener census is not cached: {}: {e}",
                cache.display()
            );
        }
    }
    switch(Some(&live));
}

/// `int n` x (name, `byte phases`, `bool receives`), as the census answers them.
fn read_heard(reply: &[u8]) -> Option<Census> {
    let mut reply = In(reply);
    (0..reply.int()?)
        .map(|_| {
            let name = reply.text()?.to_owned();
            let heard = Heard {
                phases: reply.byte()?,
                receives_cancelled: reply.bool()?,
            };
            Some((name, heard))
        })
        .collect()
}

/// Says which listened events reach Java this launch, and which only reach it from the next one.
fn report(live: &Census) {
    let census = SUBSCRIBED_WITH.get().and_then(Option::as_ref);
    let mut posted = Vec::new();
    for family in FAMILIES.iter().map(|family| family()) {
        for hook in family.hooks {
            for event in events_of(*hook).filter(|event| live.contains_key(*event)) {
                match census.map(|census| (heard_of(census, *hook), live[event])) {
                    None => posted.push(event),
                    Some((Some(subscribed), now)) => {
                        posted.push(event);
                        if subscribed.join(now) != subscribed {
                            log::warn!(
                                "Horizon: {event}'s Java listeners changed priority or receive-cancelled since the cached census; the next launch orders them anew"
                            );
                        }
                    }
                    Some((None, _)) => log::warn!(
                        "Horizon: a Java mod listens to {event}, which this launch did not subscribe to; the next launch does"
                    ),
                }
            }
        }
    }
    log::info!(
        "Horizon: Java mods listen to {} events; posted from native hooks: {posted:?}",
        live.len()
    );
}

/// A client event posted to the Java client (`client_post_event`): `fields` follow its name, `read` takes the answer.
pub(super) fn post_client<R>(
    hook: HookId,
    event: &str,
    fields: impl FnOnce(&mut Out),
    read: impl FnOnce(&mut In) -> Option<R>,
) -> Option<R> {
    let host = horizon_java_host::get()?;
    crossed(hook);
    let mut out = Out::default();
    out.text(event);
    fields(&mut out);
    // No game is at hand where a client event posts: a native the listener reaches finds none.
    let reply =
        horizon_java_host::bridge::without_active(|| host.request("client_post_event", &out.0))?;
    read(&mut In(&reply))
}

/// Runs `f` on the game behind a server hook's host, lent as a Java call lends it: only `Game` gives a host a backend.
pub(super) fn with_game<R>(
    host: &mut dyn mod_api::ServerHost,
    f: impl FnOnce(&mut Game<'static>) -> R,
) -> Option<R> {
    let backend = mod_api::backend_mut_of(host)?;
    horizon_java_host::bridge::with_active(backend, || {
        horizon_java_host::bridge::on_active::<Game<'static>, _>(f)
    })
}

/// Runs `f` on the game behind a client hook's host, lent as a Java call lends it: only `Game` gives a host a backend.
pub(super) fn with_client_game<R>(
    host: &mut dyn mod_api::ClientHost,
    f: impl FnOnce(&mut Game<'static>) -> R,
) -> Option<R> {
    let backend = mod_api::client_backend_mut_of(host)?;
    horizon_java_host::bridge::with_active(backend, || {
        horizon_java_host::bridge::on_active::<Game<'static>, _>(f)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn census(rows: &[(&str, Heard)]) -> Census {
        rows.iter()
            .map(|(name, heard)| ((*name).to_owned(), *heard))
            .collect()
    }

    /// A trait mod with the rows the Java layer computes.
    struct Rows(&'static [Option<HookId>], &'static [Listener]);

    impl mod_api::Hooks for Rows {
        fn subscribed(&self) -> &'static [Option<HookId>] {
            self.0
        }

        fn listeners(&self) -> &'static [Listener] {
            self.1
        }
    }

    fn rows(census: Option<&Census>, hooks: &[HookId], carries: bool) -> Rows {
        let (subscribed, listeners) = subscriptions(census, hooks, carries);
        Rows(Vec::leak(subscribed), Vec::leak(listeners))
    }

    #[test]
    fn neoforge_priorities_keep_their_order_on_aris_scale() {
        let ordinals: Vec<_> = PRIORITIES.iter().map(|(phase, _)| *phase as u8).collect();
        assert_eq!(
            ordinals,
            [0, 1, 2, 3, 4],
            "the table follows EventPriority's ordinals"
        );
        assert!(PRIORITIES.windows(2).all(|pair| pair[0].1 > pair[1].1));
        assert_eq!(Heard::at(&[EventPriority::Normal]).priority(), 0);
        assert_eq!(
            Heard::at(&[EventPriority::Low, EventPriority::High]).priority(),
            100
        );
        assert_eq!(Heard::at(&[EventPriority::Lowest]).priority(), -200);
    }

    #[test]
    fn an_unlistened_hook_gets_no_row_so_aris_gate_stays_shut() {
        let live = census(&[("BreakBlockEvent", Heard::at(&[EventPriority::Normal]))]);
        let hooks = [HookId::BreakBlock, HookId::AttackEntity, HookId::PlaySound];
        let family = rows(Some(&live), &hooks, true);
        assert_eq!(family.0.len(), 1);
        let mods: mod_api::Mods = Arc::from([ModEntry::new(ID, Box::new(family))]);
        mod_api::__private::install(mods);
        assert!(mod_api::__private::listened(HookId::BreakBlock));
        assert!(!mod_api::__private::listened(HookId::AttackEntity));
        assert!(!mod_api::__private::listened(HookId::PlaySound));
    }

    #[test]
    fn a_hook_the_live_census_drops_is_switched_off_in_aris_table() {
        let everything = rows(None, &[HookId::BreakBlock, HookId::AttackEntity], true);
        assert_eq!(
            everything.0.len(),
            2,
            "without a census every hook is subscribed"
        );
        let mods: mod_api::Mods = Arc::from([ModEntry::new(ID, Box::new(everything))]);
        mod_api::__private::install(mods);
        let live = census(&[("AttackEntityEvent", Heard::at(&[EventPriority::High]))]);
        for hook in [HookId::BreakBlock, HookId::AttackEntity] {
            mod_api::__private::set_hook_enabled(ID, hook, heard_of(&live, hook).is_some());
        }
        assert!(!mod_api::subscribed_by(0, Some(HookId::BreakBlock)));
        assert!(mod_api::subscribed_by(0, Some(HookId::AttackEntity)));
    }

    #[test]
    fn receive_cancelled_reaches_the_row_only_where_the_family_carries_a_cancel() {
        let receiving = Heard {
            receives_cancelled: true,
            ..Heard::at(&[EventPriority::Lowest])
        };
        let live = census(&[("BreakBlockEvent", receiving)]);
        let carried = rows(Some(&live), &[HookId::BreakBlock], true);
        assert_eq!(carried.1.len(), 1);
        assert!(carried.1[0].receive_cancelled);
        assert_eq!(carried.1[0].priority, -200);
        let dropped = rows(Some(&live), &[HookId::BreakBlock], false);
        assert!(!dropped.1[0].receive_cancelled);
    }

    #[test]
    fn events_of_one_hook_join_their_phases_and_receivers() {
        let live = census(&[
            ("BreakBlockEvent", Heard::at(&[EventPriority::Low])),
            ("PlaySoundEvent", Heard::at(&[EventPriority::Highest])),
        ]);
        assert_eq!(
            heard_of(&live, HookId::BreakBlock),
            Some(Heard::at(&[EventPriority::Low]))
        );
        assert_eq!(heard_of(&live, HookId::AttackEntity), None);
    }

    #[test]
    fn the_catalog_names_the_events_of_the_proven_hooks() {
        for (hook, event) in [
            (HookId::PlaySound, "PlaySoundEvent"),
            (HookId::GuiLayerPre, "RenderGuiLayerEvent.Pre"),
            (HookId::BreakBlock, "BreakBlockEvent"),
            (HookId::AttackEntity, "AttackEntityEvent"),
        ] {
            assert_eq!(events_of(hook).collect::<Vec<_>>(), [event]);
        }
    }

    #[test]
    fn a_cached_census_counts_only_for_the_run_it_was_taken_from() {
        let dir = std::env::temp_dir().join(format!("hz-census-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("listeners.txt");
        let receiving = Heard {
            receives_cancelled: true,
            ..Heard::at(&[EventPriority::High, EventPriority::Low])
        };
        let live = census(&[("BreakBlockEvent", receiving)]);
        std::fs::write(&path, census_text("abc", &live) + "PlaySoundEvent\n").unwrap();
        let read = read_census(&path, "abc").unwrap();
        let other = read_census(&path, "def");
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(read["BreakBlockEvent"], receiving);
        assert_eq!(read["PlaySoundEvent"], Heard::at(&[EventPriority::Normal]));
        assert_eq!(other, None);
    }

    #[test]
    fn the_census_reply_reads_each_event_and_refuses_a_short_one() {
        let mut out = Out::default();
        out.int(2);
        out.text("PlaySoundEvent");
        out.0.push(1 << EventPriority::High as u8);
        out.bool(false);
        out.text("RenderGuiLayerEvent.Pre");
        out.0
            .push(1 << EventPriority::Normal as u8 | 1 << EventPriority::Lowest as u8);
        out.bool(true);
        let heard = read_heard(&out.0).unwrap();
        assert_eq!(heard["PlaySoundEvent"], Heard::at(&[EventPriority::High]));
        assert_eq!(heard["RenderGuiLayerEvent.Pre"].priority(), 0);
        assert!(heard["RenderGuiLayerEvent.Pre"].receives_cancelled);
        assert_eq!(read_heard(&out.0[..out.0.len() - 1]), None);
    }

    #[test]
    fn crossings_are_counted_by_event_class_and_taken_once() {
        take_crossings();
        crossed(HookId::BreakBlock);
        crossed(HookId::BreakBlock);
        let taken = take_crossings();
        assert!(taken.contains(&("BreakBlockEvent", 2)), "{taken:?}");
        assert!(!taken.iter().any(|(event, _)| *event == "PlaySoundEvent"));
        assert!(
            !take_crossings()
                .iter()
                .any(|(event, _)| *event == "BreakBlockEvent")
        );
    }

    #[test]
    fn the_census_asks_each_event_once_and_says_which_a_family_posts() {
        let sited: std::collections::BTreeSet<&str> = ["BreakBlockEvent"].into();
        let request = census_request(true, &sited);
        let mut read = In(&request);
        assert_eq!(read.bool(), Some(true));
        let count = read.int().unwrap();
        let mut asked = std::collections::BTreeSet::new();
        let mut posted = Vec::new();
        for _ in 0..count {
            let name = read.text().unwrap();
            assert!(asked.insert(name), "{name} asked twice");
            if read.bool().unwrap() {
                posted.push(name);
            }
        }
        assert!(read.0.is_empty());
        assert!(asked.contains("PlayerEvent.Clone"));
        assert_eq!(posted, ["BreakBlockEvent"]);
    }

    #[test]
    fn every_family_handles_hooks_the_catalog_maps_to_an_event() {
        for family in FAMILIES.iter().map(|family| family()) {
            for hook in family.hooks {
                assert!(events_of(*hook).next().is_some(), "{:?}", *hook as usize);
            }
        }
    }
}
