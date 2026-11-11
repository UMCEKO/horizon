//! The player's own profile and the 3D skin layers model, for the server view's skins.
use super::{Game, OwnProfile};

impl Game<'_> {
    pub(super) fn poll_own_profile(&mut self) {
        let auth = self.auth();
        let Some(r) = self.remote.as_mut() else {
            return;
        };
        match &mut r.own_profile {
            OwnProfile::Idle => {
                let Some(auth) = auth else {
                    r.own_profile = OwnProfile::Done(None);
                    return;
                };
                let (send, recv) = std::sync::mpsc::channel();
                std::thread::Builder::new()
                    .name("Own profile".into())
                    .spawn(move || {
                        let _ = send.send(horizon_client::auth::own_textures(auth.as_ref()));
                    })
                    .ok();
                r.own_profile = OwnProfile::Fetching(recv);
            }
            OwnProfile::Fetching(recv) => {
                if let Ok(own) = recv.try_recv() {
                    r.own_profile = OwnProfile::Done(own);
                }
            }
            OwnProfile::Done(_) => {}
        }
    }
    /// `PlayerInfo.getSkin` for every player entity, including NPCs that are
    /// never in the tab list: resolve each profile's textures and give the
    /// level's player models their skin.
    /// The 3D skin layers model for a player (Options → Skin Customization,
    /// `threeDSkinLayers`): `kind`'s outer layer with one slab per painted
    /// texel, built once per (skin, slim) from the decoded image the skin
    /// downloader still holds (`horizon_client::mobs::skin_layers`). `None`
    /// while the option is off or the image is not in memory yet.
    pub(super) fn skin_3d_model(
        &self,
        kind: &std::sync::Arc<horizon_client::mobs::MobType>,
        skin: &str,
        slim: bool,
    ) -> Option<std::sync::Arc<horizon_client::mobs::Part>> {
        use horizon_client::mobs::skin_layers;
        if !self.settings.extras.three_d_skin_layers {
            return None;
        }
        let key = (skin.to_owned(), slim);
        if let Some(model) = self.skin_3d.borrow_mut().get(&key) {
            return Some(model);
        }
        let model_key = if slim { "player_slim#main" } else { "main" };
        let base = kind
            .layer(model_key)
            .cloned()
            .unwrap_or_else(|| kind.model.clone());
        let image = self.skin_downloads.borrow().image(skin)?.to_vec();
        let model = std::sync::Arc::new(skin_layers::extrude(
            &base,
            &image,
            skin_layers::SIZE,
            skin_layers::DEPTH,
        )?);
        self.skin_3d.borrow_mut().insert(key, model.clone());
        Some(model)
    }
}
