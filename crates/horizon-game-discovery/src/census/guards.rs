//! Mode guards: conditions that hold on one side's `Game` only, so the other side never runs what they fence.
use syn::{BinOp, Expr, Local, Pat, Stmt, UnOp};

use super::View;

/// The side a guard is true on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Server,
    Client,
}

impl View {
    fn mode(self) -> Option<Mode> {
        match self {
            View::Server => Some(Mode::Server),
            View::Client => Some(Mode::Client),
            View::Both => None,
        }
    }

    fn opposite_mode(self) -> Option<Mode> {
        self.mode().map(|mode| match mode {
            Mode::Server => Mode::Client,
            Mode::Client => Mode::Server,
        })
    }
}

struct Row {
    receiver: &'static [&'static str],
    method: &'static str,
    negated: bool,
    mode: Mode,
}

const fn row(
    receiver: &'static [&'static str],
    method: &'static str,
    negated: bool,
    mode: Mode,
) -> Row {
    Row {
        receiver,
        method,
        negated,
        mode,
    }
}

const ROLE: &[&str] = &["options", "execution_role"];

/// `self.<receiver>.<method>()` (or its negation) is true on one side only.
const METHOD_GUARDS: &[Row] = &[
    row(&["remote"], "is_none", false, Mode::Server),
    row(&[], "is_logical_server", false, Mode::Server),
    row(ROLE, "is_logical_server", false, Mode::Server),
    row(&["serving"], "is_some", false, Mode::Server),
    row(&["options", "dedicated"], "is_some", false, Mode::Server),
    row(&["lan"], "is_some", false, Mode::Server),
    row(ROLE, "is_client", true, Mode::Server),
    row(&[], "is_dedicated_server", false, Mode::Server),
    row(&["remote"], "is_some", false, Mode::Client),
    row(ROLE, "is_client", false, Mode::Client),
    row(&[], "is_logical_server", true, Mode::Client),
    row(&["integrated"], "is_some", false, Mode::Client),
];

/// `let Some(..) = self.<field>` only matches on one side.
const OPTION_GUARDS: &[(&str, Mode)] = &[
    ("lan", Mode::Server),
    ("serving", Mode::Server),
    ("remote", Mode::Client),
    ("integrated", Mode::Client),
    ("renderer", Mode::Client),
    ("window", Mode::Client),
];

/// The fields between `self` and the end of a field chain: `self.options.dedicated` is `["options", "dedicated"]`.
pub fn field_path(expr: &Expr) -> Option<Vec<String>> {
    match expr {
        Expr::Path(path) if path.path.is_ident("self") => Some(Vec::new()),
        Expr::Paren(paren) => field_path(&paren.expr),
        Expr::Field(field) => {
            let syn::Member::Named(name) = &field.member else {
                return None;
            };
            let mut path = field_path(&field.base)?;
            path.push(name.to_string());
            Some(path)
        }
        _ => None,
    }
}

fn method_mode(expr: &Expr, negated: bool) -> Option<Mode> {
    let Expr::MethodCall(call) = strip(expr) else {
        return None;
    };
    if !call.args.is_empty() {
        return None;
    }
    let receiver = field_path(&call.receiver)?;
    METHOD_GUARDS
        .iter()
        .find(|row| {
            row.negated == negated
                && call.method == row.method
                && row
                    .receiver
                    .iter()
                    .copied()
                    .eq(receiver.iter().map(String::as_str))
        })
        .map(|row| row.mode)
}

fn option_mode(pat: &Pat, scrutinee: &Expr) -> Option<Mode> {
    let Pat::TupleStruct(pat) = pat else {
        return None;
    };
    if !pat.path.is_ident("Some") {
        return None;
    }
    let mut root = scrutinee;
    loop {
        root = match root {
            Expr::Reference(reference) => &reference.expr,
            Expr::Paren(paren) => &paren.expr,
            Expr::MethodCall(call) => &call.receiver,
            Expr::Try(attempt) => &attempt.expr,
            _ => break,
        };
    }
    let path = field_path(root)?;
    let [field] = path.as_slice() else {
        return None;
    };
    OPTION_GUARDS
        .iter()
        .find(|(name, _)| name == field)
        .map(|(_, mode)| *mode)
}

fn strip(expr: &Expr) -> &Expr {
    match expr {
        Expr::Paren(paren) => strip(&paren.expr),
        Expr::Group(group) => strip(&group.expr),
        other => other,
    }
}

/// The side a single condition (no `&&`) is true on, if it is a mode guard.
pub fn mode_of(cond: &Expr) -> Option<Mode> {
    match strip(cond) {
        Expr::Unary(unary) if matches!(unary.op, UnOp::Not(_)) => method_mode(&unary.expr, true),
        Expr::MethodCall(_) => method_mode(cond, false),
        Expr::Let(binding) => option_mode(&binding.pat, &binding.expr),
        _ => None,
    }
}

fn conjuncts<'a>(cond: &'a Expr, out: &mut Vec<&'a Expr>) {
    match strip(cond) {
        Expr::Binary(binary) if matches!(binary.op, BinOp::And(_)) => {
            conjuncts(&binary.left, out);
            conjuncts(&binary.right, out);
        }
        other => out.push(other),
    }
}

/// The condition can never be true on this view: one of its conjuncts is a guard of the other side.
pub fn dead_for(cond: &Expr, view: View) -> bool {
    let Some(opposite) = view.opposite_mode() else {
        return false;
    };
    let mut parts = Vec::new();
    conjuncts(cond, &mut parts);
    parts
        .into_iter()
        .any(|part| mode_of(part) == Some(opposite))
}

/// The condition is always true on this view: it is exactly a guard of this side.
pub fn live_for(cond: &Expr, view: View) -> bool {
    view.mode().is_some() && mode_of(cond) == view.mode()
}

/// Whether a statement ends the block on this view, so the statements after it never run.
pub fn ends_block(stmt: &Stmt, view: View) -> bool {
    match stmt {
        Stmt::Local(local) => let_else_diverges(local, view),
        Stmt::Expr(Expr::If(branch), _) => {
            live_for(&branch.cond, view)
                && branch.else_branch.is_none()
                && matches!(
                    branch.then_branch.stmts.last(),
                    Some(Stmt::Expr(Expr::Return(_), _))
                )
        }
        _ => false,
    }
}

fn let_else_diverges(local: &Local, view: View) -> bool {
    let Some(init) = &local.init else {
        return false;
    };
    init.diverge.is_some()
        && view.opposite_mode().is_some()
        && option_mode(&local.pat, &init.expr) == view.opposite_mode()
}
