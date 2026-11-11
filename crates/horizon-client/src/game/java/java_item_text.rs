// game-module-cfg: feature java-mods
//! Names and tooltips of mods' items, which the Java client computes (`getHoverName`, `getTooltipLines`): asked once per stack and cached.
#![allow(dead_code)]

use super::java_hooks::JavaItemText;
use super::*;
use horizon_client::text::Text;
use horizon_java_host::wire::{In, Out};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const CAPACITY: usize = 2048;

/// A stack without its count, as its saved form, and the flags `getTooltipLines` reads.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(in crate::game) struct TextKey {
    nbt: Vec<u8>,
    advanced: bool,
    creative: bool,
}

type Answer = Option<Arc<JavaItemText>>;

#[derive(Default)]
struct Texts {
    entries: HashMap<TextKey, (u64, Answer)>,
    clock: u64,
    queued: Vec<TextKey>,
    asked: HashSet<TextKey>,
}

impl Texts {
    /// The answer for `key`, marking it used; asks for it once otherwise.
    fn get(&mut self, key: &TextKey) -> Answer {
        self.clock += 1;
        if let Some((used, answer)) = self.entries.get_mut(key) {
            *used = self.clock;
            return answer.clone();
        }
        if self.asked.len() < CAPACITY && self.asked.insert(key.clone()) {
            self.queued.push(key.clone());
        }
        None
    }

    fn store(&mut self, key: TextKey, answer: Answer) {
        self.asked.remove(&key);
        self.clock += 1;
        if self.entries.len() >= CAPACITY
            && !self.entries.contains_key(&key)
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, (used, _))| *used)
                .map(|(k, _)| k.clone())
        {
            self.entries.remove(&oldest);
        }
        self.entries.insert(key, (self.clock, answer));
    }
}

thread_local! {
    static TEXTS: RefCell<Texts> = RefCell::default();
}

fn key(stack: &ItemStack, advanced: bool, creative: bool) -> Option<TextKey> {
    let mut one = stack.clone();
    one.count = 1;
    Some(TextKey {
        nbt: super::java_mods::write_tag(horizon_items::item_nbt::item_to_nbt(&one))?,
        advanced,
        creative,
    })
}

/// The stacks to ask about, as the request and its keys.
pub(in crate::game) fn request() -> Option<(Vec<TextKey>, Vec<u8>)> {
    let keys = TEXTS.with_borrow_mut(|t| std::mem::take(&mut t.queued));
    if keys.is_empty() {
        return None;
    }
    let mut out = Out::default();
    out.int(keys.len() as i32);
    for key in &keys {
        out.bytes(&key.nbt);
        out.bool(key.advanced);
        out.bool(key.creative);
    }
    Some((keys, out.0))
}

fn component(json: &str) -> Option<Text> {
    let value = serde_json::from_str(json).ok()?;
    Some(horizon_client::component::to_text(&value, &mut |_| {
        Text::empty()
    }))
}

enum Reply {
    Answer(Answer),
    // The Java client has no level yet, so the stack is asked again later.
    Later,
}

fn parse(input: &mut In) -> Option<Reply> {
    match input.byte()? {
        0 => return Some(Reply::Answer(None)),
        2 => return Some(Reply::Later),
        _ => {}
    }
    let name = component(input.text()?)?;
    let mut lines = Vec::new();
    for _ in 0..input.int()? {
        lines.push(component(input.text()?)?);
    }
    Some(Reply::Answer(Some(Arc::new(JavaItemText { name, lines }))))
}

/// Stores the answers to `keys`; a malformed reply is dropped and its stacks stay on the components' text.
pub(in crate::game) fn apply(keys: Vec<TextKey>, reply: &[u8]) {
    let mut input = In(reply);
    let parsed = (|| {
        input.int()?;
        keys.iter()
            .map(|_| parse(&mut input))
            .collect::<Option<Vec<_>>>()
    })();
    TEXTS.with_borrow_mut(|texts| match parsed {
        Some(answers) => {
            for (key, reply) in keys.into_iter().zip(answers) {
                match reply {
                    Reply::Answer(answer) => texts.store(key, answer),
                    Reply::Later => {
                        texts.asked.remove(&key);
                    }
                }
            }
        }
        None => {
            log::warn!("Horizon: a malformed Java item text reply was dropped");
            for key in keys {
                texts.store(key, None);
            }
        }
    });
}

/// The Java client's request was lost: its stacks can be asked again.
pub(in crate::game) fn forget_pending() {
    TEXTS.with_borrow_mut(|t| {
        t.asked.clear();
        t.queued.clear();
    });
}

impl Game<'_> {
    /// A mod item's name and tooltip as its code gives them; None for other items and until the Java client has answered.
    pub(super) fn java_item_text(&self, stack: &ItemStack, creative: bool) -> Answer {
        if !horizon_java_host::get().is_some_and(|host| host.owns_item(&stack.item)) {
            return None;
        }
        let key = key(stack, self.options_advanced_tooltips(), creative)?;
        TEXTS.with_borrow_mut(|t| t.get(&key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(item: &str) -> ItemStack {
        ItemStack::new(item, 3)
    }

    #[test]
    fn key_ignores_the_count_and_follows_components_and_flags() {
        let plain = key(&stack("m:upgrade"), false, false).unwrap();
        let mut other = stack("m:upgrade");
        other.count = 1;
        assert_eq!(key(&other, false, false), Some(plain.clone()));
        other.data_mut().custom_name = Some(horizon_core::nbt::Tag::String("x".into()));
        assert_ne!(key(&other, false, false), Some(plain.clone()));
        assert_ne!(key(&stack("m:upgrade"), true, false), Some(plain.clone()));
        assert_ne!(key(&stack("m:upgrade"), false, true), Some(plain.clone()));
        assert_ne!(key(&stack("m:other"), false, false), Some(plain));
    }

    #[test]
    fn a_stack_is_asked_once_until_answered_and_a_changed_component_asks_again() {
        let mut texts = Texts::default();
        let first = key(&stack("m:upgrade"), false, false).unwrap();
        assert!(texts.get(&first).is_none());
        assert!(texts.get(&first).is_none());
        assert_eq!(texts.queued.len(), 1);
        let taken = std::mem::take(&mut texts.queued);
        texts.store(
            taken[0].clone(),
            Some(Arc::new(JavaItemText {
                name: Text::literal("Speed"),
                lines: Vec::new(),
            })),
        );
        assert_eq!(texts.get(&first).unwrap().name.plain(), "Speed");
        assert!(texts.queued.is_empty());
        let mut changed = stack("m:upgrade");
        changed.data_mut().custom_name = Some(horizon_core::nbt::Tag::String("x".into()));
        let second = key(&changed, false, false).unwrap();
        assert!(texts.get(&second).is_none());
        assert_eq!(texts.queued, vec![second]);
    }

    #[test]
    fn the_least_recently_used_entry_leaves_first() {
        let mut texts = Texts::default();
        let keys: Vec<_> = (0..CAPACITY)
            .map(|i| key(&stack(&format!("m:i{i}")), false, false).unwrap())
            .collect();
        for key in &keys {
            texts.store(key.clone(), None);
        }
        assert!(texts.entries.contains_key(&keys[0]));
        texts.get(&keys[0]);
        let extra = key(&stack("m:extra"), false, false).unwrap();
        texts.store(extra.clone(), None);
        assert_eq!(texts.entries.len(), CAPACITY);
        assert!(texts.entries.contains_key(&keys[0]));
        assert!(!texts.entries.contains_key(&keys[1]));
        assert!(texts.entries.contains_key(&extra));
    }

    #[test]
    fn a_reply_is_read_in_order_and_a_failed_stack_keeps_the_components_text() {
        let first = key(&stack("m:a"), false, false).unwrap();
        let second = key(&stack("m:b"), false, false).unwrap();
        let mut out = Out::default();
        out.int(2);
        out.bool(true);
        out.text(r#"{"text":"Speed Upgrade"}"#);
        out.int(1);
        out.text(r#"{"text":"line"}"#);
        out.bool(false);
        apply(vec![first.clone(), second.clone()], &out.0);
        TEXTS.with_borrow_mut(|t| {
            let name = t.get(&first).unwrap().name.plain();
            assert_eq!(name, "Speed Upgrade");
            assert_eq!(t.get(&first).unwrap().lines.len(), 1);
            assert!(t.get(&second).is_none());
            assert!(t.queued.is_empty());
        });
    }

    #[test]
    fn a_stack_asked_before_the_java_client_has_a_level_is_asked_again() {
        let early = key(&stack("m:early"), false, false).unwrap();
        TEXTS.with_borrow_mut(|t| assert!(t.get(&early).is_none()));
        let (keys, _) = request().unwrap();
        let mut out = Out::default();
        out.int(1);
        out.0.push(2);
        apply(keys, &out.0);
        TEXTS.with_borrow_mut(|t| {
            assert!(!t.entries.contains_key(&early));
            assert!(t.get(&early).is_none());
            assert_eq!(t.queued, vec![early.clone()]);
        });
    }
}
