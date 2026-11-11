use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// The server begins teardown (NeoForge's `ServerStoppingEvent`):
    /// the start of vanilla's `MinecraftServer.stopServer`, with the worlds
    /// still loaded and the players still connected and not yet saved, so a
    /// mod can save its own data or tell them. `native_world_lifecycle`'s
    /// `WorldCloseFinal` follows once the worlds are saved. A server whose loop
    /// fails still fires this before the final save attempt and `native_server_stopped`.
    pub(in crate::game) fn native_server_stopping(&mut self, dedicated: bool) {
        self.java_hook_server_stopping();
        self.fire_server_stopping(dedicated);
    }
}
