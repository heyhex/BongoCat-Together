//! Layout regression through the actual GPUI Settings/SettingGroup wrappers.
use super::*;
use gpui_kit::InputEvent as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext};

struct TogetherPageHarness {
    view: Entity<TogetherView>,
    group: Option<usize>,
}

impl Render for TogetherPageHarness {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let groups = self.view.update(cx, |view, cx| view.groups(window, cx));
        let group = self.group;
        div().size_full().child(
            Settings::new("together-layout-test")
                .sidebar_width(px(220.))
                .with_group_variant(GroupBoxVariant::Outline)
                .page(
                    SettingPage::new("Together").groups(groups.into_iter().enumerate().filter_map(
                        move |(index, value)| {
                            if group.is_none_or(|group| group == index) {
                                Some(value)
                            } else {
                                None
                            }
                        },
                    )),
                ),
        )
    }
}

fn frame(visual: &mut VisualTestContext) {
    visual.update(|window, cx| window.render_frame(cx));
    visual.update(|window, cx| window.render_frame(cx));
}

#[gpui_kit::test]
fn together_controls_fit_the_settings_content_width(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let service = bongocat_network::NetworkService::start().unwrap();
    for language in [
        SettingsLanguage::English,
        SettingsLanguage::ChineseSimplified,
    ] {
        for width in [800., 1000.] {
            for (group, target) in [
                (0, "together-invitation-field"),
                (1, "together-local-keys"),
                (2, "together-text-editor"),
                (3, "together-scale"),
            ] {
                let client = service.client();
                client
                    .set_share(SharePolicy {
                        text: true,
                        ..Default::default()
                    })
                    .unwrap();
                let (_, visual) = cx.add_window_view(move |window, cx| {
                    let view = cx.new(|cx| {
                        TogetherView::new(
                            client,
                            TogetherLayoutClient::default(),
                            language,
                            window,
                            cx,
                        )
                    });
                    view.update(cx, |view, cx| {
                        view.snapshot.phase = Phase::Connected;
                        view.snapshot.local_keys = (4..40).collect();
                        view.snapshot.remote.pressed = (4..30).collect();
                        view.snapshot.peer_nickname =
                            Some("昵称与昵称与昵称与昵称与昵称与昵称".into());
                        view.text.update(cx, |text, cx| {
                            text.set_value("今天一起写代码 🐱\n".repeat(20), window, cx)
                        });
                    });
                    let harness = cx.new(|_| TogetherPageHarness {
                        view,
                        group: Some(group),
                    });
                    Root::new(harness, window, cx)
                });
                visual.simulate_resize(size(px(width), px(600.)));
                frame(visual);
                let (bounds, sidebar_right) = visual.update(|window, _| {
                    (
                        window.find(target).bounds(),
                        window.find("0-0").bounds().right(),
                    )
                });
                assert!(
                    bounds.size.width > px(0.) && bounds.size.height > px(0.),
                    "{target} must be laid out"
                );
                assert!(
                    bounds.origin.x >= sidebar_right && bounds.right() <= px(width - 16.),
                    "{language:?}, width={width}, {target}: {bounds:?} must stay inside the settings pane"
                );
                if group < 3 {
                    assert!(
                        bounds.size.width > px(320.),
                        "{target} must use a full-width setting row, got {bounds:?}"
                    );
                }
                if group == 1 {
                    assert!(
                        bounds.size.height > px(30.),
                        "long key chords must wrap instead of overflowing to the right"
                    );
                }
            }
        }
    }
}

#[gpui_kit::test]
fn together_last_controls_are_reachable_by_scrolling(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let service = bongocat_network::NetworkService::start().unwrap();
    let client = service.client();
    let (_, visual) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| {
            TogetherView::new(
                client,
                TogetherLayoutClient::default(),
                SettingsLanguage::ChineseSimplified,
                window,
                cx,
            )
        });
        let harness = cx.new(|_| TogetherPageHarness { view, group: None });
        Root::new(harness, window, cx)
    });
    visual.simulate_resize(size(px(800.), px(600.)));
    frame(visual);
    for _ in 0..4 {
        visual.update(|window, cx| {
            window.dispatch_event(
                gpui_kit::ScrollWheelEvent {
                    position: point(px(700.), px(500.)),
                    delta: gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-1200.))),
                    modifiers: Default::default(),
                    touch_phase: gpui_kit::TouchPhase::Moved,
                }
                .to_platform_input(),
                cx,
            );
        });
        frame(visual);
    }
    let (bounds, visible) = visual.update(|window, _| {
        let actions = window.find("together-layout-actions");
        (actions.bounds(), actions.visible())
    });
    assert!(
        visible,
        "the final layout actions must be reachable through the page scroll"
    );
    assert!(
        bounds.origin.y >= px(0.) && bounds.bottom() <= px(600.),
        "last controls must remain in the viewport: {bounds:?}"
    );
    assert!(
        bounds.right() <= px(784.),
        "last controls must not be clipped on the right"
    );
}
