// game-module-cfg: feature java-mods
//! Mods' data components on the wire: each type's own stream codec, in the JVM.
#![allow(dead_code)]
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
    horizon_client::net::on_local_login(java_client_login);
    horizon_client::net::on_java_link(java_link);
    if std::env::var_os("HORIZON_JAVA_RUN").is_some() {
        foreign::install(Box::new(Codecs::default()));
    }
}
