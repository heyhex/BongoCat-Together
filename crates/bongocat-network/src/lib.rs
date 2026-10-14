#![forbid(unsafe_code)]

//! Optional Together service. No endpoint or network thread exists until the
//! application explicitly starts the service; endpoints only bind on create/join.
//! Secrets and shared input/text intentionally implement no Debug or logging.

mod protocol;
mod transport;
mod worker;

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub use protocol::{KeyboardMode, PeerInfo, PeerState, SharePolicy};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Offline,
    Starting,
    Waiting,
    Connecting,
    Connected,
    Reconnecting,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConnectionPath {
    #[default]
    Unknown,
    Direct,
    Relay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NetworkError {
    #[error("sharing indicator unavailable; sharing stopped")]
    IndicatorUnavailable,
    #[error("network service is busy")]
    Busy,
    #[error("network service has stopped")]
    Stopped,
    #[error("invalid or expired invitation")]
    InvalidInvite,
    #[error("peer authorization rejected")]
    Unauthorized,
    #[error("network connection timed out; direct-only mode requires a reachable UDP path")]
    Timeout,
    #[error("network endpoint unavailable")]
    Endpoint,
    #[error("connection lost")]
    Connection,
    #[error("invalid or excessive network message")]
    Protocol,
    #[error("nickname must contain 1 to 32 characters")]
    Nickname,
    #[error("text sharing is disabled or text exceeds 512 characters")]
    Text,
}

/// An in-memory UI projection, never persisted or written to logs.
#[derive(Clone, Default)]
pub struct NetworkSnapshot {
    pub local_keys: std::collections::BTreeSet<u16>,
    pub revision: u64,
    pub phase: Phase,
    pub path: ConnectionPath,
    pub rtt_ms: Option<u64>,
    pub invitation: Option<String>,
    pub peer_nickname: Option<String>,
    pub peer_info: Option<PeerInfo>,
    pub probes_received: u64,
    pub reconnect_attempt: u32,
    pub error: Option<NetworkError>,
    pub local_share: SharePolicy,
    pub remote: PeerState,
}

pub struct RoomOptions {
    pub nickname: String,
    pub direct_only: bool,
}

impl Default for RoomOptions {
    fn default() -> Self {
        Self {
            nickname: "BongoCat".into(),
            direct_only: false,
        }
    }
}

enum Command {
    Input {
        event: SharedInput,
        privacy_revision: u64,
    },
    Create(RoomOptions),
    Join {
        invitation: String,
        options: RoomOptions,
    },
    Probe,
    #[cfg(test)]
    TransientDisconnect,
    Text {
        value: String,
        privacy_revision: u64,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Control {
    generation: u64,
    shutdown: bool,
}

#[derive(Clone, Copy, Default)]
struct Privacy {
    revision: u64,
    policy: SharePolicy,
}

#[derive(Clone)]
pub struct NetworkClient {
    commands: async_channel::Sender<Command>,
    control: tokio::sync::watch::Sender<Control>,
    state: State,
    privacy: tokio::sync::watch::Sender<Privacy>,
}

impl NetworkClient {
    pub fn set_peer_info(&self, info: PeerInfo) -> Result<(), NetworkError> {
        info.validate()?;
        let mut current = self.state.info.lock().unwrap_or_else(|p| p.into_inner());
        if *current != info {
            *current = info;
        }
        Ok(())
    }
    pub fn indicator_unavailable(&self) {
        let _ = self.set_share(SharePolicy::default());
        update(&self.state, |s| {
            s.error = Some(NetworkError::IndicatorUnavailable)
        });
    }
    pub fn observe_local_keys(&self, keys: &std::collections::BTreeSet<u16>) {
        if let Ok(mut snapshot) = self.state.snapshot.try_lock() {
            snapshot.local_keys.clone_from(keys);
            snapshot.revision = snapshot.revision.saturating_add(1);
        }
    }
    pub fn snapshot(&self) -> NetworkSnapshot {
        self.state
            .snapshot
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    fn submit(&self, command: Command) -> Result<(), NetworkError> {
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                async_channel::TrySendError::Full(_) => NetworkError::Busy,
                async_channel::TrySendError::Closed(_) => NetworkError::Stopped,
            })
    }

    pub fn create(&self, options: RoomOptions) -> Result<(), NetworkError> {
        validate_nickname(&options.nickname)?;
        self.submit(Command::Create(options))
    }

    pub fn join(&self, invitation: String, options: RoomOptions) -> Result<(), NetworkError> {
        validate_nickname(&options.nickname)?;
        transport::Invite::decode(&invitation)?;
        self.submit(Command::Join {
            invitation,
            options,
        })
    }

    pub fn probe(&self) -> Result<(), NetworkError> {
        self.submit(Command::Probe)
    }

    pub fn set_share(&self, policy: SharePolicy) -> Result<(), NetworkError> {
        self.privacy.send_modify(|p| {
            p.revision += 1;
            p.policy = policy;
        });
        Ok(())
    }

    pub fn sharing_policy(&self) -> SharePolicy {
        self.privacy.borrow().policy
    }

    pub fn set_application_scope(&self, executables: Vec<String>) -> Result<(), NetworkError> {
        if executables.len() > 16
            || executables.iter().any(|name| {
                name.len() > 260
                    || name.contains(['/', '\\'])
                    || !name.to_ascii_lowercase().ends_with(".exe")
            })
        {
            return Err(NetworkError::Protocol);
        }
        *self.state.scope.lock().unwrap_or_else(|p| p.into_inner()) = executables
            .into_iter()
            .map(|s| s.to_ascii_lowercase())
            .collect();
        // Changing scope invalidates all queued precise edges and remote holds.
        let policy = self.sharing_policy();
        self.set_share(policy)
    }

    pub fn application_allowed(&self, foreground: Option<&str>) -> bool {
        let Ok(scope) = self.state.scope.try_lock() else {
            return false;
        };
        scope.is_empty()
            || foreground.is_some_and(|name| {
                scope
                    .iter()
                    .any(|allowed| allowed.eq_ignore_ascii_case(name))
            })
    }

    pub fn scope_is_restricted(&self) -> bool {
        self.state
            .scope
            .try_lock()
            .map_or(true, |scope| !scope.is_empty())
    }

    pub fn publish_input(&self, event: SharedInput) -> Result<(), NetworkError> {
        let privacy = *self.privacy.borrow();
        if !event.allowed(privacy.policy) {
            return Ok(());
        }
        let result = self.submit(Command::Input {
            event,
            privacy_revision: privacy.revision,
        });
        if result.is_err() {
            let _ = self.set_share(SharePolicy::default());
        }
        result
    }

    /// Latest-only datagram lane, isolated from reliable key/button releases.
    pub fn publish_cursor(&self, position: [f32; 2]) {
        let privacy = *self.privacy.borrow();
        if !privacy.policy.mouse
            || privacy.policy.paused
            || position
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return;
        }
        if let Ok(mut motion) = self.state.motion.try_lock() {
            *motion = Some((privacy.revision, position));
        }
    }

    pub fn set_remote_observer(&self, observer: Option<Arc<dyn RemoteObserver>>) {
        *self
            .state
            .observer
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = observer.clone();
        if let Some(observer) = observer {
            let snapshot = self.snapshot();
            observer.state_changed(snapshot.revision, &snapshot.remote);
        }
    }

    pub fn set_text(&self, text: String) -> Result<(), NetworkError> {
        let privacy = *self.privacy.borrow();
        if !privacy.policy.text
            || privacy.policy.paused
            || text.chars().count() > protocol::MAX_TEXT_CHARS
        {
            return Err(NetworkError::Text);
        }
        self.submit(Command::Text {
            value: text,
            privacy_revision: privacy.revision,
        })
    }

    /// Separate priority lane: shutdown/revoke never queues behind user data.
    pub fn disconnect(&self) {
        let _ = self.set_share(SharePolicy::default());
        self.control.send_modify(|control| control.generation += 1);
    }
}

pub struct NetworkService {
    client: NetworkClient,
    worker: Option<JoinHandle<()>>,
}

impl NetworkService {
    pub fn start() -> Result<Self, NetworkError> {
        let (commands, receiver) = async_channel::bounded(64);
        let (control, control_rx) = tokio::sync::watch::channel(Control {
            generation: 0,
            shutdown: false,
        });
        let state = Arc::new(SharedState::default());
        let (privacy, _) = tokio::sync::watch::channel(Privacy::default());
        let client = NetworkClient {
            commands,
            control,
            state: state.clone(),
            privacy: privacy.clone(),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| NetworkError::Endpoint)?;
        let worker = std::thread::Builder::new()
            .name("bongocat-together".into())
            .spawn(move || runtime.block_on(worker::run(receiver, control_rx, privacy, state)))
            .map_err(|_| NetworkError::Endpoint)?;
        Ok(Self {
            client,
            worker: Some(worker),
        })
    }

    pub fn client(&self) -> NetworkClient {
        self.client.clone()
    }
}

impl Drop for NetworkService {
    fn drop(&mut self) {
        self.client.control.send_modify(|c| {
            c.shutdown = true;
            c.generation += 1;
        });
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn validate_nickname(value: &str) -> Result<(), NetworkError> {
    if value.trim().is_empty() || value.chars().count() > 32 || value.chars().any(char::is_control)
    {
        Err(NetworkError::Nickname)
    } else {
        Ok(())
    }
}

pub trait RemoteObserver: Send + Sync {
    /// A revision orders callbacks racing attachment/teardown. Never inject OS
    /// input; the application adapter feeds only the independent cat runtime.
    fn state_changed(&self, revision: u64, state: &PeerState);
}

pub enum SharedInput {
    Key { usage: u16, down: bool },
    MouseButton { button: u8, down: bool },
    Wheel(i16),
    AbstractKeyboard(bool),
    Reset,
}

impl SharedInput {
    fn allowed(&self, policy: SharePolicy) -> bool {
        if policy.paused {
            return false;
        }
        match self {
            Self::Key { usage, .. } => {
                policy.keyboard == KeyboardMode::Precise
                    && ((4..=0xa4).contains(usage) || (0xe0..=0xe7).contains(usage))
            }
            Self::MouseButton { button, .. } => policy.mouse && *button <= 4,
            Self::Wheel(delta) => policy.mouse && delta.unsigned_abs() <= 1200,
            Self::AbstractKeyboard(_) => policy.keyboard == KeyboardMode::Abstract,
            Self::Reset => true,
        }
    }
}

#[derive(Default)]
struct SharedState {
    info: Mutex<PeerInfo>,
    snapshot: Mutex<NetworkSnapshot>,
    observer: Mutex<Option<Arc<dyn RemoteObserver>>>,
    motion: Mutex<Option<(u64, [f32; 2])>>,
    scope: Mutex<Vec<String>>,
}

type State = Arc<SharedState>;

fn update(state: &State, change: impl FnOnce(&mut NetworkSnapshot)) {
    let mut snapshot = state.snapshot.lock().unwrap_or_else(|p| p.into_inner());
    let previous = snapshot.remote.clone();
    change(&mut snapshot);
    snapshot.revision = snapshot.revision.saturating_add(1);
    let changed =
        (previous != snapshot.remote).then(|| (snapshot.revision, snapshot.remote.clone()));
    drop(snapshot);
    if let Some((revision, peer)) = changed {
        let observer = state
            .observer
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        if let Some(observer) = observer {
            observer.state_changed(revision, &peer);
        }
    }
}

#[cfg(test)]
mod tests;
