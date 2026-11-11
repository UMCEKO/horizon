use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// The server opened its worlds and is about to tick (NeoForge's
    /// `ServerStartedEvent`): every level loaded, no player joined. Where
    /// `WorldOpenReady` fired: the end of the open, before the tick loop.
    pub(in crate::game) fn native_server_started(&mut self, dedicated: bool) {
        self.java_hook_server_started();
        self.fire_server_started(dedicated);
    }
}
