// game-module-cfg: feature java-mods
// game-module-visibility: pub(crate)
//! The Java layer as a trait mod per `java_listen_*` family, subscribed only where a Java mod listens.

use super::Game;
use horizon_java_host::wire::{In, Out};
use mod_api::{HookId, ModEntry};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering};

include!(concat!(env!("OUT_DIR"), "/java_listener_families.rs"));

/// The trait-mod id the families are installed under, so a panic in one names the Java layer.
pub(crate) const ID: &str = "horizon_java";

/// One `java_listen_*` file's listener and every hook it handles.
pub(super) struct Family {
    pub(super) hooks: &'static [HookId],
    pub(super) listener: Box<dyn mod_api::Hooks>,
}

/// The census this launch subscribed with: `None` when no cached one matched the run's jars.
static SUBSCRIBED_WITH: OnceLock<Option<BTreeSet<String>>> = OnceLock::new();
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

/// Whether a census lists an event of `hook`; no census subscribes every hook a family handles.
fn census_lists(census: Option<&BTreeSet<String>>, hook: HookId) -> bool {
    census.is_none_or(|census| events_of(hook).any(|event| census.contains(event)))
}

/// A family's `Hooks::subscribed`: its hooks the census this launch read lists, computed once.
pub(super) fn subscribed(
    hooks: &[HookId],
    cell: &'static OnceLock<Vec<Option<HookId>>>,
) -> &'static [Option<HookId>] {
    cell.get_or_init(|| {
        let census = SUBSCRIBED_WITH.get().and_then(Option::as_ref);
        hooks
            .iter()
            .filter(|hook| census_lists(census, **hook))
            .map(|hook| Some(*hook))
            .collect()
    })
}

/// Per hook, by its index: whether the last census found a listener of one of its events.
fn listened_hooks() -> &'static [AtomicBool] {
    static TABLE: OnceLock<Box<[AtomicBool]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let count = mod_api::neoforge_events::EVENTS
            .iter()
            .map(|(_, hook)| *hook as usize + 1)
            .max()
            .unwrap_or(0);
        (0..count).map(|_| AtomicBool::new(false)).collect()
    })
}

/// Whether a booted Java mod listens to `hook`'s event: false until a census is in.
pub(super) fn listens(hook: HookId) -> bool {
    listened_hooks()
        .get(hook as usize)
        .is_some_and(|cell| cell.load(Ordering::Relaxed))
}

/// The hooks `live` lists an event of, by index, as [`listened_hooks`] keeps them.
fn hooks_listened(live: &BTreeSet<String>, count: usize) -> Vec<bool> {
    let mut listened = vec![false; count];
    for (event, hook) in mod_api::neoforge_events::EVENTS {
        if live.contains(*event)
            && let Some(cell) = listened.get_mut(*hook as usize)
        {
            *cell = true;
        }
    }
    listened
}

/// The catalog's events a family posts from a hook it handles.
fn sited() -> BTreeSet<&'static str> {
    FAMILIES
        .iter()
        .flat_map(|family| family().hooks)
        .flat_map(|hook| events_of(*hook))
        .collect()
}

/// The census request: `loud`, then every catalog event once with whether a family posts it.
fn census_request(loud: bool, sited: &BTreeSet<&str>) -> Vec<u8> {
    let names: BTreeSet<&str> = mod_api::neoforge_events::EVENTS
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
            "Horizon: no listener census of these Java mods yet: every hook a Java family handles is listened to until their boot answers"
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
            let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
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

/// The cached census, when it was taken from the same run as `key`.
fn read_census(path: &Path, key: &str) -> Option<BTreeSet<String>> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    (lines.next()? == format!("key {key}")).then(|| lines.map(str::to_owned).collect())
}

/// Asks the booted mods which events they listen to, once their boot is done, and keeps the answer for the next launch.
pub(super) fn take_census() {
    if horizon_java_host::run_dir().is_none() {
        return;
    }
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

/// Asks the mods which catalog events they listen to, then gates the hooks and caches the answer; `loud` has Java log each event.
fn census(loud: bool) {
    let (Some(run), Some(host)) = (
        horizon_java_host::run_dir().map(Path::to_path_buf),
        horizon_java_host::get(),
    ) else {
        return;
    };
    let request = census_request(loud, &sited());
    let reply = horizon_java_host::bridge::without_active(|| {
        host.request("listener_census", &request)
    });
    let Some(live) = reply.as_deref().and_then(read_names) else {
        log::error!("Horizon: the Java mods' listener census did not answer; no hook posts to them");
        return;
    };
    static APPLYING: Mutex<()> = Mutex::new(());
    let _applying = APPLYING.lock().unwrap_or_else(|e| e.into_inner());
    if !loud && SERVER_CENSUS.load(Ordering::Relaxed) {
        return;
    }
    report(&live);
    if let Some(cache) = CACHE.get() {
        let mut text = format!("key {}\n", census_key(&run));
        live.iter().for_each(|event| text.push_str(&format!("{event}\n")));
        let written = cache
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(cache, text));
        if let Err(e) = written {
            log::warn!("Horizon: the Java listener census is not cached: {}: {e}", cache.display());
        }
    }
    let table = listened_hooks();
    for (cell, listened) in table.iter().zip(hooks_listened(&live, table.len())) {
        cell.store(listened, Ordering::Relaxed);
    }
}

/// `int n` names, as the census answers them.
fn read_names(reply: &[u8]) -> Option<BTreeSet<String>> {
    let mut reply = In(reply);
    (0..reply.int()?)
        .map(|_| reply.text().map(str::to_owned))
        .collect()
}

/// Says which listened events reach Java this launch, and which only reach it from the next one.
fn report(live: &BTreeSet<String>) {
    let census = SUBSCRIBED_WITH.get().and_then(Option::as_ref);
    let mut posted = Vec::new();
    for family in FAMILIES.iter().map(|family| family()) {
        for hook in family.hooks {
            for event in events_of(*hook).filter(|event| live.contains(*event)) {
                if census_lists(census, *hook) {
                    posted.push(event);
                } else {
                    log::warn!(
                        "Horizon: a Java mod listens to {event}, which this launch did not subscribe to; the next launch does"
                    );
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
    event: &str,
    fields: impl FnOnce(&mut Out),
    read: impl FnOnce(&mut In) -> Option<R>,
) -> Option<R> {
    let host = horizon_java_host::get()?;
    let mut out = Out::default();
    out.text(event);
    fields(&mut out);
    // No game is at hand where a client event posts: a native the listener reaches finds none.
    let reply = horizon_java_host::bridge::without_active(|| {
        host.request("client_post_event", &out.0)
    })?;
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

    #[test]
    fn a_hook_is_listened_by_any_of_its_events_and_by_no_census() {
        let census: BTreeSet<String> = ["RenderGuiLayerEvent.Pre".to_owned()].into();
        assert!(census_lists(Some(&census), HookId::GuiLayerPre));
        assert!(!census_lists(Some(&census), HookId::PlaySound));
        assert!(census_lists(None, HookId::PlaySound));
        assert!(!census_lists(Some(&BTreeSet::new()), HookId::GuiLayerPre));
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
        std::fs::write(&path, "key abc\nPlaySoundEvent\nBreakBlockEvent\n").unwrap();
        let read = read_census(&path, "abc").unwrap();
        let other = read_census(&path, "def");
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(read, ["BreakBlockEvent".to_owned(), "PlaySoundEvent".to_owned()].into());
        assert_eq!(other, None);
    }

    #[test]
    fn the_census_reply_reads_its_names_and_refuses_a_short_one() {
        let mut out = Out::default();
        out.int(2);
        out.text("PlaySoundEvent");
        out.text("RenderGuiLayerEvent.Pre");
        let names = read_names(&out.0).unwrap();
        assert_eq!(names.len(), 2);
        assert!(names.contains("RenderGuiLayerEvent.Pre"));
        assert_eq!(read_names(&out.0[..out.0.len() - 1]), None);
    }

    #[test]
    fn a_listened_event_gates_every_hook_it_posts_from_and_no_other() {
        let live: BTreeSet<String> = ["BreakBlockEvent".to_owned()].into();
        let listened = hooks_listened(&live, listened_hooks().len());
        assert!(listened[HookId::BreakBlock as usize]);
        assert!(!listened[HookId::PlaySound as usize]);
        assert!(!listened[HookId::AttackEntity as usize]);
        assert_eq!(listened.iter().filter(|l| **l).count(), 1);
    }

    #[test]
    fn the_census_asks_each_event_once_and_says_which_a_family_posts() {
        let sited: BTreeSet<&str> = ["BreakBlockEvent"].into();
        let request = census_request(true, &sited);
        let mut read = In(&request);
        assert_eq!(read.bool(), Some(true));
        let count = read.int().unwrap();
        let mut asked = BTreeSet::new();
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
