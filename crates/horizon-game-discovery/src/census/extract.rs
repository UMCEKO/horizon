//! Reads the game's source: every `impl Game` method with what it calls and which fields it touches, per view.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use syn::{
    Attribute, Expr, ExprField, ExprMethodCall, ExprPath, Fields, ImplItem, Item, ItemImpl, Macro,
    Meta, Type,
    visit::{self, Visit},
};

use super::{View, guards};

/// Receivers a call edge is taken from: `self.name(..)`, `g.name(..)`.
const CALL_RECEIVERS: &[&str] = &["self", "game", "g", "this", "server", "client", "host_game"];
/// Receivers a field access is taken from.
const FIELD_RECEIVERS: &[&str] = &["self", "game", "g", "this"];

/// The method names called and the field names read on `Game` receivers, before filtering to real methods and fields.
#[derive(Debug, Default)]
pub struct Edges {
    pub calls: BTreeSet<String>,
    pub fields: BTreeSet<String>,
}

#[derive(Debug, Default)]
pub struct ViewEdges {
    pub client: Edges,
    pub server: Edges,
    pub both: Edges,
}

impl ViewEdges {
    pub fn of(&self, view: View) -> &Edges {
        match view {
            View::Client => &self.client,
            View::Server => &self.server,
            View::Both => &self.both,
        }
    }
}

#[derive(Debug)]
pub struct Method {
    /// Relative to the source folder, `game/world/persist.rs`.
    pub file: String,
    /// Lines of the body, braces included.
    pub lines: usize,
    /// The implemented trait's last path segment; none for an inherent method.
    pub trait_name: Option<String>,
    /// Lives in a test file or is a test.
    pub test: bool,
    pub edges: ViewEdges,
}

#[derive(Debug, Default)]
pub struct Parsed {
    /// Keyed by name, or `<Trait>::name` for a trait impl.
    pub methods: BTreeMap<String, Method>,
    /// Methods defined more than once (cfg twins): the other files.
    pub duplicates: BTreeMap<String, Vec<String>>,
    pub fields: Vec<String>,
}

pub fn parse_tree(src: &Path) -> Result<Parsed, String> {
    let mut files = Vec::new();
    let game_rs = src.join("game.rs");
    if !game_rs.is_file() {
        return Err(format!("{}: not found", game_rs.display()));
    }
    files.push(game_rs);
    let game_dir = src.join("game");
    if game_dir.is_dir() {
        collect_rs(&game_dir, &mut files)?;
    }
    files.sort();
    let mut parsed = Parsed::default();
    for path in files {
        let rel = path
            .strip_prefix(src)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let file = syn::parse_file(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if rel == "game.rs" {
            parsed.fields = game_fields(&file.items).ok_or("game.rs: no `struct Game`")?;
        }
        collect_items(&file.items, &rel, &mut parsed);
    }
    Ok(parsed)
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            collect_rs(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn game_fields(items: &[Item]) -> Option<Vec<String>> {
    for item in items {
        match item {
            Item::Struct(item) if item.ident == "Game" => {
                let Fields::Named(named) = &item.fields else {
                    return None;
                };
                return Some(
                    named
                        .named
                        .iter()
                        .filter_map(|f| f.ident.as_ref())
                        .map(ToString::to_string)
                        .collect(),
                );
            }
            Item::Mod(item) => {
                if let Some(found) = item
                    .content
                    .as_ref()
                    .and_then(|(_, items)| game_fields(items))
                {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("test")
            || matches!(&attr.meta, Meta::List(list) if list.path.is_ident("cfg") && list.tokens.to_string() == "test")
    })
}

fn is_test_path(rel: &str) -> bool {
    rel.contains("tests")
        || rel.ends_with("_test.rs")
        || rel.contains("fixture")
        || rel.contains("harness")
}

fn collect_items(items: &[Item], rel: &str, parsed: &mut Parsed) {
    for item in items {
        match item {
            Item::Mod(item) if !is_cfg_test(&item.attrs) => {
                if let Some((_, inner)) = &item.content {
                    collect_items(inner, rel, parsed);
                }
            }
            Item::Impl(item) if !is_cfg_test(&item.attrs) && is_game(&item.self_ty) => {
                collect_impl(item, rel, parsed)
            }
            _ => {}
        }
    }
}

fn is_game(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    let names: Vec<_> = path
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    matches!(
        names
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice(),
        ["Game"] | ["super", "Game"] | ["crate", "game", "Game"]
    )
}

fn collect_impl(item: &ItemImpl, rel: &str, parsed: &mut Parsed) {
    let trait_name = item
        .trait_
        .as_ref()
        .and_then(|(_, path, _)| path.segments.last())
        .map(|s| s.ident.to_string());
    for member in &item.items {
        let ImplItem::Fn(function) = member else {
            continue;
        };
        let name = function.sig.ident.to_string();
        let key = match &trait_name {
            Some(trait_name) => format!("<{trait_name}>::{name}"),
            None => name,
        };
        let braces = function.block.brace_token.span;
        let method = Method {
            file: rel.to_owned(),
            lines: braces.close().end().line - braces.open().start().line + 1,
            trait_name: trait_name.clone(),
            test: is_test_path(rel) || is_cfg_test(&function.attrs),
            edges: ViewEdges {
                client: edges_of(&function.block, View::Client),
                server: edges_of(&function.block, View::Server),
                both: edges_of(&function.block, View::Both),
            },
        };
        match parsed.methods.get(&key) {
            None => {
                parsed.methods.insert(key, method);
            }
            Some(old) => {
                // keep the non-test one, then the longer one
                let replace = (old.test && !method.test)
                    || (old.test == method.test && method.lines > old.lines);
                let other = if replace {
                    old.file.clone()
                } else {
                    method.file.clone()
                };
                parsed
                    .duplicates
                    .entry(key.clone())
                    .or_default()
                    .push(other);
                if replace {
                    parsed.methods.insert(key, method);
                }
            }
        }
    }
}

fn edges_of(block: &syn::Block, view: View) -> Edges {
    let mut walker = Walker {
        view,
        edges: Edges::default(),
    };
    walker.visit_block(block);
    walker.edges
}

struct Walker {
    view: View,
    edges: Edges,
}

/// A path or field named like a `Game` handle: `self`, `g`, and `level.game`.
fn receiver_is(expr: &Expr, names: &[&str]) -> bool {
    match expr {
        Expr::Path(path) => {
            path.qself.is_none() && names.iter().any(|name| path.path.is_ident(name))
        }
        Expr::Field(field) => {
            matches!(&field.member, syn::Member::Named(member) if names.iter().any(|name| member == name))
        }
        Expr::Paren(paren) => receiver_is(&paren.expr, names),
        _ => false,
    }
}

impl Walker {
    fn scan(&mut self, tokens: TokenStream) {
        let trees: Vec<TokenTree> = tokens.into_iter().collect();
        let punct_at = |at: usize, ch: char| matches!(trees.get(at), Some(TokenTree::Punct(p)) if p.as_char() == ch);
        for (at, tree) in trees.iter().enumerate() {
            match tree {
                TokenTree::Group(group) => self.scan(group.stream()),
                TokenTree::Ident(ident) => {
                    let ident = ident.to_string();
                    if CALL_RECEIVERS.contains(&ident.as_str()) && punct_at(at + 1, '.') {
                        let Some(TokenTree::Ident(member)) = trees.get(at + 2) else {
                            continue;
                        };
                        let called = matches!(trees.get(at + 3), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis)
                            || punct_at(at + 3, ':');
                        if called {
                            self.edges.calls.insert(member.to_string());
                        } else if FIELD_RECEIVERS.contains(&ident.as_str()) {
                            self.edges.fields.insert(member.to_string());
                        }
                    } else if (ident == "Self" || ident == "Game")
                        && punct_at(at + 1, ':')
                        && punct_at(at + 2, ':')
                        && let Some(TokenTree::Ident(name)) = trees.get(at + 3)
                    {
                        self.edges.calls.insert(name.to_string());
                    }
                }
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for Walker {
    fn visit_block(&mut self, block: &'ast syn::Block) {
        for stmt in &block.stmts {
            self.visit_stmt(stmt);
            if guards::ends_block(stmt, self.view) {
                break;
            }
        }
    }

    fn visit_expr_if(&mut self, branch: &'ast syn::ExprIf) {
        if guards::dead_for(&branch.cond, self.view) {
            self.visit_expr(&branch.cond);
            if let Some((_, other)) = &branch.else_branch {
                self.visit_expr(other);
            }
        } else if guards::live_for(&branch.cond, self.view) {
            self.visit_expr(&branch.cond);
            self.visit_block(&branch.then_branch);
        } else {
            visit::visit_expr_if(self, branch);
        }
    }

    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        if receiver_is(&call.receiver, CALL_RECEIVERS) {
            self.edges.calls.insert(call.method.to_string());
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_path(&mut self, path: &'ast ExprPath) {
        let segments = &path.path.segments;
        if path.qself.is_none()
            && segments.len() == 2
            && (segments[0].ident == "Self" || segments[0].ident == "Game")
        {
            self.edges.calls.insert(segments[1].ident.to_string());
        }
        visit::visit_expr_path(self, path);
    }

    fn visit_expr_field(&mut self, field: &'ast ExprField) {
        if let syn::Member::Named(name) = &field.member
            && receiver_is(&field.base, FIELD_RECEIVERS)
        {
            self.edges.fields.insert(name.to_string());
        }
        visit::visit_expr_field(self, field);
    }

    fn visit_macro(&mut self, mac: &'ast Macro) {
        self.scan(mac.tokens.clone());
    }
}
