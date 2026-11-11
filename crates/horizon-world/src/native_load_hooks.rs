//! Callbacks into the executable for data-loading native trait mods.
use crate::{
    loot::{LootContext, LootStack},
    native_load_context::LoadContext,
};
use horizon_core::random::RandomState;
use serde_json::{Map, Value};
use std::sync::OnceLock;

/// Data-loading hooks, installed before any data loads. Earlier loads see no mods.
pub struct NativeLoadHooks {
    pub loot_table_load: fn(&LoadContext, &str, &mut Value),
    pub modify_default_components: fn(&LoadContext, &str, &mut Map<String, Value>),
    pub loot_function: fn(&str, &Value, &LootContext<'_>, &mut LootStack),
    pub loot_condition: fn(&str, &Value, &LootContext<'_>, &mut bool),
    /// Whether a top-level roll of this table is handed to `loot_rolled`; false skips it at no cost.
    pub loot_wanted: fn(&str) -> bool,
    /// After a top-level roll and splitter, before a chest's shuffle; it may advance the random state.
    pub loot_rolled: fn(
        &str,
        &LootContext<'_>,
        Option<&LootStack>,
        &mut Option<RandomState>,
        &mut Vec<LootStack>,
    ),
}

static HOOKS: OnceLock<NativeLoadHooks> = OnceLock::new();

/// Install the process-wide callbacks once; a second installation keeps the first.
pub fn install(hooks: NativeLoadHooks) -> Result<(), NativeLoadHooks> {
    HOOKS.set(hooks)
}

pub(crate) fn loot_table_load(context: &LoadContext, id: &str, table: &mut Value) {
    if let Some(hooks) = HOOKS.get() {
        (hooks.loot_table_load)(context, id, table);
    }
}

pub(crate) fn modify_default_components(
    context: &LoadContext,
    item: &str,
    components: &mut Map<String, Value>,
) {
    if let Some(hooks) = HOOKS.get() {
        (hooks.modify_default_components)(context, item, components);
    }
}

pub(crate) fn loot_function(
    kind: &str,
    data: &Value,
    ctx: &LootContext<'_>,
    stack: &mut LootStack,
) {
    if let Some(hooks) = HOOKS.get() {
        (hooks.loot_function)(kind, data, ctx, stack);
    }
}

pub(crate) fn loot_condition(kind: &str, data: &Value, ctx: &LootContext<'_>, holds: &mut bool) {
    if let Some(hooks) = HOOKS.get() {
        (hooks.loot_condition)(kind, data, ctx, holds);
    }
}

/// Whether the installed hooks want a top-level roll of `table`; false when none are installed.
pub fn loot_wanted(table: &str) -> bool {
    HOOKS.get().is_some_and(|hooks| (hooks.loot_wanted)(table))
}

pub(crate) fn loot_rolled(
    table: &str,
    ctx: &LootContext<'_>,
    tool: Option<&LootStack>,
    state: &mut Option<RandomState>,
    items: &mut Vec<LootStack>,
) {
    if let Some(hooks) = HOOKS.get() {
        (hooks.loot_rolled)(table, ctx, tool, state, items);
    }
}
