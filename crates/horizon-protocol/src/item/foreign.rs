//! Data component types whose stream codec is a mod's code, read and written by whoever hosts it (`docs/java-mods.md`).
use super::{Component, component::VANILLA_TYPES};
use crate::{Error, Result, buf::Reader, registry::Ids};
use horizon_world::nbt::Tag;
use std::sync::{Arc, OnceLock, RwLock};

/// Each type's own codecs, by the type's registry name.
pub trait StreamCodecs: Send + Sync {
    /// One value from the start of `input`: the bytes it took, and its saved form if the type has a persistent codec.
    fn read(&self, name: &str, input: &[u8]) -> Option<(usize, Option<Tag>)>;
    /// What each type's stream codec writes for its saved value; None where it can't.
    fn write(&self, values: &[(&str, &Tag)]) -> Vec<Option<Arc<[u8]>>>;
}

static CODECS: OnceLock<Box<dyn StreamCodecs>> = OnceLock::new();
static NAMES: RwLock<Vec<String>> = RwLock::new(Vec::new());

pub fn install(codecs: Box<dyn StreamCodecs>) {
    let _ = CODECS.set(codecs);
}

/// The merged `minecraft:data_component_type`: built-in registries aren't synced, so server and client read the same file.
pub fn set_ids(ids: &Ids) {
    ids.names.clone_into(&mut NAMES.write().unwrap());
}

/// Whether component `kind` is read and written by a host's codecs.
pub fn owns(kind: i32) -> bool {
    kind >= VANILLA_TYPES && CODECS.get().is_some()
}

pub(super) fn read(kind: i32, r: &mut Reader) -> Result<Component> {
    let failed = || Error::new(format!("unsupported item component {kind}"));
    let name = usize::try_from(kind)
        .ok()
        .and_then(|i| NAMES.read().unwrap().get(i).cloned())
        .ok_or_else(failed)?;
    let codecs = CODECS.get().ok_or_else(failed)?;
    let (len, value) = codecs.read(&name, r.ahead()).ok_or_else(failed)?;
    Ok(Component::Foreign {
        bytes: r.bytes(len)?.to_vec(),
        value,
    })
}

/// The stream form of each saved value, as [`StreamCodecs::write`] gives it.
pub fn write(values: &[(&str, &Tag)]) -> Vec<Option<Arc<[u8]>>> {
    match CODECS.get() {
        Some(codecs) => codecs.write(values),
        None => values.iter().map(|_| None).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DelimitedSlot, Slot, Stack, Template};
    use super::*;
    use crate::buf::{Codec, Writer};
    use horizon_world::nbt::Tag;

    const MODE: i32 = VANILLA_TYPES;
    const TRANSIENT: i32 = VANILLA_TYPES + 1;

    struct Fake;
    impl StreamCodecs for Fake {
        fn read(&self, name: &str, input: &[u8]) -> Option<(usize, Option<Tag>)> {
            let len = usize::from(*input.first()?);
            let text = input.get(1..1 + len)?;
            let value = match name {
                "test:mode" => Some(Tag::String(std::str::from_utf8(text).ok()?.into())),
                "test:transient" => None,
                _ => return None,
            };
            Some((1 + len, value))
        }
        fn write(&self, values: &[(&str, &Tag)]) -> Vec<Option<Arc<[u8]>>> {
            values
                .iter()
                .map(|(name, value)| match (*name, value) {
                    ("test:mode", Tag::String(s)) => {
                        let s = s.to_string_lossy();
                        let mut out = vec![s.len() as u8];
                        out.extend(s.as_bytes());
                        Some(out.into())
                    }
                    _ => None,
                })
                .collect()
        }
    }

    fn setup() {
        install(Box::new(Fake));
        let mut names = vec![String::new(); VANILLA_TYPES as usize];
        names.extend(["test:mode".to_string(), "test:transient".to_string()]);
        set_ids(&Ids::from_names(names));
    }

    fn foreign(text: &str, value: Option<&str>) -> Component {
        let mut bytes = vec![text.len() as u8];
        bytes.extend(text.as_bytes());
        Component::Foreign {
            bytes,
            value: value.map(|v| Tag::String(v.into())),
        }
    }

    #[test]
    fn a_value_is_read_with_its_codec_and_what_follows_stays_readable() {
        setup();
        let mut stack = Stack::new(7, 1);
        stack.added.push((MODE, foreign("rotate", Some("rotate"))));
        stack.added.push((TRANSIENT, foreign("opaque", None)));
        stack
            .added
            .push((super::super::component::DAMAGE, Component::Int(3)));
        let mut w = Writer::new();
        Slot::write(&Some(stack), &mut w);
        let mut r = Reader::new(&w.data);
        let read = Slot::read(&mut r).unwrap().unwrap();
        assert_eq!(r.remaining(), 0);
        assert_eq!(
            read.component(MODE),
            Some(&foreign("rotate", Some("rotate")))
        );
        assert_eq!(read.component(TRANSIENT), Some(&foreign("opaque", None)));
        assert_eq!(read.damage(), 3);
    }

    #[test]
    fn a_value_goes_back_as_the_bytes_that_came() {
        setup();
        let mut stack = Stack::new(7, 1);
        stack.added.push((TRANSIENT, foreign("opaque", None)));
        let mut w = Writer::new();
        Template::write(&stack, &mut w);
        let mut again = Writer::new();
        Template::write(
            &Template::read(&mut Reader::new(&w.data)).unwrap(),
            &mut again,
        );
        assert_eq!(w.data, again.data);
    }

    #[test]
    fn a_creative_slot_carries_a_value_behind_its_length() {
        setup();
        let mut stack = Stack::new(7, 1);
        stack.added.push((MODE, foreign("wrench", Some("wrench"))));
        let mut w = Writer::new();
        DelimitedSlot::write(&Some(stack), &mut w);
        let read = DelimitedSlot::read(&mut Reader::new(&w.data))
            .unwrap()
            .unwrap();
        assert_eq!(
            read.component(MODE),
            Some(&foreign("wrench", Some("wrench")))
        );
    }

    #[test]
    fn a_type_the_codecs_refuse_fails_the_read() {
        setup();
        let mut r = Reader::new(&[0]);
        assert!(Component::read(VANILLA_TYPES + 9, &mut r).is_err());
    }

    #[test]
    fn saved_values_become_stream_bytes() {
        setup();
        let mode = Tag::String("wrench".into());
        let other = Tag::Int(1);
        let out = write(&[("test:mode", &mode), ("test:mode", &other)]);
        assert_eq!(out[0].as_deref(), Some(&b"\x06wrench"[..]));
        assert_eq!(out[1], None);
    }
}
