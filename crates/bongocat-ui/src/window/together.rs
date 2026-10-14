use super::*;
use bongocat_network::{
    ConnectionPath, KeyboardMode, NetworkClient, NetworkError, NetworkSnapshot, Phase, RoomOptions,
    SharePolicy,
};
use bongocat_ui_protocol::{TogetherLayoutClient, TogetherLayoutCommand, TogetherModelStatus};
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::{
    input::{Textarea, TextareaState},
    switch::Switch,
};
use gpui_kit::{AnyElement, Axis, ClipboardItem, Subscription};

#[derive(Clone, Debug, PartialEq)]
struct FriendModelChoice {
    key: SettingsModelKey,
    title: SharedString,
}
impl SearchableListItem for FriendModelChoice {
    type Value = Self;
    fn title(&self) -> SharedString {
        self.title.clone()
    }
    fn value(&self) -> &Self {
        self
    }
}

pub(crate) struct TogetherView {
    layout: TogetherLayoutClient,
    models: Vec<FriendModelChoice>,
    selected_model: Option<SettingsModelKey>,
    last_model_status: TogetherModelStatus,
    model_select: Entity<SelectState<SearchableVec<FriendModelChoice>>>,
    _model_subscription: Subscription,
    client: NetworkClient,
    snapshot: NetworkSnapshot,
    pub(super) language: SettingsLanguage,
    nickname: Entity<InputState>,
    invitation: Entity<InputState>,
    text: Entity<TextareaState>,
    direct_only: bool,
    text_generation: u64,
    _text_subscription: Subscription,
    pending_error: Option<NetworkError>,
    scope: Entity<InputState>,
    _scope_subscription: Subscription,
}

impl SettingsView {
    pub fn attach_together(
        &mut self,
        client: NetworkClient,
        layout: TogetherLayoutClient,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.together.is_none() {
            let language = self.display_language();
            let together = cx.new(|cx| TogetherView::new(client, layout, language, window, cx));
            self.together_subscription = Some(cx.observe(&together, |_, _, cx| cx.notify()));
            self.together = Some(together);
            cx.notify();
        }
    }
}

impl TogetherView {
    fn new(
        client: NetworkClient,
        layout: TogetherLayoutClient,
        language: SettingsLanguage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let nickname = cx.new(|cx| InputState::new(window, cx).default_value("BongoCat"));
        let invitation = cx.new(|cx| InputState::new(window, cx));
        let text = cx.new(|cx| TextareaState::new(window, cx).rows(3));
        let scope = cx.new(|cx| InputState::new(window, cx).placeholder("code.exe; notepad.exe"));
        let model_select = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(Vec::<FriendModelChoice>::new()),
                None,
                window,
                cx,
            )
        });
        let model_subscription = cx.subscribe(
            &model_select,
            |view, _, event: &SelectEvent<SearchableVec<FriendModelChoice>>, cx| {
                if let SelectEvent::Confirm(Some(choice)) = event {
                    view.layout
                        .command(TogetherLayoutCommand::SelectModel(choice.key.clone()));
                    cx.notify();
                }
            },
        );
        let scope_subscription = cx.subscribe(&scope, |view, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                let value = input.read(cx).value().to_string();
                let executables = value
                    .split(';')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect();
                let result = view.client.set_application_scope(executables);
                view.report(result, cx);
            }
        });
        let subscription = cx.subscribe(&text, |view, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                view.text_generation += 1;
                let generation = view.text_generation;
                cx.spawn(async move |view, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(120))
                        .await;
                    let _ = view.update(cx, |view, cx| {
                        let policy = view.client.sharing_policy();
                        if view.text_generation != generation || !policy.text || policy.paused {
                            return;
                        }
                        let value = view.text.read(cx).value().to_string();
                        if let Err(error) = view.client.set_text(value) {
                            view.pending_error = Some(error);
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
        });
        cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if view
                    .update(cx, |view, cx| {
                        let next = view.client.snapshot();
                        if next.revision != view.snapshot.revision {
                            if next.error != view.snapshot.error {
                                view.pending_error = next.error;
                            }
                            view.snapshot = next;
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            layout,
            models: Vec::new(),
            selected_model: None,
            last_model_status: TogetherModelStatus::Idle,
            model_select,
            _model_subscription: model_subscription,
            snapshot: client.snapshot(),
            client,
            language,
            nickname,
            invitation,
            text,
            direct_only: false,
            text_generation: 0,
            _text_subscription: subscription,
            pending_error: None,
            scope,
            _scope_subscription: scope_subscription,
        }
    }

    pub(super) fn sync_models(
        &mut self,
        entries: &[SettingsModelEntry],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let choices: Vec<_> = entries
            .iter()
            .filter(|entry| matches!(entry.availability, SettingsModelAvailability::Ready { .. }))
            .map(|entry| FriendModelChoice {
                key: SettingsModelKey {
                    id: entry.id.clone(),
                    origin: entry.origin,
                },
                title: entry.title.clone().into(),
            })
            .collect();
        let selected = self.layout.snapshot().model;
        if self.models != choices || self.selected_model.as_ref() != Some(&selected) {
            self.models = choices.clone();
            self.selected_model = Some(selected.clone());
            self.model_select.update(cx, |state, cx| {
                state.set_items(SearchableVec::new(choices.clone()), window, cx);
                if let Some(choice) = choices
                    .iter()
                    .find(|choice| choice.key == selected)
                    .or_else(|| {
                        choices.iter().find(|choice| {
                            choice.key.id == "standard"
                                && choice.key.origin == SettingsModelOrigin::BuiltIn
                        })
                    })
                {
                    state.set_selected_value(choice, window, cx);
                }
            });
        }
    }

    fn label(&self, key: &str) -> SharedString {
        bongocat_i18n::text(self.language.catalog_locale(), key)
            .to_owned()
            .into()
    }

    fn options(&self, cx: &App) -> RoomOptions {
        RoomOptions {
            nickname: self.nickname.read(cx).value().to_string(),
            direct_only: self.direct_only,
        }
    }

    fn report(&mut self, result: Result<(), NetworkError>, cx: &mut Context<Self>) {
        if let Err(error) = result {
            self.pending_error = Some(error);
        }
        cx.notify();
    }
}

fn error_key(error: NetworkError) -> &'static str {
    match error {
        NetworkError::IndicatorUnavailable => "together.error_indicator",
        NetworkError::Busy => "together.error_busy",
        NetworkError::InvalidInvite => "together.error_invite",
        NetworkError::Unauthorized => "together.error_auth",
        NetworkError::Timeout => "together.error_timeout",
        NetworkError::Nickname => "together.error_nickname",
        NetworkError::Text => "together.error_text",
        NetworkError::Protocol => "together.error_protocol",
        NetworkError::Endpoint | NetworkError::Stopped | NetworkError::Connection => {
            "together.error_connection"
        }
    }
}

pub(crate) fn key_chord(keys: &BTreeSet<u16>) -> String {
    keys.iter()
        .filter(|key| (0xe0..=0xe7).contains(*key))
        .chain(keys.iter().filter(|key| !(0xe0..=0xe7).contains(*key)))
        .map(|key| key_label(*key))
        .collect::<Vec<_>>()
        .join(" + ")
}

fn key_label(key: u16) -> String {
    match key {
        4..=29 => char::from(b'A' + (key - 4) as u8).to_string(),
        30..=38 => (key - 29).to_string(),
        39 => "0".into(),
        58..=69 => format!("F{}", key - 57),
        104..=115 => format!("F{}", key - 91),
        _ => match key {
            40 => "Enter",
            41 => "Esc",
            42 => "Backspace",
            43 => "Tab",
            44 => "Space",
            45 => "-",
            46 => "=",
            47 => "[",
            48 => "]",
            49 => "\\",
            51 => ";",
            52 => "'",
            53 => "`",
            54 => ",",
            55 => ".",
            56 => "/",
            57 => "CapsLock",
            70 => "PrintScreen",
            71 => "ScrollLock",
            72 => "Pause",
            73 => "Insert",
            74 => "Home",
            75 => "PageUp",
            76 => "Delete",
            77 => "End",
            78 => "PageDown",
            79 => "→",
            80 => "←",
            81 => "↓",
            82 => "↑",
            0xe0 => "Ctrl",
            0xe1 => "Shift",
            0xe2 => "Alt",
            0xe3 => "Win",
            0xe4 => "Right Ctrl",
            0xe5 => "Right Shift",
            0xe6 => "Right Alt",
            0xe7 => "Right Win",
            _ => return format!("HID {key:02X}"),
        }
        .into(),
    }
}

impl TogetherView {
    /// Use the same SettingGroup/SettingItem layout and responsive stacking as
    /// other settings pages. A whole page must never be a horizontal row's field.
    pub(super) fn groups(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<SettingGroup> {
        if let Some(error) = self.pending_error.take() {
            window.push_notification(
                Notification::new()
                    .message(self.label(error_key(error)))
                    .with_type(NotificationType::Error),
                cx,
            );
        }
        let layout = self.layout.snapshot();
        if layout.model_status == TogetherModelStatus::Failed
            && self.last_model_status != TogetherModelStatus::Failed
        {
            window.push_notification(
                Notification::new()
                    .message(self.label("together.error_model"))
                    .with_type(NotificationType::Error),
                cx,
            );
        }
        self.last_model_status = layout.model_status;
        let policy = self.client.sharing_policy();
        if !policy.text && !self.text.read(cx).value().is_empty() {
            self.text
                .update(cx, |text, cx| text.set_value("", window, cx));
        }
        let connected = self.snapshot.phase == Phase::Connected;
        let idle = matches!(self.snapshot.phase, Phase::Offline | Phase::Failed);
        let connection = SettingGroup::new()
            .title(self.label("together.connection_group"))
            .description(self.label("together.description"))
            .item(self.item(
                "together.connection_group",
                "together.status",
                Axis::Horizontal,
                false,
                cx,
                |view, _, _, _| {
                    let phase = match view.snapshot.phase {
                        Phase::Offline => "together.offline",
                        Phase::Starting => "together.starting",
                        Phase::Waiting => "together.waiting",
                        Phase::Connecting => "together.connecting",
                        Phase::Connected => "together.connected",
                        Phase::Reconnecting => "together.reconnecting",
                        Phase::Failed => "together.failed",
                    };
                    let path = match view.snapshot.path {
                        ConnectionPath::Unknown => "together.unknown",
                        ConnectionPath::Direct => "together.direct",
                        ConnectionPath::Relay => "together.relay",
                    };
                    let latency = view.snapshot.rtt_ms.map_or_else(
                        || view.label("together.unknown").to_string(),
                        |ms| format!("{ms} ms"),
                    );
                    div()
                        .id("together-status")
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(view.label(phase))
                        .child(view.label(path))
                        .child(latency)
                        .into_any_element()
                },
            ))
            .item(self.item(
                "together.connection_group",
                "together.nickname",
                Axis::Horizontal,
                !idle,
                cx,
                |view, options, _, _| {
                    Input::new(&view.nickname)
                        .disabled(options.is_disabled())
                        .into_any_element()
                },
            ))
            .item(self.item(
                "together.connection_group",
                "together.remote_nickname",
                Axis::Horizontal,
                false,
                cx,
                |view, _, _, _| {
                    div()
                        .max_w_full()
                        .child(
                            view.snapshot
                                .peer_nickname
                                .clone()
                                .unwrap_or_else(|| "—".into()),
                        )
                        .into_any_element()
                },
            ))
            .item(self.item(
                "together.connection_group",
                "together.direct_only",
                Axis::Horizontal,
                !idle,
                cx,
                |view, options, _, cx| {
                    Switch::new("together-direct")
                        .checked(view.direct_only)
                        .disabled(options.is_disabled())
                        .on_click(cx.listener(|view, checked, _, cx| {
                            view.direct_only = *checked;
                            cx.notify();
                        }))
                        .into_any_element()
                },
            ))
            .item(self.custom("together.connection_group", cx, |view, _, cx| {
                div()
                    .id("together-room-actions")
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("together-create")
                            .label(view.label("together.create"))
                            .disabled(!matches!(
                                view.snapshot.phase,
                                Phase::Offline | Phase::Failed
                            ))
                            .on_click(cx.listener(|view, _, _, cx| {
                                let result = view.client.create(view.options(cx));
                                view.report(result, cx);
                            })),
                    )
                    .child(
                        Button::new("together-copy")
                            .label(view.label("together.copy"))
                            .disabled(view.snapshot.invitation.is_none())
                            .on_click(cx.listener(|view, _, _, cx| {
                                if let Some(code) = &view.snapshot.invitation {
                                    cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
                                }
                            })),
                    )
                    .child(
                        Button::new("together-disconnect")
                            .label(view.label("together.disconnect"))
                            .disabled(matches!(
                                view.snapshot.phase,
                                Phase::Offline | Phase::Failed
                            ))
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.client.disconnect();
                                view.clear_text(window, cx);
                                cx.notify();
                            })),
                    )
                    .into_any_element()
            }))
            .item(self.item(
                "together.connection_group",
                "together.invitation",
                Axis::Vertical,
                !idle,
                cx,
                |view, options, _, _| {
                    div()
                        .id("together-invitation-field")
                        .test_support()
                        .w_full()
                        .min_w_0()
                        .child(Input::new(&view.invitation).disabled(options.is_disabled()))
                        .into_any_element()
                },
            ))
            .item(self.custom("together.connection_group", cx, |view, _, cx| {
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("together-join")
                            .label(view.label("together.join"))
                            .disabled(!matches!(
                                view.snapshot.phase,
                                Phase::Offline | Phase::Failed
                            ))
                            .on_click(cx.listener(|view, _, _, cx| {
                                let code = view.invitation.read(cx).value().to_string();
                                let result = view.client.join(code, view.options(cx));
                                view.report(result, cx);
                            })),
                    )
                    .child(
                        Button::new("together-probe")
                            .label(view.label("together.probe"))
                            .disabled(view.snapshot.phase != Phase::Connected)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.report(view.client.probe(), cx);
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{}: {}",
                                view.label("together.received"),
                                view.snapshot.probes_received
                            )),
                    )
                    .into_any_element()
            }));
        let mut input = SettingGroup::new()
            .title(self.label("together.input_group"))
            .description(self.label("together.privacy"))
            .item(self.item(
                "together.input_group",
                "together.local_keys",
                Axis::Vertical,
                false,
                cx,
                |view, _, _, _| Self::keys("together-local-keys", &view.snapshot.local_keys),
            ))
            .item(self.item(
                "together.input_group",
                "together.remote_keys",
                Axis::Vertical,
                false,
                cx,
                |view, _, _, _| Self::keys("together-remote-keys", &view.snapshot.remote.pressed),
            ))
            .item(self.item(
                "together.input_group",
                "together.abstract_share",
                Axis::Horizontal,
                !connected,
                cx,
                |view, options, _, cx| {
                    Switch::new("together-abstract")
                        .checked(view.client.sharing_policy().keyboard == KeyboardMode::Abstract)
                        .disabled(options.is_disabled())
                        .on_click(cx.listener(|view, checked, _, cx| {
                            let mut policy = view.client.sharing_policy();
                            policy.keyboard = if *checked {
                                KeyboardMode::Abstract
                            } else {
                                KeyboardMode::Off
                            };
                            view.report(view.client.set_share(policy), cx);
                        }))
                        .into_any_element()
                },
            ))
            .item(self.item(
                "together.input_group",
                "together.precise_share",
                Axis::Horizontal,
                !connected || !cfg!(target_os = "windows"),
                cx,
                |view, options, _, cx| {
                    Switch::new("together-precise")
                        .checked(view.client.sharing_policy().keyboard == KeyboardMode::Precise)
                        .disabled(options.is_disabled())
                        .on_click(cx.listener(|view, checked, _, cx| {
                            let mut policy = view.client.sharing_policy();
                            policy.keyboard = if *checked {
                                KeyboardMode::Precise
                            } else {
                                KeyboardMode::Off
                            };
                            view.report(view.client.set_share(policy), cx);
                        }))
                        .into_any_element()
                },
            ))
            .item(self.item(
                "together.input_group",
                "together.scope",
                Axis::Vertical,
                false,
                cx,
                |view, options, _, _| {
                    div()
                        .id("together-scope-field")
                        .w_full()
                        .min_w_0()
                        .child(Input::new(&view.scope).disabled(options.is_disabled()))
                        .into_any_element()
                },
            ))
            .item(self.item(
                "together.input_group",
                "together.mouse_share",
                Axis::Horizontal,
                !connected,
                cx,
                |view, options, _, cx| {
                    Switch::new("together-mouse")
                        .checked(view.client.sharing_policy().mouse)
                        .disabled(options.is_disabled())
                        .on_click(cx.listener(|view, checked, _, cx| {
                            let mut policy = view.client.sharing_policy();
                            policy.mouse = *checked;
                            view.report(view.client.set_share(policy), cx);
                        }))
                        .into_any_element()
                },
            ));
        if policy.keyboard == KeyboardMode::Precise && !policy.paused {
            input = input.item(self.custom("together.input_group", cx, |view, _, cx| {
                div()
                    .w_full()
                    .min_w_0()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(view.label("together.precise_active"))
                    .into_any_element()
            }));
        }
        input = input.item(self.custom("together.input_group", cx, |view, _, cx| {
            let paused = view.client.sharing_policy().paused;
            div()
                .id("together-sharing-actions")
                .w_full()
                .min_w_0()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("together-pause")
                        .label(view.label(if paused {
                            "together.resume"
                        } else {
                            "together.pause"
                        }))
                        .disabled(view.snapshot.phase != Phase::Connected)
                        .on_click(cx.listener(|view, _, _, cx| {
                            let mut policy = view.client.sharing_policy();
                            policy.paused = !policy.paused;
                            view.report(view.client.set_share(policy), cx);
                        })),
                )
                .child(
                    Button::new("together-stop")
                        .label(view.label("together.stop_all"))
                        .disabled(view.snapshot.phase != Phase::Connected)
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.report(view.client.set_share(SharePolicy::default()), cx);
                            view.clear_text(window, cx);
                        })),
                )
                .into_any_element()
        }));
        let mut text = SettingGroup::new()
            .title(self.label("together.text_group"))
            .item(self.item(
                "together.text_group",
                "together.text_share",
                Axis::Horizontal,
                !connected,
                cx,
                |view, options, _, cx| {
                    Switch::new("together-text")
                        .checked(view.client.sharing_policy().text)
                        .disabled(options.is_disabled())
                        .on_click(cx.listener(|view, checked, window, cx| {
                            let result = view.client.set_share(SharePolicy {
                                text: *checked,
                                ..view.client.sharing_policy()
                            });
                            view.clear_text(window, cx);
                            view.report(result, cx);
                        }))
                        .into_any_element()
                },
            ));
        if policy.text {
            text = text
                .item(self.item(
                    "together.text_group",
                    "together.sharing",
                    Axis::Vertical,
                    !connected,
                    cx,
                    |view, options, _, cx| {
                        div()
                            .id("together-text-editor")
                            .test_support()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(Textarea::new(&view.text).disabled(options.is_disabled()))
                            .child(div().text_sm().child(format!(
                                "{}/512",
                                view.text.read(cx).value().chars().count()
                            )))
                            .into_any_element()
                    },
                ))
                .item(self.custom("together.text_group", cx, |view, _, cx| {
                    Button::new("together-stop-text")
                        .label(view.label("together.stop_text"))
                        .on_click(cx.listener(|view, _, window, cx| {
                            let result = view.client.set_share(SharePolicy {
                                text: false,
                                ..view.client.sharing_policy()
                            });
                            view.clear_text(window, cx);
                            view.report(result, cx);
                        }))
                        .into_any_element()
                }));
        }
        text = text.item(self.item(
            "together.text_group",
            "together.friend_text",
            Axis::Vertical,
            false,
            cx,
            |view, _, _, _| {
                div()
                    .id("together-remote-text")
                    .w_full()
                    .min_w_0()
                    .max_h(px(160.))
                    .overflow_y_scroll()
                    .child(if view.snapshot.remote.text.is_empty() {
                        "—".into()
                    } else {
                        view.snapshot.remote.text.clone()
                    })
                    .into_any_element()
            },
        ));
        let mut friend = SettingGroup::new()
            .title(self.label("together.friend_group"))
            .item(self.item(
                "together.friend_group",
                "together.friend_model",
                Axis::Horizontal,
                false,
                cx,
                |view, options, window, _| {
                    Select::new(&view.model_select)
                        .disabled(options.is_disabled())
                        .menu_width(dropdown_menu_width(
                            window,
                            view.models.iter().map(|choice| choice.title.clone()),
                        ))
                        .into_any_element()
                },
            ));
        if layout.model_status == TogetherModelStatus::Loading {
            friend = friend.item(self.custom("together.friend_group", cx, |view, _, _| {
                div()
                    .text_sm()
                    .child(view.label("together.loading_model"))
                    .into_any_element()
            }));
        } else if layout.model_status == TogetherModelStatus::Failed {
            friend = friend.item(self.custom("together.friend_group", cx, |view, _, cx| {
                Button::new("together-retry-model")
                    .label(view.label("together.retry_model"))
                    .disabled(view.snapshot.phase != Phase::Connected)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.layout.command(TogetherLayoutCommand::RetryModel);
                        cx.notify();
                    }))
                    .into_any_element()
            }));
        }
        friend = friend
            .item(self.item(
                "together.friend_group",
                "together.friend_visible",
                Axis::Horizontal,
                false,
                cx,
                |view, options, _, cx| {
                    Switch::new("together-visible")
                        .checked(view.layout.snapshot().visible)
                        .disabled(options.is_disabled())
                        .on_click(cx.listener(|view, checked, _, cx| {
                            view.layout
                                .command(TogetherLayoutCommand::SetVisible(*checked));
                            cx.notify();
                        }))
                        .into_any_element()
                },
            ))
            .item(self.item(
                "together.friend_group",
                "together.friend_scale",
                Axis::Horizontal,
                false,
                cx,
                |view, _, _, cx| {
                    let scale = view.layout.snapshot().scale_percent;
                    div()
                        .id("together-scale")
                        .test_support()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("together-smaller")
                                .label("−")
                                .disabled(scale <= 25)
                                .on_click(cx.listener(|view, _, _, cx| {
                                    let scale = view.layout.snapshot().scale_percent;
                                    view.layout.command(TogetherLayoutCommand::SetScale(
                                        scale.saturating_sub(25).max(25),
                                    ));
                                    cx.notify();
                                })),
                        )
                        .child(format!("{scale}%"))
                        .child(
                            Button::new("together-larger")
                                .label("+")
                                .disabled(scale >= 400)
                                .on_click(cx.listener(|view, _, _, cx| {
                                    let scale = view.layout.snapshot().scale_percent;
                                    view.layout.command(TogetherLayoutCommand::SetScale(
                                        (scale + 25).min(400),
                                    ));
                                    cx.notify();
                                })),
                        )
                        .into_any_element()
                },
            ))
            .item(self.custom("together.friend_group", cx, |view, _, cx| {
                div()
                    .id("together-layout-actions")
                    .test_support()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("together-swap")
                            .label(view.label("together.swap"))
                            .disabled(view.snapshot.phase != Phase::Connected)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.layout.command(TogetherLayoutCommand::SwapPositions);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("together-reset-layout")
                            .label(view.label("together.reset_layout"))
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.layout.command(TogetherLayoutCommand::ResetLayout);
                                cx.notify();
                            })),
                    )
                    .into_any_element()
            }));
        vec![connection, input, text, friend]
    }

    fn clear_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.text_generation += 1;
        self.text
            .update(cx, |text, cx| text.set_value("", window, cx));
    }

    fn item<F>(
        &self,
        group: &'static str,
        key: &'static str,
        axis: Axis,
        disabled: bool,
        cx: &Context<Self>,
        render: F,
    ) -> SettingItem
    where
        F: Fn(&mut Self, &RenderOptions, &mut Window, &mut Context<Self>) -> AnyElement + 'static,
    {
        let view = cx.entity();
        SettingItem::new(
            self.label(key),
            SettingField::element(
                move |options: &RenderOptions, window: &mut Window, app: &mut App| {
                    view.update(app, |view, cx| render(view, options, window, cx))
                },
            ),
        )
        .layout(axis)
        .disabled(disabled)
        .keywords([self.label("navigation.together.title"), self.label(group)])
    }

    fn custom<F>(&self, group: &'static str, cx: &Context<Self>, render: F) -> SettingItem
    where
        F: Fn(&mut Self, &mut Window, &mut Context<Self>) -> AnyElement + 'static,
    {
        let view = cx.entity();
        SettingItem::render(
            move |_: &RenderOptions, window: &mut Window, app: &mut App| {
                view.update(app, |view, cx| render(view, window, cx))
            },
        )
        .keywords([self.label("navigation.together.title"), self.label(group)])
    }

    fn keys(id: &'static str, keys: &BTreeSet<u16>) -> AnyElement {
        let chord = if keys.is_empty() {
            "—".into()
        } else {
            key_chord(keys)
        };
        div()
            .id(id)
            .test_support()
            .w_full()
            .min_w_0()
            .child(chord)
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "tests/together.rs"]
mod tests;
