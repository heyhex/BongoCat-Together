//! Typed input observation and friend-runtime delivery. Neither side calls an
//! operating-system input-injection API.
use bongocat_input::{
    CursorPosition, CursorProducer, CursorSample, CursorViewport, InputControl, InputEdge,
    InputEvent, InputObserver, InputProducer, InputSource, MonotonicMillis, MouseButton,
    PhysicalKey,
};
use bongocat_network::{
    KeyboardMode, NetworkClient, PeerState, RemoteObserver, SharePolicy, SharedInput,
};
use std::{collections::BTreeSet, sync::Mutex, time::Instant};

pub struct TogetherInputObserver {
    client: NetworkClient,
    foreground: fn() -> Option<String>,
    pressed: Mutex<BTreeSet<u16>>,
    buttons: Mutex<BTreeSet<u8>>,
}

impl TogetherInputObserver {
    pub fn new(client: NetworkClient, foreground: fn() -> Option<String>) -> Self {
        Self {
            client,
            foreground,
            pressed: Mutex::new(BTreeSet::new()),
            buttons: Mutex::new(BTreeSet::new()),
        }
    }

    fn key(&self, usage: u16, down: bool, pressed: &mut BTreeSet<u16>) {
        if down {
            pressed.insert(usage);
        } else {
            pressed.remove(&usage);
        }
        let policy = self.client.sharing_policy();
        let event = match policy.keyboard {
            KeyboardMode::Off => return,
            KeyboardMode::Abstract => SharedInput::AbstractKeyboard(!pressed.is_empty()),
            KeyboardMode::Precise => {
                if self.client.scope_is_restricted()
                    && !self
                        .client
                        .application_allowed((self.foreground)().as_deref())
                {
                    let _ = self.client.publish_input(SharedInput::Reset);
                    return;
                }
                SharedInput::Key { usage, down }
            }
        };
        let _ = self.client.publish_input(event);
    }
}

impl InputObserver for TogetherInputObserver {
    fn on_input(&self, event: &InputEvent) {
        // The local runtime has already accepted this event. A contested
        // optional mirror fails closed and cannot block local animation.
        let Ok(mut pressed) = self.pressed.try_lock() else {
            self.on_overflow();
            return;
        };
        let Ok(mut buttons) = self.buttons.try_lock() else {
            self.on_overflow();
            return;
        };
        match event {
            InputEvent::Reset { .. } => {
                pressed.clear();
                buttons.clear();
                self.on_overflow();
            }
            InputEvent::Edge {
                control: InputControl::Key(key),
                edge,
                ..
            } => self.key(key.hid_usage(), *edge == InputEdge::Down, &mut pressed),
            InputEvent::Edge {
                control: InputControl::Mouse(button),
                edge,
                ..
            } => {
                if let Some(button) = mouse_index(*button) {
                    if *edge == InputEdge::Down {
                        buttons.insert(button);
                    } else {
                        buttons.remove(&button);
                    }
                    let _ = self.client.publish_input(SharedInput::MouseButton {
                        button,
                        down: *edge == InputEdge::Down,
                    });
                }
            }
            InputEvent::Reconcile {
                pressed: actual, ..
            } => {
                if self.client.sharing_policy().keyboard == KeyboardMode::Precise
                    && self.client.scope_is_restricted()
                    && !self
                        .client
                        .application_allowed((self.foreground)().as_deref())
                {
                    let _ = self.client.publish_input(SharedInput::Reset);
                }
                let actual_buttons: BTreeSet<_> = actual
                    .iter()
                    .filter_map(|control| match control {
                        InputControl::Mouse(button) => mouse_index(*button),
                        _ => None,
                    })
                    .collect();
                let released_buttons = buttons
                    .difference(&actual_buttons)
                    .copied()
                    .collect::<Vec<_>>();
                for button in released_buttons {
                    buttons.remove(&button);
                    let _ = self.client.publish_input(SharedInput::MouseButton {
                        button,
                        down: false,
                    });
                }
                let actual: BTreeSet<_> = actual
                    .iter()
                    .filter_map(|control| match control {
                        InputControl::Key(key) => Some(key.hid_usage()),
                        _ => None,
                    })
                    .collect();
                let released = pressed.difference(&actual).copied().collect::<Vec<_>>();
                for usage in released {
                    self.key(usage, false, &mut pressed);
                }
                // A press captured before sharing was enabled is not promoted
                // into a new disclosure by reconciliation.
            }
            _ => {}
        }
        self.client.observe_local_keys(&pressed);
    }

    fn on_overflow(&self) {
        let _ = self.client.set_share(SharePolicy::default());
    }
    fn on_wheel(&self, delta: i16) {
        let _ = self.client.publish_input(SharedInput::Wheel(delta));
    }
}

fn mouse_index(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Right => Some(1),
        MouseButton::Middle => Some(2),
        MouseButton::Back => Some(3),
        MouseButton::Forward => Some(4),
        MouseButton::Other(_) => None,
    }
}

fn mouse_button(index: u8) -> MouseButton {
    [
        MouseButton::Left,
        MouseButton::Right,
        MouseButton::Middle,
        MouseButton::Back,
        MouseButton::Forward,
    ][usize::from(index.min(4))]
}

pub struct TogetherRemoteInput {
    input: InputProducer,
    cursor: CursorProducer,
    previous: Mutex<(u64, PeerState)>,
    started: Instant,
}

impl TogetherRemoteInput {
    pub fn new(input: InputProducer, cursor: CursorProducer) -> Self {
        Self {
            input,
            cursor,
            previous: Mutex::new((0, PeerState::default())),
            started: Instant::now(),
        }
    }
}

impl RemoteObserver for TogetherRemoteInput {
    fn state_changed(&self, revision: u64, state: &PeerState) {
        let mut previous = self.previous.lock().unwrap_or_else(|p| p.into_inner());
        if revision <= previous.0 {
            return;
        }
        let at = MonotonicMillis::new(self.started.elapsed().as_millis() as u64);
        let keys = |state: &PeerState| {
            let mut keys = state.pressed.clone();
            if state.abstract_keyboard {
                keys.insert(PhysicalKey::KEY_A.hid_usage());
            }
            keys
        };
        let before = keys(&previous.1);
        let after = keys(state);
        let mut events = Vec::new();
        for usage in before.difference(&after) {
            events.push((
                InputControl::Key(PhysicalKey::from_hid_usage(*usage)),
                InputEdge::Up,
            ));
        }
        for usage in after.difference(&before) {
            events.push((
                InputControl::Key(PhysicalKey::from_hid_usage(*usage)),
                InputEdge::Down,
            ));
        }
        for button in previous.1.mouse_buttons.difference(&state.mouse_buttons) {
            events.push((InputControl::Mouse(mouse_button(*button)), InputEdge::Up));
        }
        for button in state.mouse_buttons.difference(&previous.1.mouse_buttons) {
            events.push((InputControl::Mouse(mouse_button(*button)), InputEdge::Down));
        }
        for (control, edge) in events {
            if self
                .input
                .publish(InputEvent::Edge {
                    control,
                    edge,
                    source: InputSource::Capture,
                    at,
                })
                .is_err()
            {
                let _ = self
                    .input
                    .recover(bongocat_input::InputResetReason::QueueOverflow, at);
                break;
            }
        }
        if state.cursor != previous.1.cursor
            && let Some([x, y]) = state.cursor
        {
            let position = CursorPosition {
                x: f64::from(x) * 1000.0,
                y: f64::from(y) * 1000.0,
            };
            let _ = self.cursor.publish(CursorSample {
                pointer: position,
                position,
                viewport: CursorViewport {
                    origin: CursorPosition { x: 0.0, y: 0.0 },
                    width: 1000.0,
                    height: 1000.0,
                },
                at,
            });
        }
        *previous = (revision, state.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bongocat_network::{NetworkService, Phase, RoomOptions};
    use bongocat_runtime::RuntimeOwner;
    use std::{sync::Arc, time::Duration};

    fn wait(mut predicate: impl FnMut() -> bool) {
        let start = Instant::now();
        while !predicate() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "input integration deadline"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn foreground_scope_lock_and_remote_runtime_release() {
        let host = NetworkService::start().unwrap();
        let guest = NetworkService::start().unwrap();
        let a = host.client();
        let b = guest.client();
        a.create(RoomOptions {
            nickname: "host".into(),
            direct_only: true,
        })
        .unwrap();
        wait(|| a.snapshot().invitation.is_some());
        b.join(
            a.snapshot().invitation.unwrap(),
            RoomOptions {
                nickname: "friend".into(),
                direct_only: true,
            },
        )
        .unwrap();
        wait(|| a.snapshot().phase == Phase::Connected && b.snapshot().phase == Phase::Connected);
        let runtime = RuntimeOwner::start(true, 128);
        b.set_remote_observer(Some(Arc::new(TogetherRemoteInput::new(
            runtime.input_producer(),
            runtime.cursor_producer(),
        ))));
        let observer = TogetherInputObserver::new(a.clone(), || Some("notepad.exe".into()));
        a.set_application_scope(vec!["code.exe".into()]).unwrap();
        a.set_share(SharePolicy {
            keyboard: KeyboardMode::Precise,
            ..Default::default()
        })
        .unwrap();
        let edge = |key, down| InputEvent::Edge {
            control: InputControl::Key(PhysicalKey::from_hid_usage(key)),
            edge: if down { InputEdge::Down } else { InputEdge::Up },
            source: InputSource::Capture,
            at: MonotonicMillis::new(1),
        };
        observer.on_input(&edge(4, true));
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(runtime.client().snapshot().input.pressed_key_count, 0);
        a.set_application_scope(vec!["notepad.exe".into()]).unwrap();
        observer.on_input(&edge(6, true));
        observer.on_input(&edge(0xe0, true));
        wait(|| runtime.client().snapshot().input.pressed_key_count == 2);
        observer.on_input(&InputEvent::Reset {
            reason: bongocat_input::InputResetReason::SessionLock,
            at: MonotonicMillis::new(2),
        });
        wait(|| runtime.client().snapshot().input.pressed_key_count == 0);
        assert_eq!(a.sharing_policy(), SharePolicy::default());
        b.set_remote_observer(None);
        runtime.shutdown(Duration::from_secs(2)).unwrap();
    }
}
