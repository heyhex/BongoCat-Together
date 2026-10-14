//! Process-local presentation commands, independent of network permissions.
use crate::{SettingsModelKey, SettingsModelOrigin};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TogetherModelStatus {
    #[default]
    Idle,
    Loading,
    Ready,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TogetherLayoutSnapshot {
    pub model_status: TogetherModelStatus,
    pub retry_epoch: u64,
    pub revision: u64,
    pub model: SettingsModelKey,
    pub visible: bool,
    pub scale_percent: u16,
    pub swap_epoch: u64,
    pub reset_epoch: u64,
    pub follow_peer_model: bool,
    pub follow_peer_scale: bool,
}

impl Default for TogetherLayoutSnapshot {
    fn default() -> Self {
        Self {
            model_status: TogetherModelStatus::Idle,
            retry_epoch: 0,
            revision: 0,
            model: SettingsModelKey {
                id: "standard".into(),
                origin: SettingsModelOrigin::BuiltIn,
            },
            visible: true,
            scale_percent: 100,
            swap_epoch: 0,
            reset_epoch: 0,
            follow_peer_model: true,
            follow_peer_scale: true,
        }
    }
}

pub enum TogetherLayoutCommand {
    SetModelStatus(TogetherModelStatus),
    RetryModel,
    ApplyPeerDisplay(SettingsModelKey, u16),
    SelectModel(SettingsModelKey),
    SetVisible(bool),
    SetScale(u16),
    SwapPositions,
    ResetLayout,
}

#[derive(Clone, Default)]
pub struct TogetherLayoutClient(Arc<Mutex<TogetherLayoutSnapshot>>);

impl TogetherLayoutClient {
    pub fn snapshot(&self) -> TogetherLayoutSnapshot {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn command(&self, command: TogetherLayoutCommand) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        match command {
            TogetherLayoutCommand::SetModelStatus(status) => {
                if state.model_status == status {
                    return true;
                }
                state.model_status = status;
            }
            TogetherLayoutCommand::RetryModel => {
                state.retry_epoch += 1;
                state.model_status = TogetherModelStatus::Idle;
            }
            TogetherLayoutCommand::ApplyPeerDisplay(model, scale) => {
                if !(25..=400).contains(&scale) {
                    return false;
                }
                let changed = (state.follow_peer_model && state.model != model)
                    || (state.follow_peer_scale && state.scale_percent != scale);
                if !changed {
                    return true;
                }
                if state.follow_peer_model {
                    state.model = model;
                }
                if state.follow_peer_scale {
                    state.scale_percent = scale;
                }
            }
            TogetherLayoutCommand::SelectModel(model) => {
                if model.id.is_empty() || model.id.len() > 128 || model.id.contains(['/', '\\']) {
                    return false;
                }
                state.model = model;
                state.follow_peer_model = false;
            }
            TogetherLayoutCommand::SetVisible(visible) => state.visible = visible,
            TogetherLayoutCommand::SetScale(scale) if (25..=400).contains(&scale) => {
                state.scale_percent = scale;
                state.follow_peer_scale = false;
            }
            TogetherLayoutCommand::SetScale(_) => return false,
            TogetherLayoutCommand::SwapPositions => state.swap_epoch += 1,
            TogetherLayoutCommand::ResetLayout => {
                state.reset_epoch += 1;
                state.scale_percent = 100;
                state.visible = true;
                state.follow_peer_scale = false;
            }
        }
        state.revision += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_commands_are_bounded_and_reset_preserves_model() {
        let client = TogetherLayoutClient::default();
        assert!(!client.command(TogetherLayoutCommand::SetScale(0)));
        let model = SettingsModelKey {
            id: "keyboard".into(),
            origin: SettingsModelOrigin::BuiltIn,
        };
        assert!(client.command(TogetherLayoutCommand::SelectModel(model.clone())));
        client.command(TogetherLayoutCommand::SetScale(150));
        client.command(TogetherLayoutCommand::ResetLayout);
        let state = client.snapshot();
        assert_eq!(state.scale_percent, 100);
        assert_eq!(state.model, model);
        assert_eq!(state.reset_epoch, 1);
        client.command(TogetherLayoutCommand::ApplyPeerDisplay(
            SettingsModelKey {
                id: "gamepad".into(),
                origin: SettingsModelOrigin::BuiltIn,
            },
            200,
        ));
        assert_eq!(client.snapshot().model, model);
        assert_eq!(client.snapshot().scale_percent, 100);
    }
}
