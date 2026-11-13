// game-module-cfg: feature java-mods
//! Mods' data components on the wire: each type's own stream codec, in the JVM.
use super::java_mods::{read_tag, write_tag};
use horizon_client::net::JavaLink;
use horizon_core::nbt::Tag;
use horizon_java_host::wire::{In, Out};
use horizon_protocol::item::foreign::{self, StreamCodecs};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// An entity's state is rebuilt each tick to compare it, so each value gets one call into Java.
const REMEMBERED: usize = 4096;

#[derive(Default)]
struct Codecs {
    written: Mutex<Written>,
}

/// Java's encodings by component type, then by saved value: looked up by borrowed keys, so a hit allocates nothing.
#[derive(Default)]
struct Written {
    by_type: HashMap<String, HashMap<Vec<u8>, Option<Arc<[u8]>>>>,
    len: usize,
}

impl Written {
    fn get(&self, name: &str, value: &[u8]) -> Option<Option<Arc<[u8]>>> {
        self.by_type.get(name)?.get(value).cloned()
    }

    fn insert(&mut self, name: &str, value: &[u8], answer: Option<Arc<[u8]>>) {
        if self.len >= REMEMBERED {
            self.by_type.clear();
            self.len = 0;
        }
        let values = match self.by_type.get_mut(name) {
            Some(values) => values,
            None => self.by_type.entry(name.to_owned()).or_default(),
        };
        if values.insert(value.to_vec(), answer).is_none() {
            self.len += 1;
        }
    }
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
    // The protocol codec has no `&mut Game` in reach, so no native it reaches may find one.
    let host = horizon_java_host::get()?;
    let reply =
        horizon_java_host::bridge::without_active(|| host.request("component_write", &out.0))?;
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
                .map(|(name, value)| written.get(name, value))
                .collect()
        };
        if let Some(known) = known {
            return known;
        }
        // Not under the lock: a codec is mod code, and it may take its time.
        let answers = ask(&saved).unwrap_or_else(|| vec![None; saved.len()]);
        let mut written = self.written.lock().unwrap();
        for ((name, value), answer) in saved.iter().zip(&answers) {
            written.insert(name, value, answer.clone());
        }
        answers
    }
}

/// The font table the window prepared for the Java client, which it starts with.
static FONT: Mutex<Option<Vec<u8>>> = Mutex::new(None);

pub(super) fn prepare_font(table: Vec<u8>) {
    *FONT.lock().unwrap() = Some(table);
}

/// The window joined its own server: the Java client starts and runs its configuration, before any play packet.
fn java_client_login(profile: u128, name: &str) {
    connect("client_connect", profile, name, "");
}

/// A server in another process has a Java side: its configuration, then its play packets, come to the Java client over the connection.
fn java_link(link: JavaLink<'_>) {
    match link {
        JavaLink::Connect {
            profile,
            name,
            address,
        } => connect("client_connect_remote", profile, name, address),
        JavaLink::Frame(frame) => {
            let Some(host) = horizon_java_host::get() else {
                return;
            };
            let mut out = Out::default();
            out.bytes(frame);
            host.request("client_remote_frame", &out.0);
        }
    }
}

fn connect(op: &str, profile: u128, name: &str, address: &str) {
    let Some(host) = horizon_java_host::get() else {
        return;
    };
    if let Some(font) = FONT.lock().unwrap().take() {
        host.request("client_font", &font);
    }
    let mut out = Out::default();
    out.long((profile >> 64) as i64);
    out.long(profile as i64);
    out.text(name);
    if op == "client_connect_remote" {
        out.text(address);
    }
    host.request(op, &out.0);
}

pub(super) fn install() {
    if !horizon_java_host::configured() {
        return;
    }
    horizon_client::net::on_local_login(java_client_login);
    horizon_client::net::on_java_link(java_link);
    foreign::install(Box::new(Codecs::default()));
}

#[cfg(test)]
mod tests {
    #[test]
    fn without_a_run_folder_no_jvm_starts_and_no_hook_is_installed() {
        horizon_java_host::set_run(None);
        super::super::java_hooks::java_hook_start(None, false).unwrap();
        super::install();
        assert!(!horizon_java_host::configured());
        assert!(!horizon_client::net::java_link_installed());
        assert!(horizon_java_host::get().is_none());
    }

    #[test]
    fn remembered_encodings_stay_bounded_and_keyed_by_type_and_value() {
        let mut written = super::Written::default();
        let answer: std::sync::Arc<[u8]> = std::sync::Arc::from(&[7u8][..]);
        written.insert("mod:a", &[1], Some(answer.clone()));
        written.insert("mod:b", &[1], None);
        assert_eq!(written.get("mod:a", &[1]), Some(Some(answer)));
        assert_eq!(written.get("mod:b", &[1]), Some(None));
        assert_eq!(written.get("mod:a", &[2]), None);
        for i in 0..super::REMEMBERED as u32 {
            written.insert("mod:c", &i.to_be_bytes(), None);
        }
        assert!(written.len <= super::REMEMBERED);
        assert_eq!(written.get("mod:a", &[1]), None);
    }
}
