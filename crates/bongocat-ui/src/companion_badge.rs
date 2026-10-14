//! GPUI presentation for the friend nickname, held keys and temporary text.
use crate::SettingsLanguage;
use bongocat_network::{KeyboardMode, NetworkClient, NetworkSnapshot, SharePolicy};
use bongocat_platform::CompanionBadgePositioner;
use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme, Root};
use gpui_kit::{
    App, AppContext, Context, IntoElement, ParentElement, Render, Styled, WeakEntity, Window,
    WindowBackgroundAppearance, WindowHandle, WindowKind, WindowOptions, div, prelude::*, px,
};
use std::time::{Duration, Instant};

struct BadgeView {
    client: NetworkClient,
    nickname: String,
    text: String,
    keys: String,
    local_keys: String,
    wheel: Option<(u64, i16)>,
    wheel_changed: Instant,
    local_sharing: bool,
    local_precise: bool,
    language: SettingsLanguage,
    text_changed: Instant,
}

impl Render for BadgeView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = self.language.catalog_locale();
        div()
            .size_full()
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .rounded_lg()
            .when(self.local_sharing, |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_sm().child(bongocat_i18n::text(
                            locale,
                            if self.local_precise {
                                "together.badge_precise"
                            } else {
                                "together.badge_sharing"
                            },
                        )))
                        .child(
                            Button::new("badge-stop-sharing")
                                .label(bongocat_i18n::text(locale, "together.badge_stop"))
                                .on_click(cx.listener(|view, _, _, cx| {
                                    let _ = view.client.set_share(SharePolicy::default());
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(div().text_sm().child(self.nickname.clone()))
            .when(!self.local_keys.is_empty(), |this| {
                this.child(
                    div()
                        .text_sm()
                        .max_h(px(48.))
                        .overflow_hidden()
                        .child(format!(
                            "{}: {}",
                            bongocat_i18n::text(locale, "together.local_keys"),
                            self.local_keys
                        )),
                )
            })
            .when(!self.keys.is_empty(), |this| {
                this.child(
                    div()
                        .text_sm()
                        .max_h(px(48.))
                        .overflow_hidden()
                        .child(self.keys.clone()),
                )
            })
            .when(
                self.wheel.is_some() && self.wheel_changed.elapsed() < Duration::from_millis(800),
                |this| {
                    this.child(div().text_sm().child(format!(
                        "{} {}",
                        bongocat_i18n::text(locale, "together.mouse_wheel"),
                        if self.wheel.is_some_and(|(_, delta)| delta > 0) {
                            "↑"
                        } else {
                            "↓"
                        }
                    )))
                },
            )
            .when(
                !self.text.is_empty() && self.text_changed.elapsed() < Duration::from_secs(10),
                |this| {
                    this.child(
                        div()
                            .id("companion-text")
                            .w_full()
                            .max_h(px(160.))
                            .overflow_y_scroll()
                            .text_sm()
                            .child(self.text.clone()),
                    )
                },
            )
    }
}

pub struct CompanionBadgeHandle {
    positioner: Option<CompanionBadgePositioner>,
    window: WindowHandle<Root>,
    view: WeakEntity<BadgeView>,
}

pub fn open_companion_badge(
    client: NetworkClient,
    cx: &mut App,
) -> Result<CompanionBadgeHandle, String> {
    let mut view_entity = None;
    let mut positioner = None;
    let window = cx
        .open_window(
            WindowOptions {
                titlebar: None,
                focus: false,
                show: true,
                kind: WindowKind::PopUp,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            |window, cx| {
                let close_client = client.clone();
                window.on_window_should_close(cx, move |_, _| {
                    let _ = close_client.set_share(SharePolicy::default());
                    true
                });
                let view = cx.new(|_| BadgeView {
                    client,
                    nickname: String::new(),
                    text: String::new(),
                    keys: String::new(),
                    local_keys: String::new(),
                    wheel: None,
                    wheel_changed: Instant::now(),
                    local_sharing: false,
                    local_precise: false,
                    language: SettingsLanguage::English,
                    text_changed: Instant::now(),
                });
                view_entity = Some(view.downgrade());
                positioner = CompanionBadgePositioner::start(window).ok();
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .map_err(|_| "companion badge window unavailable")?;
    if positioner.is_none() {
        let _ = window.update(cx, |_, window, _| window.remove_window());
        return Err("companion badge positioning unavailable".into());
    }
    Ok(CompanionBadgeHandle {
        positioner,
        window,
        view: view_entity.ok_or("companion badge view unavailable")?,
    })
}

impl CompanionBadgeHandle {
    pub fn update(
        &self,
        snapshot: &NetworkSnapshot,
        language: SettingsLanguage,
        anchor: (i32, i32, u32),
        cx: &mut App,
    ) -> Result<(), String> {
        let (scale, logical_height) = self
            .window
            .update(cx, |_, window, cx| {
                let logical_height = self
                    .view
                    .update(cx, |view, cx| {
                        if view.text != snapshot.remote.text {
                            view.text_changed = Instant::now();
                            view.text.clone_from(&snapshot.remote.text);
                        }
                        view.nickname = snapshot.peer_nickname.clone().unwrap_or_default();
                        view.keys = crate::window::key_chord(&snapshot.remote.pressed);
                        view.local_keys = crate::window::key_chord(&snapshot.local_keys);
                        if view.wheel != snapshot.remote.wheel {
                            view.wheel = snapshot.remote.wheel;
                            view.wheel_changed = Instant::now();
                        }
                        let policy = snapshot.local_share;
                        view.local_sharing = !policy.paused
                            && (policy.keyboard != KeyboardMode::Off
                                || policy.mouse
                                || policy.text);
                        view.local_precise = policy.keyboard == KeyboardMode::Precise;
                        view.language = language;
                        cx.notify();
                        let text_height = if !view.text.is_empty()
                            && view.text_changed.elapsed() < Duration::from_secs(10)
                        {
                            ((view.text.chars().count() / 22 + view.text.lines().count()) as f32
                                * 18.0)
                                .min(160.0)
                        } else {
                            0.0
                        };
                        38.0 + text_height
                            + if view.keys.is_empty() { 0.0 } else { 48.0 }
                            + if view.local_keys.is_empty() {
                                0.0
                            } else {
                                48.0
                            }
                            + if view.wheel.is_some()
                                && view.wheel_changed.elapsed() < Duration::from_millis(800)
                            {
                                24.0
                            } else {
                                0.0
                            }
                            + if view.local_sharing { 40.0 } else { 0.0 }
                    })
                    .map_err(|_| "companion badge view released")?;
                Ok::<_, &str>((window.scale_factor(), logical_height))
            })
            .map_err(|_| "companion badge window released")?
            .map_err(str::to_owned)?;
        let height = (logical_height * scale) as u32;
        if let Some(positioner) = self.positioner.as_ref() {
            positioner.update((
                anchor.0,
                anchor.1.saturating_sub_unsigned(height),
                anchor
                    .2
                    .max((300.0 * scale) as u32)
                    .min((440.0 * scale) as u32),
                height,
            ));
        }
        Ok(())
    }

    pub fn verify_smoke_content(&self, expected: &str, cx: &mut App) -> Result<(), String> {
        self.window
            .update(cx, |_, window, cx| {
                let matches = self
                    .view
                    .update(cx, |view, _| view.text == expected)
                    .unwrap_or(false);
                if matches && window.viewport_size().height > px(0.) {
                    Ok(())
                } else {
                    Err("badge smoke content/viewport mismatch".into())
                }
            })
            .map_err(|_| "badge smoke window unavailable")?
    }

    pub fn close(mut self, cx: &mut App) {
        self.positioner.take();
        let _ = self
            .window
            .update(cx, |_, window, _| window.remove_window());
    }
}
