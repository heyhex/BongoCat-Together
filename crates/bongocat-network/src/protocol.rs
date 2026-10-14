use crate::NetworkError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub(crate) const VERSION: u16 = 1;
pub(crate) const MAX_FRAME: usize = 8192;
pub(crate) const MAX_TEXT_CHARS: usize = 512;

/// Display hints only. Model assets are never sent or downloaded implicitly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerInfo {
    pub model_id: String,
    pub built_in: bool,
    pub scale_percent: u16,
}

impl Default for PeerInfo {
    fn default() -> Self {
        Self {
            model_id: "standard".into(),
            built_in: true,
            scale_percent: 100,
        }
    }
}

impl PeerInfo {
    pub(crate) fn validate(&self) -> Result<(), NetworkError> {
        if self.model_id.is_empty()
            || self.model_id.len() > 64
            || !self
                .model_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            || !(25..=400).contains(&self.scale_percent)
        {
            return Err(NetworkError::Protocol);
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MouseDatagram {
    pub share_revision: u64,
    pub version: u16,
    pub session: [u8; 16],
    pub sequence: u64,
    pub position: [f32; 2],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardMode {
    #[default]
    Off,
    Abstract,
    Precise,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharePolicy {
    pub keyboard: KeyboardMode,
    pub mouse: bool,
    pub text: bool,
    pub paused: bool,
}

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerState {
    pub share_revision: u64,
    pub abstract_keyboard: bool,
    pub share: SharePolicy,
    pub pressed: BTreeSet<u16>,
    pub mouse_buttons: BTreeSet<u8>,
    pub cursor: Option<[f32; 2]>,
    pub text: String,
    pub wheel: Option<(u64, i16)>,
}

// Wire contents deliberately have no Debug. Only stable NetworkError codes may
// reach logs. Sender identity is authenticated once by TLS, not on every key.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Envelope {
    pub version: u16,
    pub session: [u8; 16],
    pub sequence: u64,
    pub message: Message,
}

#[derive(Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum Message {
    Join {
        authorization: [u8; 32],
        nickname: String,
    },
    Accept {
        nickname: String,
    },
    Reject,
    Probe,
    PeerInfo(PeerInfo),
    Heartbeat,
    ShareStateChanged(SharePolicy, u64),
    TextUpdate(String),
    KeyboardDown(u16),
    KeyboardUp(u16),
    MouseMove([f32; 2]),
    MouseButtonDown(u8),
    MouseButtonUp(u8),
    MouseWheel(i16),
    AbstractKeyboard(bool),
    StateSnapshot(PeerState),
    StateReset,
    InputReset,
    Disconnect,
}

pub(crate) struct ReceiverState {
    session: [u8; 16],
    sequence: u64,
    pub peer: PeerState,
}

impl ReceiverState {
    pub fn new(session: [u8; 16]) -> Self {
        Self {
            session,
            sequence: 0,
            peer: PeerState::default(),
        }
    }

    pub fn apply(&mut self, envelope: &Envelope) -> Result<bool, NetworkError> {
        if envelope.version != VERSION || envelope.session != self.session {
            return Err(NetworkError::Protocol);
        }
        if envelope.sequence <= self.sequence {
            return Ok(false);
        }
        // Reliable streams cannot reorder. A gap means loss/replay/corruption;
        // force connection recovery instead of preserving an uncertain key set.
        if envelope.sequence != self.sequence + 1 {
            return Err(NetworkError::Protocol);
        }
        self.sequence = envelope.sequence;
        let active = !self.peer.share.paused;
        match &envelope.message {
            Message::ShareStateChanged(policy, revision) => {
                self.peer = PeerState {
                    share: *policy,
                    share_revision: *revision,
                    ..PeerState::default()
                };
            }
            Message::TextUpdate(text) if active && self.peer.share.text => {
                if text.chars().count() > MAX_TEXT_CHARS {
                    return Err(NetworkError::Protocol);
                }
                self.peer.text.clone_from(text);
            }
            Message::KeyboardDown(key) | Message::KeyboardUp(key)
                if active && self.peer.share.keyboard == KeyboardMode::Precise =>
            {
                if !valid_key(*key) {
                    return Err(NetworkError::Protocol);
                }
                if matches!(envelope.message, Message::KeyboardDown(_)) {
                    self.peer.pressed.insert(*key);
                } else {
                    self.peer.pressed.remove(key);
                }
            }
            Message::MouseMove(position) if active && self.peer.share.mouse => {
                if !position
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
                {
                    return Err(NetworkError::Protocol);
                }
                self.peer.cursor = Some(*position);
            }
            Message::MouseButtonDown(button) | Message::MouseButtonUp(button)
                if active && self.peer.share.mouse =>
            {
                if *button > 4 {
                    return Err(NetworkError::Protocol);
                }
                if matches!(envelope.message, Message::MouseButtonDown(_)) {
                    self.peer.mouse_buttons.insert(*button);
                } else {
                    self.peer.mouse_buttons.remove(button);
                }
            }
            Message::MouseWheel(delta)
                if active && self.peer.share.mouse && delta.unsigned_abs() <= 1200 =>
            {
                self.peer.wheel = Some((envelope.sequence, *delta));
            }
            Message::AbstractKeyboard(down)
                if active && self.peer.share.keyboard == KeyboardMode::Abstract =>
            {
                self.peer.abstract_keyboard = *down;
            }
            Message::InputReset => {
                self.peer.pressed.clear();
                self.peer.mouse_buttons.clear();
                self.peer.cursor = None;
                self.peer.abstract_keyboard = false;
                self.peer.wheel = None;
            }
            Message::StateReset => {
                self.peer = PeerState::default();
            }
            Message::StateSnapshot(snapshot) => {
                validate_snapshot(snapshot)?;
                self.peer = snapshot.clone();
            }
            Message::PeerInfo(info) => info.validate()?,
            Message::Probe | Message::Heartbeat | Message::Disconnect => {}
            _ => return Err(NetworkError::Protocol),
        }
        Ok(true)
    }
}

fn valid_key(key: u16) -> bool {
    (4..=0xa4).contains(&key) || (0xe0..=0xe7).contains(&key)
}

fn validate_snapshot(state: &PeerState) -> Result<(), NetworkError> {
    if state.pressed.len() > 64
        || state.mouse_buttons.len() > 5
        || state.pressed.iter().any(|key| !valid_key(*key))
        || state.mouse_buttons.iter().any(|button| *button > 4)
        || state.text.chars().count() > MAX_TEXT_CHARS
        || (!state.pressed.is_empty() && state.share.keyboard != KeyboardMode::Precise)
        || (state.abstract_keyboard
            && (state.share.keyboard != KeyboardMode::Abstract || state.share.paused))
        || ((!state.mouse_buttons.is_empty() || state.cursor.is_some()) && !state.share.mouse)
        || (!state.text.is_empty() && !state.share.text)
        || state.wheel.is_some_and(|(_, delta)| {
            !state.share.mouse || state.share.paused || delta.unsigned_abs() > 1200
        })
        || (state.share.paused
            && (!state.pressed.is_empty()
                || !state.mouse_buttons.is_empty()
                || !state.text.is_empty()
                || state.cursor.is_some()))
        || state
            .cursor
            .is_some_and(|p| p.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)))
    {
        Err(NetworkError::Protocol)
    } else {
        Ok(())
    }
}
