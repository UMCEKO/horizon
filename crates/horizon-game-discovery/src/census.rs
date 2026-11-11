//! The client/server census: which side's `Game` reaches each `impl Game` method, field and file.
mod extract;
mod guards;
mod report;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub use report::{
    AreaReport, Census, Duplicate, FieldKind, FieldReport, FilePurity, FileReport, Listing,
    MethodReport, PurityTally, Side, SideCounts, Totals,
};

use extract::Parsed;

/// A way of reading the code: as a client's `Game` runs it, as a server's does, or with no guard pruned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Client,
    Server,
    Both,
}

const CLIENT_ROOTS: &[&str] = &[
    "tick_remote",
    "render",
    "hud",
    "poll_remote",
    "handle_packet",
    "handle_window_event_inner",
    "<ApplicationHandler>::window_event",
    "<ApplicationHandler>::about_to_wait",
    "<ApplicationHandler>::device_event",
    "<ApplicationHandler>::resumed",
    "remote_handle_input",
    "connect_server",
    "join_server",
    "leave_server",
    "tick_client_load",
];

const SERVER_ROOTS: &[&str] = &[
    "tick_server_body",
    "run_dedicated_inner",
    "run_integrated_inner",
    "lan_guest_packet",
    "tick_guests",
    "tick_lan",
    "tick_lan_packets",
    "lan_process_events",
    "integrated_open",
    "run_tasks",
    "owner_console_command",
    "run_command_harness",
];

/// Traits whose impls on `Game` are the server's entry points.
const SERVER_TRAITS: &[&str] = &[
    "ServerHost",
    "Levels",
    "Players",
    "Commands",
    "BackendMut",
    "Messages",
];
/// Traits whose impls on `Game` are the client's entry points.
const CLIENT_TRAITS: &[&str] = &[
    "ClientHost",
    "ClientSession",
    "ClientBackendMut",
    "ApplicationHandler",
    "RenderAssets",
    "InstanceGroups",
];

/// Where a view stops following a method's calls: the client never goes past `tick`'s remote return, the server never enters it.
const CUTS: &[(View, &str, &[&str])] = &[
    (
        View::Client,
        "tick",
        &[
            "tick_world",
            "tick_functions",
            "save_game_with",
            "advance_clock",
            "run_scheduled_functions",
        ],
    ),
    (View::Server, "tick", &["tick_remote"]),
];

/// Runs the census over `root`, a Horizon checkout (or any folder holding `crates/horizon-client/src`).
pub fn run(root: &Path) -> Result<Census, String> {
    let src = root.join("crates/horizon-client/src");
    census(&extract::parse_tree(&src)?)
}

fn trait_roots(parsed: &Parsed, traits: &[&str]) -> Vec<String> {
    parsed
        .methods
        .iter()
        .filter(|(_, m)| m.trait_name.as_deref().is_some_and(|t| traits.contains(&t)))
        .map(|(key, _)| key.clone())
        .collect()
}

fn reach(
    parsed: &Parsed,
    inherent: &BTreeSet<&str>,
    view: View,
    roots: &[String],
) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<&str> = roots
        .iter()
        .map(String::as_str)
        .filter(|r| parsed.methods.contains_key(*r))
        .collect();
    while let Some(key) = stack.pop() {
        if !seen.insert(key.to_owned()) {
            continue;
        }
        let cut: &[&str] = CUTS
            .iter()
            .find(|(v, name, _)| *v == view && *name == key)
            .map_or(&[], |(_, _, cut)| cut);
        for callee in &parsed.methods[key].edges.of(view).calls {
            if inherent.contains(callee.as_str())
                && !cut.contains(&callee.as_str())
                && !seen.contains(callee)
            {
                stack.push(callee);
            }
        }
    }
    seen
}

/// Per field: how many methods touch it as [client, server, both].
type Access<'a> = BTreeMap<&'a str, [usize; 3]>;
const CLIENT: usize = 0;
const SERVER: usize = 1;
const BOTH: usize = 2;

fn bump<'a>(access: &mut Access<'a>, names: impl IntoIterator<Item = &'a str>, slot: usize) {
    for name in names {
        access.entry(name).or_default()[slot] += 1;
    }
}

fn census(parsed: &Parsed) -> Result<Census, String> {
    let inherent: BTreeSet<&str> = parsed
        .methods
        .iter()
        .filter(|(_, m)| m.trait_name.is_none())
        .map(|(key, _)| key.as_str())
        .collect();
    let fields: BTreeSet<&str> = parsed.fields.iter().map(String::as_str).collect();

    let roots = |names: &[&str], traits: &[&str]| -> Vec<String> {
        names
            .iter()
            .map(|n| (*n).to_owned())
            .chain(trait_roots(parsed, traits))
            .collect()
    };
    let client_roots = roots(CLIENT_ROOTS, CLIENT_TRAITS);
    let server_roots = roots(SERVER_ROOTS, SERVER_TRAITS);
    let missing_roots: Vec<String> = client_roots
        .iter()
        .chain(&server_roots)
        .filter(|root| !parsed.methods.contains_key(*root))
        .cloned()
        .collect();
    let client = reach(parsed, &inherent, View::Client, &client_roots);
    let server = reach(parsed, &inherent, View::Server, &server_roots);

    let side_of = |key: &str| match (client.contains(key), server.contains(key)) {
        (true, true) => Side::Both,
        (true, false) => Side::Client,
        (false, true) => Side::Server,
        (false, false) => Side::Neither,
    };

    let mut methods = Vec::new();
    let mut access: Access = BTreeMap::new();
    for (key, method) in parsed.methods.iter().filter(|(_, m)| !m.test) {
        let side = side_of(key);
        methods.push(MethodReport {
            name: key.clone(),
            file: method.file.clone(),
            lines: method.lines,
            side,
        });
        let used = |view: View| -> BTreeSet<&str> {
            method
                .edges
                .of(view)
                .fields
                .iter()
                .map(String::as_str)
                .filter(|f| fields.contains(f))
                .collect()
        };
        match side {
            Side::Both => {
                let (c, s) = (used(View::Client), used(View::Server));
                bump(&mut access, c.intersection(&s).copied(), BOTH);
                bump(&mut access, c.difference(&s).copied(), CLIENT);
                bump(&mut access, s.difference(&c).copied(), SERVER);
            }
            Side::Client => bump(&mut access, used(View::Client), CLIENT),
            Side::Server => bump(&mut access, used(View::Server), SERVER),
            Side::Neither => {}
        }
    }
    let field_reports = parsed
        .fields
        .iter()
        .map(|name| {
            let [client_methods, server_methods, both_methods] =
                access.get(name.as_str()).copied().unwrap_or_default();
            FieldReport::new(name, client_methods, server_methods, both_methods)
        })
        .collect();
    let duplicates = parsed
        .duplicates
        .iter()
        .map(|(method, files)| Duplicate {
            method: method.clone(),
            files: files.clone(),
        })
        .collect();
    Ok(Census::new(
        methods,
        field_reports,
        duplicates,
        missing_roots,
    ))
}

#[cfg(test)]
mod tests;
