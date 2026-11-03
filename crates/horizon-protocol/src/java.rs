//! The channel a Java mods' server and its players' Java clients talk over, one frame a payload.
//!
//! The server announces its Java side with an empty [`CHANNEL`] payload in the configuration phase; a client
//! with Java mods of its own answers with the same channel and one byte, 1. From then on each frame is the
//! packet a Java client and a Java server would have exchanged in one JVM, written by Minecraft's own protocols.

/// The custom payload channel, both ways.
pub const CHANNEL: &str = "horizon:java";
