//! Friend-window owner. Tick outside GPUI borrows, like the main overlay.
use bongocat_app::TogetherCat;
use bongocat_network::{NetworkClient, NetworkSnapshot, Phase};
use bongocat_overlay::{OverlaySessionOptions, OverlayWindowBounds, ProductOverlaySession};
use bongocat_runtime::{OverlaySettings, RuntimeCommand, RuntimeOwner};
use bongocat_ui_protocol::{
    SettingsModelKey, TogetherLayoutClient, TogetherLayoutCommand, TogetherModelStatus,
};
use std::{path::PathBuf, thread::JoinHandle, time::Duration};

type ModelLoader = (SettingsModelKey, JoinHandle<Result<TogetherCat, String>>);

pub(crate) struct TogetherOverlay {
    root: PathBuf,
    layout: TogetherLayoutClient,
    loader: Option<ModelLoader>,
    cat: Option<RuntimeOwner>,
    overlay: Option<ProductOverlaySession>,
    requested: Option<SettingsModelKey>,
    active: Option<SettingsModelKey>,
    failed: bool,
    ticks: u64,
    applied_revision: Option<u64>,
    reset_epoch: u64,
    saved_bounds: Option<OverlayWindowBounds>,
    observer_client: Option<NetworkClient>,
    peer_display: Option<bongocat_network::PeerInfo>,
    retry_epoch: u64,
}

impl TogetherOverlay {
    pub fn new(root: PathBuf, layout: TogetherLayoutClient) -> Self {
        Self {
            root,
            layout,
            loader: None,
            cat: None,
            overlay: None,
            requested: None,
            active: None,
            failed: false,
            ticks: 0,
            applied_revision: None,
            reset_epoch: 0,
            saved_bounds: None,
            observer_client: None,
            peer_display: None,
            retry_epoch: 0,
        }
    }

    pub fn tick(
        &mut self,
        snapshot: Option<&NetworkSnapshot>,
        client: Option<&NetworkClient>,
        local: Option<OverlayWindowBounds>,
    ) -> Result<(), String> {
        let result = self.tick_inner(snapshot, client, local);
        if result.is_err() {
            self.failed = true;
            self.layout.command(TogetherLayoutCommand::SetModelStatus(
                TogetherModelStatus::Failed,
            ));
        }
        result
    }

    fn tick_inner(
        &mut self,
        snapshot: Option<&NetworkSnapshot>,
        client: Option<&NetworkClient>,
        local: Option<OverlayWindowBounds>,
    ) -> Result<(), String> {
        let connected = snapshot.is_some_and(|s| s.phase == Phase::Connected);
        if !connected {
            self.peer_display = None;
        }
        if let Some(info) = snapshot.and_then(|s| s.peer_info.as_ref())
            && self.peer_display.as_ref() != Some(info)
        {
            // Resolve only a catalog/store ID. No remote path, resource or
            // arbitrary file is accepted; missing assets use the preset cat.
            let model = SettingsModelKey {
                id: if info.built_in {
                    info.model_id.clone()
                } else {
                    "standard".into()
                },
                origin: bongocat_ui_protocol::SettingsModelOrigin::BuiltIn,
            };
            self.layout.command(
                bongocat_ui_protocol::TogetherLayoutCommand::ApplyPeerDisplay(
                    model,
                    info.scale_percent,
                ),
            );
            self.peer_display = Some(info.clone());
        }
        let display = self.layout.snapshot();
        if display.retry_epoch != self.retry_epoch {
            self.retry_epoch = display.retry_epoch;
            self.failed = false;
        }
        if !connected {
            self.layout.command(TogetherLayoutCommand::SetModelStatus(
                TogetherModelStatus::Idle,
            ));
            if self.overlay.is_some() || self.cat.is_some() {
                self.clear()?;
            }
            self.failed = false;
        }
        if self.requested.as_ref() != Some(&display.model) {
            self.failed = false;
            if self.active.as_ref() == Some(&display.model) {
                self.layout.command(TogetherLayoutCommand::SetModelStatus(
                    TogetherModelStatus::Ready,
                ));
            }
        }
        if self
            .loader
            .as_ref()
            .is_some_and(|(_, loader)| loader.is_finished())
        {
            let (key, loader) = self.loader.take().expect("loader exists");
            let loaded = loader.join().map_err(|_| "friend loader failed")?;
            match loaded {
                Ok(cat) if connected && key == display.model => {
                    let TogetherCat { runtime, frames } = cat;
                    let bounds = self.window_bounds().or(self.saved_bounds);
                    let options = OverlaySessionOptions {
                        window_bounds: bounds,
                        click_through: false,
                        scale_percent: display.scale_percent,
                        ..Default::default()
                    };
                    // Prepare and show the candidate first. On GPU failure the
                    // current cat stays live with its own runtime and window.
                    match ProductOverlaySession::start_remote(runtime.client(), frames, options) {
                        Ok(mut overlay) => {
                            if bounds.is_none()
                                && let Some(local) = local
                            {
                                overlay
                                    .place_beside(local)
                                    .map_err(|_| "friend initial position failed")?;
                            }
                            self.clear()?;
                            if let Some(client) = client {
                                client.set_remote_observer(Some(std::sync::Arc::new(
                                    bongocat_app::TogetherRemoteInput::new(
                                        runtime.input_producer(),
                                        runtime.cursor_producer(),
                                    ),
                                )));
                                self.observer_client = Some(client.clone());
                            }
                            self.overlay = Some(overlay);
                            self.cat = Some(runtime);
                            self.active = Some(key);
                            self.applied_revision = None;
                            self.layout.command(TogetherLayoutCommand::SetModelStatus(
                                TogetherModelStatus::Ready,
                            ));
                        }
                        Err(_) => {
                            self.failed = true;
                            return Err(
                                "friend window failed to start; previous model preserved".into()
                            );
                        }
                    }
                }
                Ok(cat) => {
                    let _ = cat.runtime.shutdown(Duration::from_secs(2));
                }
                Err(error) => {
                    self.failed = true;
                    return Err(error);
                }
            }
        }
        if connected
            && self.loader.is_none()
            && self.active.as_ref() != Some(&display.model)
            && !self.failed
        {
            let root = self.root.clone();
            let selected = display.clone();
            self.requested = Some(display.model.clone());
            self.layout.command(TogetherLayoutCommand::SetModelStatus(
                TogetherModelStatus::Loading,
            ));
            self.loader = Some((
                display.model.clone(),
                std::thread::Builder::new()
                    .name("bongocat-friend-model".into())
                    .spawn(move || TogetherCat::prepare(&root, &selected))
                    .map_err(|_| "friend loader unavailable")?,
            ));
        }
        if self.applied_revision != Some(display.revision)
            && let Some(runtime) = self.cat.as_ref()
        {
            let client = runtime.client();
            client
                .send(RuntimeCommand::SetOverlaySettings(OverlaySettings {
                    click_through: false,
                    scale_percent: display.scale_percent,
                    ..Default::default()
                }))
                .map_err(|_| "friend display update failed")?;
            client
                .send(RuntimeCommand::SetOverlayVisible(display.visible))
                .map_err(|_| "friend visibility update failed")?;
            self.applied_revision = Some(display.revision);
        }
        if display.reset_epoch != self.reset_epoch
            && let (Some(local), Some(overlay)) = (local, self.overlay.as_mut())
        {
            overlay
                .place_beside(local)
                .map_err(|_| "friend layout reset failed")?;
            self.reset_epoch = display.reset_epoch;
        }
        if let Some(overlay) = self.overlay.as_mut()
            && overlay.tick().is_err()
        {
            self.failed = true;
            self.clear()?;
            return Err("friend rendering failed".into());
        }
        if self.overlay.is_some() {
            self.ticks += 1;
        }
        Ok(())
    }

    fn clear(&mut self) -> Result<(), String> {
        self.saved_bounds = self.window_bounds().or(self.saved_bounds);
        if let Some(client) = self.observer_client.take() {
            client.set_remote_observer(None);
        }
        if let Some(overlay) = self.overlay.as_mut() {
            overlay
                .stop_input()
                .map_err(|_| "friend input teardown failed")?;
        }
        if let Some(cat) = self.cat.take() {
            cat.shutdown(Duration::from_secs(2))
                .map_err(|_| "friend runtime teardown failed")?;
        }
        if let Some(overlay) = self.overlay.take() {
            overlay
                .finish_after_runtime_shutdown()
                .map_err(|_| "friend window teardown failed")?;
        }
        self.active = None;
        self.applied_revision = None;
        Ok(())
    }

    pub fn shutdown(&mut self) {
        let _ = self.clear();
        if let Some((_, loader)) = self.loader.take()
            && let Ok(Ok(cat)) = loader.join()
        {
            let _ = cat.runtime.shutdown(Duration::from_secs(2));
        }
    }
    pub fn set_position(&mut self, x: i32, y: i32) -> Result<(), String> {
        self.overlay
            .as_mut()
            .ok_or("friend window unavailable")?
            .set_position(x, y)
            .map_err(|_| "friend position update failed".into())
    }
    pub fn smoke_runtime(&self) -> Option<bongocat_runtime::RuntimeClient> {
        self.cat.as_ref().map(|cat| cat.client())
    }
    pub fn smoke_frame_count(&self) -> u64 {
        self.ticks
    }
    pub fn window_bounds(&self) -> Option<OverlayWindowBounds> {
        self.overlay
            .as_ref()
            .and_then(|overlay| overlay.window_bounds().ok())
    }
}

impl Drop for TogetherOverlay {
    fn drop(&mut self) {
        self.shutdown();
    }
}
