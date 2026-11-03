//! Mods' data components on the wire: each type's own stream codec, in the JVM.
use super::java_mods::{read_tag, write_tag};
use horizon_java_host::wire::{In, Out};
use horizon_protocol::item::foreign::{self, StreamCodecs};
use horizon_world::nbt::Tag;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// An entity's state is rebuilt each tick to compare it, so each value gets one call into Java.
const REMEMBERED: usize = 4096;

#[derive(Default)]
struct Codecs {
    written: Mutex<HashMap<(String, Vec<u8>), Option<Arc<[u8]>>>>,
}

// `NbtIo` takes a compound at the root, and a value can be any tag.
fn boxed(value: &Tag) -> Option<Vec<u8>> {
    write_tag(Tag::compound([("v", value.clone())]))
}

fn unboxed(bytes: &[u8]) -> Option<Tag> {
    read_tag(bytes)?.get("v").cloned()
}

fn ask(saved: &[(&str, Vec<u8>)]) -> Option<Vec<Option<Arc<[u8]>>>> {
    let mut out = Out::default();
    out.int(saved.len() as i32);
    for (name, value) in saved {
        out.text(name);
        out.bytes(value);
    }
    let reply = horizon_java_host::get()?.request("component_write", &out.0)?;
    let mut reply = In(&reply);
    saved
        .iter()
        .map(|_| {
            Some(if reply.bool()? {
                Some(reply.bytes()?.into())
            } else {
                None
            })
        })
        .collect()
}

impl StreamCodecs for Codecs {
    fn read(&self, name: &str, input: &[u8]) -> Option<(usize, Option<Tag>)> {
        let mut out = Out::default();
        out.text(name);
        out.bytes(input);
        let reply = horizon_java_host::get()?.request("component_read", &out.0)?;
        let mut reply = In(&reply);
        let len = usize::try_from(reply.int()?).ok()?;
        let value = if reply.bool()? {
            Some(unboxed(reply.bytes()?)?)
        } else {
            None
        };
        Some((len, value))
    }

    fn write(&self, values: &[(&str, &Tag)]) -> Vec<Option<Arc<[u8]>>> {
        let saved: Vec<_> = values
            .iter()
            .map(|(name, value)| (*name, boxed(value).unwrap_or_default()))
            .collect();
        let known: Option<Vec<_>> = {
            let written = self.written.lock().unwrap();
            saved
                .iter()
                .map(|(name, value)| written.get(&(name.to_string(), value.clone())).cloned())
                .collect()
        };
        if let Some(known) = known {
            return known;
        }
        // Not under the lock: a codec is mod code, and it may take its time.
        let answers = ask(&saved).unwrap_or_else(|| vec![None; saved.len()]);
        let mut written = self.written.lock().unwrap();
        if written.len() >= REMEMBERED {
            written.clear();
        }
        for ((name, value), answer) in saved.iter().zip(&answers) {
            written.insert((name.to_string(), value.clone()), answer.clone());
        }
        answers
    }
}

pub(super) fn install() {
    if std::env::var_os("HORIZON_JAVA_RUN").is_some() {
        foreign::install(Box::new(Codecs::default()));
    }
}
