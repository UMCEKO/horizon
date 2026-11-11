//! The census result: typed, serializable, and printable as text.
use std::{collections::BTreeMap, fmt::Write};

use serde::{Deserialize, Serialize};

/// Which side's `Game` reaches a method or touches a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Server,
    Client,
    Both,
    /// Reached from neither side's roots: startup, fixtures, glue called by generated code.
    Neither,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::Server, Side::Client, Side::Both, Side::Neither];

    fn label(self) -> &'static str {
        match self {
            Side::Server => "server only",
            Side::Client => "client only",
            Side::Both => "both",
            Side::Neither => "neither",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SideCounts {
    pub server: usize,
    pub client: usize,
    pub both: usize,
    pub neither: usize,
}

impl SideCounts {
    pub fn get(&self, side: Side) -> usize {
        match side {
            Side::Server => self.server,
            Side::Client => self.client,
            Side::Both => self.both,
            Side::Neither => self.neither,
        }
    }

    pub fn add(&mut self, side: Side, amount: usize) {
        match side {
            Side::Server => self.server += amount,
            Side::Client => self.client += amount,
            Side::Both => self.both += amount,
            Side::Neither => self.neither += amount,
        }
    }

    pub fn total(&self) -> usize {
        self.server + self.client + self.both + self.neither
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodReport {
    /// The method name, or `<Trait>::name` for a trait impl.
    pub name: String,
    pub file: String,
    /// Lines of the body.
    pub lines: usize,
    pub side: Side,
}

/// How the methods touching a field split, finer than the field's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    /// Touched by server-side methods only.
    Server,
    /// Server-side methods, and shared methods that use it on the server view only.
    ServerShared,
    Client,
    ClientShared,
    /// Touched by client-side and by server-side methods.
    Direct,
    /// Touched only by shared methods, on both views.
    Shared,
    Untouched,
}

impl FieldKind {
    pub fn side(self) -> Side {
        match self {
            FieldKind::Server | FieldKind::ServerShared => Side::Server,
            FieldKind::Client | FieldKind::ClientShared => Side::Client,
            FieldKind::Direct | FieldKind::Shared => Side::Both,
            FieldKind::Untouched => Side::Neither,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldReport {
    pub name: String,
    pub side: Side,
    pub kind: FieldKind,
    /// Methods reached by the client only that touch it.
    pub client_methods: usize,
    /// Methods reached by the server only that touch it.
    pub server_methods: usize,
    /// Shared methods that touch it on both views.
    pub both_methods: usize,
}

impl FieldReport {
    pub(super) fn new(
        name: &str,
        client_methods: usize,
        server_methods: usize,
        both_methods: usize,
    ) -> Self {
        let kind = match (client_methods > 0, server_methods > 0, both_methods > 0) {
            (true, true, _) => FieldKind::Direct,
            (false, false, true) => FieldKind::Shared,
            (true, false, shared) => {
                if shared {
                    FieldKind::ClientShared
                } else {
                    FieldKind::Client
                }
            }
            (false, true, shared) => {
                if shared {
                    FieldKind::ServerShared
                } else {
                    FieldKind::Server
                }
            }
            (false, false, false) => FieldKind::Untouched,
        };
        FieldReport {
            name: name.to_owned(),
            side: kind.side(),
            kind,
            client_methods,
            server_methods,
            both_methods,
        }
    }
}

/// What a file's methods add up to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilePurity {
    PureServer,
    PureClient,
    PureBoth,
    /// More than one of server, client and both.
    Mixed,
    /// Only methods neither side reaches.
    NeitherOnly,
}

impl FilePurity {
    fn label(self) -> &'static str {
        match self {
            FilePurity::PureServer => "only server",
            FilePurity::PureClient => "only client",
            FilePurity::PureBoth => "only both",
            FilePurity::Mixed => "mixed",
            FilePurity::NeitherOnly => "only neither",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileReport {
    pub file: String,
    pub purity: FilePurity,
    /// Body lines of its methods per side.
    pub lines: SideCounts,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaReport {
    /// The folder under `game/` (`blocks`, `network`...), or the file for `game.rs`.
    pub area: String,
    pub methods: SideCounts,
    pub lines: SideCounts,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurityTally {
    pub purity: FilePurity,
    pub files: usize,
    pub lines: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    pub methods: SideCounts,
    pub lines: SideCounts,
    pub fields: SideCounts,
    pub files: Vec<PurityTally>,
}

/// A method defined in more than one place (cfg twins); the census keeps the longer non-test one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Duplicate {
    pub method: String,
    pub files: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Census {
    pub totals: Totals,
    pub areas: Vec<AreaReport>,
    pub methods: Vec<MethodReport>,
    pub fields: Vec<FieldReport>,
    pub files: Vec<FileReport>,
    pub duplicates: Vec<Duplicate>,
    /// Root names the census looks for that no method has.
    pub missing_roots: Vec<String>,
}

/// What `Census::listing` prints one line per.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Listing {
    Methods,
    Fields,
    Files,
}

fn area_of(file: &str) -> String {
    let parts: Vec<&str> = file.split('/').collect();
    if parts[0] == "game" && parts.len() > 2 {
        parts[1].to_owned()
    } else {
        parts[0].to_owned()
    }
}

impl Census {
    pub(super) fn new(
        methods: Vec<MethodReport>,
        fields: Vec<FieldReport>,
        duplicates: Vec<Duplicate>,
        missing_roots: Vec<String>,
    ) -> Self {
        let mut totals = Totals {
            methods: SideCounts::default(),
            lines: SideCounts::default(),
            fields: SideCounts::default(),
            files: Vec::new(),
        };
        let mut areas: BTreeMap<String, AreaReport> = BTreeMap::new();
        let mut by_file: BTreeMap<&str, SideCounts> = BTreeMap::new();
        for method in &methods {
            totals.methods.add(method.side, 1);
            totals.lines.add(method.side, method.lines);
            let area = areas
                .entry(area_of(&method.file))
                .or_insert_with_key(|area| AreaReport {
                    area: area.clone(),
                    methods: SideCounts::default(),
                    lines: SideCounts::default(),
                });
            area.methods.add(method.side, 1);
            area.lines.add(method.side, method.lines);
            by_file
                .entry(&method.file)
                .or_default()
                .add(method.side, method.lines);
        }
        for field in &fields {
            totals.fields.add(field.side, 1);
        }
        let mut purity: BTreeMap<FilePurity, PurityTally> = BTreeMap::new();
        let mut files = Vec::new();
        for (file, lines) in by_file {
            let live: Vec<Side> = [Side::Server, Side::Client, Side::Both]
                .into_iter()
                .filter(|s| lines.get(*s) > 0)
                .collect();
            let kind = match live.as_slice() {
                [] => FilePurity::NeitherOnly,
                [Side::Server] => FilePurity::PureServer,
                [Side::Client] => FilePurity::PureClient,
                [Side::Both] => FilePurity::PureBoth,
                _ => FilePurity::Mixed,
            };
            let tally = purity.entry(kind).or_insert(PurityTally {
                purity: kind,
                files: 0,
                lines: 0,
            });
            tally.files += 1;
            tally.lines += lines.total();
            files.push(FileReport {
                file: file.to_owned(),
                purity: kind,
                lines,
            });
        }
        totals.files = purity.into_values().collect();
        let mut areas: Vec<AreaReport> = areas.into_values().collect();
        areas.sort_by(|a, b| {
            b.lines
                .total()
                .cmp(&a.lines.total())
                .then_with(|| a.area.cmp(&b.area))
        });
        Census {
            totals,
            areas,
            methods,
            fields,
            files,
            duplicates,
            missing_roots,
        }
    }

    /// The tables of the census as text.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let t = &self.totals;
        let _ = writeln!(
            out,
            "Methods of `impl Game` (non-test) by the side that reaches them"
        );
        let _ = writeln!(
            out,
            "{:<14}{:>9}{:>13}",
            "reached by", "methods", "body lines"
        );
        for side in Side::ALL {
            let _ = writeln!(
                out,
                "{:<14}{:>9}{:>13}",
                side.label(),
                group(t.methods.get(side)),
                group(t.lines.get(side))
            );
        }
        let _ = writeln!(
            out,
            "{:<14}{:>9}{:>13}",
            "total",
            group(t.methods.total()),
            group(t.lines.total())
        );
        let _ = writeln!(out, "\nBody lines by area");
        let _ = writeln!(
            out,
            "{:<16}{:>9}{:>9}{:>9}{:>9}",
            "area", "server", "client", "both", "neither"
        );
        for area in &self.areas {
            let l = &area.lines;
            let _ = writeln!(
                out,
                "{:<16}{:>9}{:>9}{:>9}{:>9}",
                area.area,
                group(l.server),
                group(l.client),
                group(l.both),
                group(l.neither)
            );
        }
        let _ = writeln!(out, "\nFiles by what their methods add up to");
        let _ = writeln!(out, "{:<14}{:>9}{:>13}", "files", "count", "body lines");
        for tally in &t.files {
            let _ = writeln!(
                out,
                "{:<14}{:>9}{:>13}",
                tally.purity.label(),
                group(tally.files),
                group(tally.lines)
            );
        }
        let _ = writeln!(
            out,
            "\nFields of `Game` ({}) by the side whose methods touch them",
            group(self.fields.len())
        );
        for side in Side::ALL {
            let _ = writeln!(out, "{:<14}{:>9}", side.label(), group(t.fields.get(side)));
        }
        if !self.missing_roots.is_empty() {
            let _ = writeln!(
                out,
                "\nRoots with no method: {}",
                self.missing_roots.join(", ")
            );
        }
        out
    }

    /// One line per method, field or file, largest first.
    pub fn listing(&self, what: Listing) -> String {
        let mut out = String::new();
        match what {
            Listing::Methods => {
                let mut rows: Vec<&MethodReport> = self.methods.iter().collect();
                rows.sort_by(|a, b| b.lines.cmp(&a.lines).then_with(|| a.name.cmp(&b.name)));
                for m in rows {
                    let _ = writeln!(
                        out,
                        "{:<8}{:>7}  {}  {}",
                        m.side.label().split(' ').next().unwrap_or(""),
                        m.lines,
                        m.name,
                        m.file
                    );
                }
            }
            Listing::Fields => {
                for f in &self.fields {
                    let _ = writeln!(
                        out,
                        "{:<8}{:>5} client{:>5} server{:>5} both  {}",
                        f.side.label().split(' ').next().unwrap_or(""),
                        f.client_methods,
                        f.server_methods,
                        f.both_methods,
                        f.name
                    );
                }
            }
            Listing::Files => {
                let mut rows: Vec<&FileReport> = self.files.iter().collect();
                rows.sort_by(|a, b| {
                    b.lines
                        .total()
                        .cmp(&a.lines.total())
                        .then_with(|| a.file.cmp(&b.file))
                });
                for f in rows {
                    let l = &f.lines;
                    let _ = writeln!(
                        out,
                        "{:>7}  server {:>6} client {:>6} both {:>6} neither {:>6}  {}",
                        l.total(),
                        l.server,
                        l.client,
                        l.both,
                        l.neither,
                        f.file
                    );
                }
            }
        }
        out
    }

    pub fn json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }
}

/// Digits grouped by thousands.
fn group(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (at, ch) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}
