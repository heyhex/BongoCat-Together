use super::*;
use bongocat_network::{
    ConnectionPath, KeyboardMode, NetworkClient, NetworkService, Phase, RoomOptions, SharePolicy,
    SharedInput,
};
use bongocat_runtime::RuntimeClient;

async fn until(mut predicate: impl FnMut() -> bool) -> Result<(), String> {
    let start = Instant::now();
    while !predicate() {
        if start.elapsed() > Duration::from_secs(15) {
            return Err("Together smoke deadline exceeded".into());
        }
        Timer::after(Duration::from_millis(25)).await;
    }
    Ok(())
}

pub(crate) async fn run(
    client: NetworkClient,
    friend: Rc<RefCell<together_overlay::TogetherOverlay>>,
    local: RuntimeClient,
    cx: &mut AsyncApp,
) -> Result<(), String> {
    let guest = NetworkService::start().map_err(|e| e.to_string())?;
    let guest_client = guest.client();
    guest_client
        .set_peer_info(bongocat_network::PeerInfo {
            model_id: "missing_peer_model".into(),
            built_in: true,
            scale_percent: 100,
        })
        .map_err(|e| e.to_string())?;
    client
        .create(RoomOptions {
            nickname: "Smoke host".into(),
            direct_only: true,
        })
        .map_err(|e| e.to_string())?;
    until(|| client.snapshot().invitation.is_some()).await?;
    guest_client
        .join(
            client.snapshot().invitation.ok_or("missing smoke invite")?,
            RoomOptions {
                nickname: "好友 🐱".into(),
                direct_only: true,
            },
        )
        .map_err(|e| e.to_string())?;
    until(|| {
        client.snapshot().phase == Phase::Connected
            && client.snapshot().path == ConnectionPath::Direct
    })
    .await?;
    until(|| {
        client
            .snapshot()
            .peer_info
            .as_ref()
            .is_some_and(|info| info.model_id == "missing_peer_model")
    })
    .await?;
    until(|| friend.borrow().smoke_frame_count() >= 10).await?;
    until(|| {
        friend.borrow().smoke_runtime().is_some_and(|runtime| {
            runtime
                .snapshot()
                .active_model
                .is_some_and(|model| model.id.as_str() == "standard")
        })
    })
    .await?;
    guest_client
        .set_peer_info(bongocat_network::PeerInfo {
            model_id: "keyboard".into(),
            built_in: true,
            scale_percent: 100,
        })
        .map_err(|e| e.to_string())?;
    until(|| {
        friend.borrow().smoke_runtime().is_some_and(|runtime| {
            runtime
                .snapshot()
                .active_model
                .is_some_and(|model| model.id.as_str() == "keyboard")
        })
    })
    .await?;
    let layout = cx.update(|cx| cx.global::<ProductCoordinator>().together_layout.clone());
    layout.command(bongocat_ui_protocol::TogetherLayoutCommand::SetScale(125));
    until(|| {
        friend
            .borrow()
            .smoke_runtime()
            .is_some_and(|runtime| runtime.snapshot().overlay_settings.scale_percent == 125)
    })
    .await?;
    layout.command(bongocat_ui_protocol::TogetherLayoutCommand::SetVisible(
        false,
    ));
    until(|| {
        friend
            .borrow()
            .smoke_runtime()
            .is_some_and(|runtime| !runtime.snapshot().overlay_visible)
    })
    .await?;
    layout.command(bongocat_ui_protocol::TogetherLayoutCommand::SetVisible(
        true,
    ));
    layout.command(bongocat_ui_protocol::TogetherLayoutCommand::SelectModel(
        bongocat_ui_protocol::SettingsModelKey {
            id: "keyboard".into(),
            origin: bongocat_ui_protocol::SettingsModelOrigin::BuiltIn,
        },
    ));
    until(|| {
        friend.borrow().smoke_runtime().is_some_and(|runtime| {
            runtime
                .snapshot()
                .active_model
                .is_some_and(|model| model.id.as_str() == "keyboard")
        })
    })
    .await?;
    layout.command(bongocat_ui_protocol::TogetherLayoutCommand::SelectModel(
        bongocat_ui_protocol::SettingsModelKey {
            id: "not_installed".into(),
            origin: bongocat_ui_protocol::SettingsModelOrigin::Imported,
        },
    ));
    until(|| layout.snapshot().model_status == bongocat_ui_protocol::TogetherModelStatus::Failed)
        .await?;
    if !friend.borrow().smoke_runtime().is_some_and(|runtime| {
        runtime
            .snapshot()
            .active_model
            .is_some_and(|model| model.id.as_str() == "keyboard")
    }) {
        return Err("failed friend model replaced the usable cat".into());
    }
    layout.command(bongocat_ui_protocol::TogetherLayoutCommand::RetryModel);
    until(|| layout.snapshot().model_status == bongocat_ui_protocol::TogetherModelStatus::Failed)
        .await?;
    layout.command(bongocat_ui_protocol::TogetherLayoutCommand::SelectModel(
        bongocat_ui_protocol::SettingsModelKey {
            id: "keyboard".into(),
            origin: bongocat_ui_protocol::SettingsModelOrigin::BuiltIn,
        },
    ));
    until(|| layout.snapshot().model_status == bongocat_ui_protocol::TogetherModelStatus::Ready)
        .await?;
    let before_swap = friend
        .borrow()
        .window_bounds()
        .ok_or("friend bounds missing")?;
    layout.command(bongocat_ui_protocol::TogetherLayoutCommand::SwapPositions);
    until(|| {
        friend
            .borrow()
            .window_bounds()
            .is_some_and(|bounds| bounds.x != before_swap.x || bounds.y != before_swap.y)
    })
    .await?;
    layout.command(bongocat_ui_protocol::TogetherLayoutCommand::SwapPositions);
    until(|| {
        friend
            .borrow()
            .window_bounds()
            .is_some_and(|bounds| bounds.x == before_swap.x && bounds.y == before_swap.y)
    })
    .await?;
    layout.command(bongocat_ui_protocol::TogetherLayoutCommand::ResetLayout);
    let remote = friend
        .borrow()
        .smoke_runtime()
        .ok_or("missing friend runtime")?;
    let local_before = local.snapshot().input.pressed_key_count;
    guest_client
        .set_share(SharePolicy {
            keyboard: KeyboardMode::Precise,
            text: true,
            mouse: true,
            ..Default::default()
        })
        .map_err(|_| "smoke share failed")?;
    guest_client
        .publish_input(SharedInput::Key {
            usage: 4,
            down: true,
        })
        .map_err(|_| "friend test edge rejected")?;
    until(|| remote.snapshot().model_input.left_hand_down).await?;
    if local.snapshot().input.pressed_key_count != local_before {
        return Err("local input changed during friend isolation probe".into());
    }
    guest_client
        .publish_input(SharedInput::Key {
            usage: 4,
            down: false,
        })
        .map_err(|_| "friend test release rejected")?;
    until(|| !remote.snapshot().model_input.left_hand_down).await?;
    guest_client
        .set_text("今天写代码也要一起摸鱼 🐱\n中文 · Emoji · 多行".into())
        .map_err(|_| "smoke bubble text rejected")?;
    until(|| !client.snapshot().remote.text.is_empty()).await?;
    Timer::after(Duration::from_millis(500)).await;
    cx.update(|cx| {
        let badge = cx
            .global_mut::<ProductCoordinator>()
            .together_badge
            .take()
            .ok_or("friend badge was not created")?;
        let result =
            badge.verify_smoke_content("今天写代码也要一起摸鱼 🐱\n中文 · Emoji · 多行", cx);
        cx.global_mut::<ProductCoordinator>().together_badge = Some(badge);
        result
    })?;
    client
        .set_share(SharePolicy {
            text: true,
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
    client
        .set_text("中文输入框 😀\n第二行".into())
        .map_err(|e| e.to_string())?;
    until(|| guest_client.snapshot().remote.text == "中文输入框 😀\n第二行").await?;
    client.indicator_unavailable();
    until(|| guest_client.snapshot().remote.text.is_empty()).await?;
    if client.sharing_policy() != SharePolicy::default() {
        return Err("indicator failure did not revoke sharing".into());
    }
    client.disconnect();
    until(|| {
        client.snapshot().phase == Phase::Offline && friend.borrow().smoke_runtime().is_none()
    })
    .await?;
    Ok(())
}
