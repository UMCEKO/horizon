//! Key mappings registered at runtime (the Java mods' boot capture) as `KeyRegistration`s for Controls and `options.txt`.
use crate::mod_list::{ALT, CONTROL, KeyMapping, SHIFT};
use mod_api::{ClientModifiers, KeyConflictContext, KeyRegistration};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, RwLock};

/// NeoForge's `KeyModifier`, as `options.txt` saves it after the key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Modifier {
    #[default]
    None,
    Control,
    ControlOrCommand,
    Shift,
    Alt,
}
impl Modifier {
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Control => "CONTROL",
            Self::ControlOrCommand => "CONTROL_OR_COMMAND",
            Self::Shift => "SHIFT",
            Self::Alt => "ALT",
        }
    }
    /// `KeyModifier.valueFromString`; a name it doesn't know is no modifier.
    pub fn parse(name: &str) -> Self {
        [
            Self::Control,
            Self::ControlOrCommand,
            Self::Shift,
            Self::Alt,
        ]
        .into_iter()
        .find(|m| m.name().eq_ignore_ascii_case(name))
        .unwrap_or(Self::None)
    }
    /// The modifier bits Controls binds with (`mod_list::{SHIFT, CONTROL, ALT}`).
    pub const fn bits(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Control | Self::ControlOrCommand => CONTROL,
            Self::Shift => SHIFT,
            Self::Alt => ALT,
        }
    }
    /// `KeyModifier.getActiveModifier`: the first of control, shift and alt in `bits`.
    pub const fn of_bits(bits: u32) -> Self {
        if bits & CONTROL != 0 {
            Self::Control
        } else if bits & SHIFT != 0 {
            Self::Shift
        } else if bits & ALT != 0 {
            Self::Alt
        } else {
            Self::None
        }
    }
}

/// NeoForge's `KeyConflictContext` by name.
pub fn parse_context(name: &str) -> Option<KeyConflictContext> {
    Some(match name {
        "UNIVERSAL" => KeyConflictContext::Universal,
        "GUI" => KeyConflictContext::Gui,
        "IN_GAME" => KeyConflictContext::InGame,
        _ => return None,
    })
}

/// A binding's `options.txt` text, `key` or `key:MODIFIER`, as its key and modifier bits.
pub fn parse_binding(text: &str) -> (String, u32) {
    match text.split_once(':') {
        Some((key, modifier)) => (key.to_owned(), Modifier::parse(modifier).bits()),
        None => (text.to_owned(), 0),
    }
}
/// A binding as `options.txt` saves it.
pub fn binding_text(key: &str, modifiers: u32) -> String {
    match Modifier::of_bits(modifiers) {
        Modifier::None => key.to_owned(),
        m => format!("{key}:{}", m.name()),
    }
}

/// `Category.label`: `Identifier.toLanguageKey("key.category")`.
pub fn category_lang_key(id: &str) -> String {
    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
    format!("key.category.{namespace}.{}", path.replace('/', "."))
}

/// The mods' mappings in the client's order (id = `KeyMapping` name), and the context of each of vanilla's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModKeys {
    pub keys: Vec<KeyRegistration>,
    pub vanilla: HashMap<String, KeyConflictContext>,
}
impl ModKeys {
    /// The boot's key list: per line `name`, `1` if vanilla's, category, key, modifier, context, tab separated.
    pub fn parse_export(text: &str) -> Result<Self, String> {
        let mut out = Self::default();
        for line in text.lines().filter(|l| !l.is_empty()) {
            let fields: Vec<&str> = line.split('\t').collect();
            let [name, vanilla, category, key, modifier, context] = fields[..] else {
                return Err(format!(
                    "a key mapping line has {} fields: {line}",
                    fields.len()
                ));
            };
            let context =
                parse_context(context).ok_or_else(|| format!("no conflict context {context}"))?;
            if vanilla == "1" {
                out.vanilla.insert(name.to_owned(), context);
            } else {
                out.keys.push(
                    KeyRegistration::new(name, key)
                        .label(name)
                        .modifiers(ClientModifiers::new(Modifier::parse(modifier).bits()))
                        .category(&category_lang_key(category))
                        .context(context),
                );
            }
        }
        Ok(out)
    }
    pub fn get(&self, name: &str) -> Option<&KeyRegistration> {
        self.keys.iter().find(|k| k.id == name)
    }
    /// The registrations as Controls' rows.
    pub fn mappings(&self) -> Vec<KeyMapping> {
        self.keys
            .iter()
            .map(|k| KeyMapping {
                id: k.id.clone(),
                label: k.label.clone(),
                key: k.key.clone(),
                modifiers: k.modifiers.bits,
                context: engine_context(k.context),
                category: k.category.clone(),
            })
            .collect()
    }
    /// The context vanilla mapping `name` conflicts in: the client's, else universal.
    pub fn vanilla_context(&self, name: &str) -> crate::mod_list::KeyConflictContext {
        self.vanilla
            .get(name)
            .map_or(crate::mod_list::KeyConflictContext::Universal, |c| {
                engine_context(*c)
            })
    }
}

pub fn engine_context(context: KeyConflictContext) -> crate::mod_list::KeyConflictContext {
    use crate::mod_list::KeyConflictContext as Engine;
    match context {
        KeyConflictContext::InGame => Engine::InGame,
        KeyConflictContext::Gui => Engine::Gui,
        // Universal and any context a newer API adds conflict everywhere, as NeoForge's default does.
        _ => Engine::Universal,
    }
}

/// The player's bindings of runtime mappings by name: key and modifier bits; one not here is on its default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bindings(BTreeMap<String, (String, u32)>);
impl Bindings {
    /// Mapping `name` as bound, else `default`.
    pub fn binding(&self, name: &str, default: (&str, u32)) -> (String, u32) {
        self.0
            .get(name)
            .cloned()
            .unwrap_or_else(|| (default.0.to_owned(), default.1))
    }
    pub fn set(&mut self, name: &str, key: &str, modifiers: u32) {
        self.0.insert(name.to_owned(), (key.to_owned(), modifiers));
    }
    pub fn reset(&mut self, name: &str) {
        self.0.remove(name);
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    /// From `options.txt`'s mod key lines.
    pub fn from_saved(lines: &[(String, String)]) -> Self {
        Self(
            lines
                .iter()
                .map(|(name, text)| (name.clone(), parse_binding(text)))
                .collect(),
        )
    }
    /// `Options.save`'s lines: every registered mapping, or the loaded lines while none are known yet.
    pub fn saved(&self) -> Vec<(String, String)> {
        if pending() {
            return self
                .0
                .iter()
                .map(|(name, (key, bits))| (name.clone(), binding_text(key, *bits)))
                .collect();
        }
        let table = mod_keys();
        let out: BTreeMap<String, String> = table
            .keys
            .iter()
            .map(|k| {
                let (key, bits) = self.binding(&k.id, (&k.key, k.modifiers.bits));
                (k.id.clone(), binding_text(&key, bits))
            })
            .collect();
        out.into_iter().collect()
    }
}

static MOD_KEYS: RwLock<Option<Arc<ModKeys>>> = RwLock::new(None);
const NO_MODS: u8 = 0;
const PENDING: u8 = 1;
const LOADED: u8 = 2;
static STATE: AtomicU8 = AtomicU8::new(NO_MODS);

thread_local! {
    static LOCAL: std::cell::RefCell<Option<Arc<ModKeys>>> = const { std::cell::RefCell::new(None) };
}

/// `f` with these as this thread's runtime registrations, so tests don't share the process's.
pub fn with_thread_mod_keys<R>(keys: ModKeys, f: impl FnOnce() -> R) -> R {
    let before = LOCAL.with(|l| l.replace(Some(Arc::new(keys))));
    let out = f();
    LOCAL.with(|l| *l.borrow_mut() = before);
    out
}

/// The runtime registrations, once the boot captured them (none before, and none without Java mods).
pub fn mod_keys() -> Arc<ModKeys> {
    if let Some(local) = LOCAL.with(|l| l.borrow().clone()) {
        return local;
    }
    MOD_KEYS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// Registers the captured mappings, replacing any registered before.
pub fn set_mod_keys(keys: ModKeys) {
    *MOD_KEYS.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(keys));
    STATE.store(LOADED, Ordering::Relaxed);
}

/// Mods that register mappings at runtime run but haven't yet: saved lines are kept until they do.
pub fn expect_mod_keys() {
    let _ = STATE.compare_exchange(NO_MODS, PENDING, Ordering::Relaxed, Ordering::Relaxed);
}

fn pending() -> bool {
    LOCAL.with(|l| l.borrow().is_none()) && STATE.load(Ordering::Relaxed) == PENDING
}

/// Whether mapping `name` is registered at runtime.
pub fn is_mod_key(name: &str) -> bool {
    mod_keys().get(name).is_some()
}

/// Whether `Options.save` writes mapping `name`: registered, or registrations aren't known yet.
pub fn saves_mod_key(name: &str) -> bool {
    pending() || is_mod_key(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn export() -> ModKeys {
        ModKeys::parse_export(
            "key.forward\t1\tminecraft:movement\tkey.keyboard.w\tNONE\tIN_GAME\n\
             key.test_mod.mode\t0\ttest_mod:keys\tkey.keyboard.n\tNONE\tIN_GAME\n\
             key.test_mod.menu\t0\ttest_mod:keys\tkey.keyboard.m\tCONTROL\tGUI\n\
             key.test_mod.any\t0\ttest_mod:more/keys\tkey.keyboard.n\tSHIFT\tUNIVERSAL\n",
        )
        .unwrap()
    }

    #[test]
    fn the_boot_export_becomes_registrations() {
        let keys = export();
        assert_eq!(keys.vanilla["key.forward"], KeyConflictContext::InGame);
        let ids: Vec<_> = keys.keys.iter().map(|k| k.id.as_str()).collect();
        assert_eq!(
            ids,
            ["key.test_mod.mode", "key.test_mod.menu", "key.test_mod.any"]
        );
        let menu = &keys.keys[1];
        assert_eq!(
            (menu.key.as_str(), menu.modifiers.bits, menu.context),
            ("key.keyboard.m", 2, KeyConflictContext::Gui)
        );
        assert_eq!(menu.label, "key.test_mod.menu");
        assert_eq!(keys.keys[0].category, "key.category.test_mod.keys");
        assert_eq!(keys.keys[2].category, "key.category.test_mod.more.keys");
        assert_eq!(keys.keys[2].modifiers.bits, 1);
        assert!(ModKeys::parse_export("key.a\t0\tx:y\tkey.keyboard.a\tNONE").is_err());
        assert!(ModKeys::parse_export("a\t0\tx:y\tkey.keyboard.a\tNONE\tSOMETIMES").is_err());
    }

    #[test]
    fn a_binding_saves_as_key_and_modifier() {
        assert_eq!(
            parse_binding("key.keyboard.k"),
            ("key.keyboard.k".into(), 0)
        );
        assert_eq!(
            parse_binding("key.keyboard.k:SHIFT"),
            ("key.keyboard.k".into(), 1)
        );
        assert_eq!(
            parse_binding("key.keyboard.k:control_or_command"),
            ("key.keyboard.k".into(), 2)
        );
        assert_eq!(
            parse_binding("key.keyboard.k:bogus"),
            ("key.keyboard.k".into(), 0)
        );
        assert_eq!(binding_text("key.keyboard.k", 0), "key.keyboard.k");
        assert_eq!(binding_text("key.keyboard.k", 4), "key.keyboard.k:ALT");
        assert_eq!(
            binding_text("key.keyboard.k", 1 | 2),
            "key.keyboard.k:CONTROL"
        );
    }

    #[test]
    fn saved_lines_are_the_registered_mappings() {
        with_thread_mod_keys(export(), || {
            let mut b = Bindings::from_saved(&[
                ("key.test_mod.mode".into(), "key.keyboard.k:SHIFT".into()),
                ("key.gone_mod.x".into(), "key.keyboard.x".into()),
            ]);
            b.set("key.test_mod.any", "key.keyboard.j", 0);
            assert_eq!(
                b.saved(),
                [
                    ("key.test_mod.any".to_owned(), "key.keyboard.j".to_owned()),
                    (
                        "key.test_mod.menu".to_owned(),
                        "key.keyboard.m:CONTROL".to_owned()
                    ),
                    (
                        "key.test_mod.mode".to_owned(),
                        "key.keyboard.k:SHIFT".to_owned()
                    ),
                ]
            );
        });
    }
}
