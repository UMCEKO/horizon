//! The owner link: the closed set of requests the window sends its integrated server thread.
use super::{Dim, Game, integrated::ServerOptions, lan::LanSettings, lan_screen::Snapshot};
use horizon_assets::resource_source::ResourceSource;
use horizon_client::datapack::DataPack;
use horizon_mod_package::{manifest::MissingJar, selection::SelectedPackage};
use horizon_protocol::connection::Pipe;
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex, mpsc::SyncSender},
};

/// Where the server puts the answer to a request the window waits for.
pub(super) type Reply<T> = SyncSender<T>;

/// Work the window hands its server thread between two ticks (`MinecraftServer.execute`).
pub(super) enum OwnerRequest {
    /// `IntegratedServer.publishServer`.
    Publish(LanSettings, Reply<Result<u16, String>>),
    /// `IntegratedServer.unpublishServer`: whether it was published.
    Unpublish(Reply<bool>),
    /// `horizon-ctl lan status`.
    LanStatus(Reply<serde_json::Value>),
    /// New command access and game mode forcing settings of the open world.
    LanApplySettings {
        guest_commands: bool,
        force: bool,
        done: Reply<()>,
    },
    /// The world options screen's state.
    ServerOptions(Reply<ServerOptions>),
    /// The world options screen's changes, from `before` to `wanted`.
    ApplyWorldOptions {
        before: Snapshot,
        wanted: Snapshot,
        done: Reply<()>,
    },
    /// Far terrain for a client in `dim`.
    LodSource(
        Dim,
        Reply<Option<Arc<dyn horizon_client::streaming::LodSource>>>,
    ),
    /// A splitscreen player (offline uuid) joins through memory.
    JoinSplit(u128, Reply<Option<Pipe>>),
    /// A splitscreen player (offline uuid) left.
    LeaveSplit(u128, Reply<()>),
    /// The pregen commands are on or off.
    SetPregen(bool),
    /// A live content mod switch: the data packs reload from the new root.
    ReloadMods(Box<ModsReload>),
    /// Transitional: the requests not typed yet.
    Closure(Box<dyn FnOnce(&mut Game<'_>) + Send>),
}

/// What a live content mod switch changes on the server (`Game::mods_reloaded`).
pub(super) struct ModsReload {
    pub root: std::path::PathBuf,
    pub data_source: ResourceSource,
    pub data_changes: ResourceSource,
    pub additional_data: Arc<[Arc<DataPack>]>,
    pub enabled: BTreeSet<String>,
    pub selected: Arc<[SelectedPackage]>,
    pub refused: Arc<Mutex<Vec<MissingJar>>>,
}

impl Game<'_> {
    /// Answers one request of the window, on the server thread.
    pub(super) fn serve_owner_request(&mut self, request: OwnerRequest) {
        match request {
            OwnerRequest::Publish(settings, reply) => {
                let _ = reply.send(self.publish(settings));
            }
            OwnerRequest::Unpublish(reply) => {
                let _ = reply.send(self.unpublish());
            }
            OwnerRequest::LanStatus(reply) => {
                let _ = reply.send(self.lan_status());
            }
            OwnerRequest::LanApplySettings {
                guest_commands,
                force,
                done,
            } => {
                self.lan_apply_settings(guest_commands, force);
                let _ = done.send(());
            }
            OwnerRequest::ServerOptions(reply) => {
                let _ = reply.send(self.server_options());
            }
            OwnerRequest::ApplyWorldOptions {
                before,
                wanted,
                done,
            } => {
                self.apply_world_options(&before, &wanted);
                let _ = done.send(());
            }
            OwnerRequest::LodSource(dim, reply) => {
                let _ = reply.send(self.lod_source_in(dim));
            }
            OwnerRequest::JoinSplit(uuid, reply) => {
                let _ = reply.send(self.join_split_seat(uuid));
            }
            OwnerRequest::LeaveSplit(uuid, done) => {
                if let Some(host) = self.lan.as_mut() {
                    host.hub.ops.remove(&uuid);
                }
                let _ = done.send(());
            }
            OwnerRequest::SetPregen(enabled) => self.set_pregen_commands(enabled),
            OwnerRequest::ReloadMods(reload) => self.apply_mods_reload(*reload),
            OwnerRequest::Closure(task) => task(self),
        }
    }

    /// A splitscreen seat gets the owner's permissions and a connection through memory.
    fn join_split_seat(&mut self, uuid: u128) -> Option<Pipe> {
        let host = self.lan.as_mut()?;
        let owner = host.owner.as_ref()?.uuid;
        let level = host.hub.ops.get(&owner).copied().unwrap_or(0);
        host.hub.ops.insert(uuid, level);
        host.hub.lan.connect_local().ok()
    }

    /// The command tree shows the pregen commands only while they are on.
    fn set_pregen_commands(&mut self, enabled: bool) {
        use horizon_client::command::vanilla;
        if self.ui.dispatcher.0.find("pregen").is_some() != enabled {
            vanilla::set_horizon_commands(enabled);
            self.ui.dispatcher =
                super::chat::SessionDispatcher(Arc::new(vanilla::session_dispatcher(
                    &self.level.rule_registry,
                    &self.level.enabled_features,
                )));
            self.restore_native_commands();
            self.resend_commands_filtered();
        }
    }
}
