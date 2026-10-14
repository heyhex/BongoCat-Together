use super::*;
use crate::protocol::{Envelope, Message, ReceiverState, VERSION};
use std::time::{Duration, Instant};

fn wait(client: &NetworkClient, predicate: impl Fn(&NetworkSnapshot) -> bool) -> NetworkSnapshot {
    let start = Instant::now();
    loop {
        let snapshot = client.snapshot();
        if predicate(&snapshot) {
            return snapshot;
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "network state deadline: phase={:?}, error={:?}",
            snapshot.phase,
            snapshot.error
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn options(name: &str) -> RoomOptions {
    RoomOptions {
        nickname: name.into(),
        direct_only: true,
    }
}

#[test]
fn quic_two_peers_probes_unicode_revoke_and_disconnect() {
    let host = NetworkService::start().unwrap();
    let guest = NetworkService::start().unwrap();
    let a = host.client();
    let b = guest.client();
    a.create(options("猫咪 A")).unwrap();
    let code = wait(&a, |s| s.invitation.is_some()).invitation.unwrap();
    b.join(code, options("猫咪 B")).unwrap();
    assert_eq!(
        wait(&a, |s| s.phase == Phase::Connected)
            .peer_nickname
            .as_deref(),
        Some("猫咪 B")
    );
    wait(&b, |s| {
        s.phase == Phase::Connected && s.path == ConnectionPath::Direct
    });
    a.probe().unwrap();
    b.probe().unwrap();
    wait(&a, |s| s.probes_received == 1);
    wait(&b, |s| s.probes_received == 1);
    a.set_peer_info(PeerInfo {
        model_id: "keyboard".into(),
        built_in: true,
        scale_percent: 125,
    })
    .unwrap();
    wait(&b, |s| {
        s.peer_info
            .as_ref()
            .is_some_and(|i| i.model_id == "keyboard" && i.scale_percent == 125)
    });
    assert!(
        a.set_peer_info(PeerInfo {
            model_id: "../../secret".into(),
            ..Default::default()
        })
        .is_err()
    );
    let policy = SharePolicy {
        text: true,
        ..Default::default()
    };
    a.set_share(policy).unwrap();
    a.set_text("今天写代码 🐱\n一起摸鱼".into()).unwrap();
    wait(&b, |s| s.remote.text == "今天写代码 🐱\n一起摸鱼");
    a.set_text("修改了 😀".into()).unwrap();
    wait(&b, |s| s.remote.text == "修改了 😀");
    a.set_share(SharePolicy {
        mouse: true,
        text: true,
        ..Default::default()
    })
    .unwrap();
    wait(&b, |s| s.remote.share.mouse && s.remote.text == "修改了 😀");
    a.set_share(SharePolicy::default()).unwrap();
    wait(&b, |s| s.remote.text.is_empty() && !s.remote.share.text);
    assert_eq!(
        a.set_text("must never arrive".into()),
        Err(NetworkError::Text)
    );
    assert!(b.snapshot().remote.text.is_empty());
    a.disconnect();
    wait(&a, |s| s.phase == Phase::Offline);
    wait(&b, |s| s.phase == Phase::Offline);
    assert!(a.snapshot().invitation.is_none());
}

#[test]
fn wrong_capability_is_rejected_and_host_remains_available() {
    let host = NetworkService::start().unwrap();
    let intruder = NetworkService::start().unwrap();
    let guest = NetworkService::start().unwrap();
    let a = host.client();
    a.create(options("host")).unwrap();
    let code = wait(&a, |s| s.invitation.is_some()).invitation.unwrap();
    let mut invite = transport::Invite::decode(&code).unwrap();
    invite.authorization[0] ^= 1;
    intruder
        .client()
        .join(invite.encode().unwrap(), options("intruder"))
        .unwrap();
    wait(&intruder.client(), |s| s.phase == Phase::Failed);
    guest.client().join(code, options("friend")).unwrap();
    wait(&a, |s| s.phase == Phase::Connected);
    a.create(options("duplicate")).unwrap();
    wait(&a, |s| s.error == Some(NetworkError::Busy));
    assert_eq!(a.snapshot().peer_nickname.as_deref(), Some("friend"));
}

#[test]
fn invitations_are_bounded_versioned_and_expire() {
    assert!(matches!(
        transport::Invite::decode("bad"),
        Err(NetworkError::InvalidInvite)
    ));
    assert!(matches!(
        transport::Invite::decode(&"b".repeat(9000)),
        Err(NetworkError::InvalidInvite)
    ));
    let endpoint_id = iroh::SecretKey::generate().public();
    let mut invite = transport::Invite::new(iroh::EndpointAddr::new(endpoint_id), true).unwrap();
    let encoded = invite.encode().unwrap();
    assert!(transport::Invite::decode(&encoded).is_ok());
    invite.version += 1;
    assert!(transport::Invite::decode(&invite.encode().unwrap()).is_err());
    invite.version = VERSION;
    invite.expires_at = 1;
    assert!(transport::Invite::decode(&invite.encode().unwrap()).is_err());
}

fn envelope(sequence: u64, message: Message) -> Envelope {
    Envelope {
        version: VERSION,
        session: [7; 16],
        sequence,
        message,
    }
}

#[test]
fn precise_keys_chords_release_duplicates_gaps_and_privacy() {
    let mut state = ReceiverState::new([7; 16]);
    assert!(state.apply(&envelope(1, Message::KeyboardDown(4))).is_err());
    let mut state = ReceiverState::new([7; 16]);
    let share = SharePolicy {
        keyboard: KeyboardMode::Precise,
        mouse: true,
        ..Default::default()
    };
    state
        .apply(&envelope(1, Message::ShareStateChanged(share, 1)))
        .unwrap();
    state
        .apply(&envelope(2, Message::KeyboardDown(0xe0)))
        .unwrap();
    state.apply(&envelope(3, Message::KeyboardDown(6))).unwrap();
    assert_eq!(state.peer.pressed.len(), 2);
    assert!(!state.apply(&envelope(3, Message::KeyboardDown(6))).unwrap());
    state.apply(&envelope(4, Message::KeyboardUp(6))).unwrap();
    assert_eq!(state.peer.pressed.len(), 1);
    state
        .apply(&envelope(5, Message::MouseButtonDown(0)))
        .unwrap();
    state
        .apply(&envelope(6, Message::MouseButtonUp(0)))
        .unwrap();
    state
        .apply(&envelope(7, Message::MouseMove([0.2, 0.8])))
        .unwrap();
    state
        .apply(&envelope(8, Message::MouseWheel(-120)))
        .unwrap();
    assert_eq!(state.peer.wheel, Some((8, -120)));
    state
        .apply(&envelope(
            9,
            Message::ShareStateChanged(SharePolicy::default(), 2),
        ))
        .unwrap();
    assert!(state.peer.pressed.is_empty());
    assert!(state.peer.cursor.is_none());
    assert!(state.apply(&envelope(11, Message::Heartbeat)).is_err());
}

#[test]
fn unicode_limits_reset_and_invalid_positions() {
    let mut state = ReceiverState::new([7; 16]);
    state
        .apply(&envelope(
            1,
            Message::ShareStateChanged(
                SharePolicy {
                    text: true,
                    mouse: true,
                    ..Default::default()
                },
                1,
            ),
        ))
        .unwrap();
    state
        .apply(&envelope(2, Message::TextUpdate("中😀\n文".into())))
        .unwrap();
    assert_eq!(state.peer.text, "中😀\n文");
    state
        .apply(&envelope(3, Message::TextUpdate(String::new())))
        .unwrap();
    assert!(state.peer.text.is_empty());
    assert!(
        state
            .apply(&envelope(4, Message::TextUpdate("中".repeat(513))))
            .is_err()
    );
    assert!(
        state
            .apply(&envelope(5, Message::MouseMove([f32::NAN, 0.0])))
            .is_err()
    );
    state.apply(&envelope(6, Message::StateReset)).unwrap();
    assert!(state.peer.text.is_empty());
    assert_eq!(state.peer.share, SharePolicy::default());
}

#[test]
fn service_shutdown_cancels_a_waiting_room_and_has_no_sockets_offline() {
    let start = Instant::now();
    let service = NetworkService::start().unwrap();
    assert_eq!(service.client().snapshot().phase, Phase::Offline);
    service.client().create(options("host")).unwrap();
    wait(&service.client(), |s| s.phase == Phase::Waiting);
    drop(service);
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn reconnect_uses_same_authenticated_identity_and_resets_sharing() {
    let host = NetworkService::start().unwrap();
    let guest = NetworkService::start().unwrap();
    let a = host.client();
    let b = guest.client();
    a.create(options("host")).unwrap();
    let code = wait(&a, |s| s.invitation.is_some()).invitation.unwrap();
    b.join(code, options("friend")).unwrap();
    wait(&a, |s| s.phase == Phase::Connected);
    wait(&b, |s| s.phase == Phase::Connected);
    a.set_share(SharePolicy {
        text: true,
        ..Default::default()
    })
    .unwrap();
    a.set_text("ephemeral".into()).unwrap();
    wait(&b, |s| s.remote.text == "ephemeral");
    a.submit(Command::TransientDisconnect).unwrap();
    wait(&a, |s| {
        s.phase == Phase::Connected && s.reconnect_attempt > 0
    });
    wait(&b, |s| {
        s.phase == Phase::Connected && s.reconnect_attempt > 0
    });
    assert!(b.snapshot().remote.text.is_empty());
    assert!(!a.snapshot().local_share.text);
    b.probe().unwrap();
    wait(&a, |s| s.probes_received > 0);
}

#[test]
fn third_peer_is_refused_without_replacing_friend() {
    let host = NetworkService::start().unwrap();
    let friend = NetworkService::start().unwrap();
    let third = NetworkService::start().unwrap();
    let a = host.client();
    a.create(options("host")).unwrap();
    let code = wait(&a, |s| s.invitation.is_some()).invitation.unwrap();
    friend
        .client()
        .join(code.clone(), options("friend"))
        .unwrap();
    wait(&a, |s| s.phase == Phase::Connected);
    third.client().join(code, options("third")).unwrap();
    wait(&third.client(), |s| s.phase == Phase::Failed);
    assert_eq!(a.snapshot().phase, Phase::Connected);
    assert_eq!(a.snapshot().peer_nickname.as_deref(), Some("friend"));
}

#[test]
fn oversized_wire_length_is_rejected_before_payload_allocation() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let host = transport::bind(true).await.unwrap();
            let guest = transport::bind(true).await.unwrap();
            let (a, b) = tokio::join!(
                async { host.accept().await.unwrap().await.unwrap() },
                guest.connect(host.addr(), transport::ALPN)
            );
            let b = b.unwrap();
            let sent = async {
                let (mut send, _) = b.open_bi().await.unwrap();
                send.write_all(&((protocol::MAX_FRAME + 1) as u32).to_be_bytes())
                    .await
                    .unwrap();
                send.finish().unwrap();
                send
            };
            let receive = async {
                let (_, mut recv) = a.accept_bi().await.unwrap();
                assert!(matches!(
                    transport::read(&mut recv).await,
                    Err(NetworkError::Protocol)
                ));
            };
            let _ = tokio::join!(sent, receive);
            transport::close(host).await;
            transport::close(guest).await;
        });
}

#[test]
fn authorized_edges_datagrams_pause_and_abstract_mode() {
    let host = NetworkService::start().unwrap();
    let guest = NetworkService::start().unwrap();
    let a = host.client();
    let b = guest.client();
    a.create(options("host")).unwrap();
    let code = wait(&a, |s| s.invitation.is_some()).invitation.unwrap();
    b.join(code, options("friend")).unwrap();
    wait(&a, |s| s.phase == Phase::Connected);
    wait(&b, |s| s.phase == Phase::Connected);
    a.publish_input(SharedInput::Key {
        usage: 4,
        down: true,
    })
    .unwrap();
    assert!(b.snapshot().remote.pressed.is_empty());
    a.set_share(SharePolicy {
        keyboard: KeyboardMode::Precise,
        mouse: true,
        ..Default::default()
    })
    .unwrap();
    a.publish_input(SharedInput::Key {
        usage: 0xe0,
        down: true,
    })
    .unwrap();
    a.publish_input(SharedInput::Key {
        usage: 6,
        down: true,
    })
    .unwrap();
    a.publish_input(SharedInput::MouseButton {
        button: 1,
        down: true,
    })
    .unwrap();
    for i in 0..5000 {
        a.publish_cursor([i as f32 / 5000.0, 0.25]);
    }
    wait(&b, |s| {
        s.remote.pressed.len() == 2 && s.remote.mouse_buttons.contains(&1)
    });
    wait(&b, |s| s.remote.cursor.is_some_and(|p| p[0] > 0.99));
    a.publish_input(SharedInput::Key {
        usage: 6,
        down: false,
    })
    .unwrap();
    a.publish_input(SharedInput::Key {
        usage: 0xe0,
        down: false,
    })
    .unwrap();
    a.publish_input(SharedInput::MouseButton {
        button: 1,
        down: false,
    })
    .unwrap();
    wait(&b, |s| {
        s.remote.pressed.is_empty() && s.remote.mouse_buttons.is_empty()
    });
    a.set_share(SharePolicy {
        keyboard: KeyboardMode::Abstract,
        ..Default::default()
    })
    .unwrap();
    a.publish_input(SharedInput::AbstractKeyboard(true))
        .unwrap();
    wait(&b, |s| s.remote.abstract_keyboard);
    assert!(b.snapshot().remote.pressed.is_empty());
    a.set_share(SharePolicy {
        paused: true,
        keyboard: KeyboardMode::Abstract,
        ..Default::default()
    })
    .unwrap();
    a.publish_input(SharedInput::AbstractKeyboard(true))
        .unwrap();
    wait(&b, |s| s.remote.share.paused && !s.remote.abstract_keyboard);
}

#[test]
fn full_edge_queue_revokes_sharing_without_blocking_shutdown() {
    let service = NetworkService::start().unwrap();
    let client = service.client();
    client.create(options("host")).unwrap();
    wait(&client, |s| s.phase == Phase::Waiting);
    client
        .set_share(SharePolicy {
            keyboard: KeyboardMode::Precise,
            ..Default::default()
        })
        .unwrap();
    let mut full = false;
    for _ in 0..100 {
        full |= client.publish_input(SharedInput::Key {
            usage: 4,
            down: true,
        }) == Err(NetworkError::Busy);
    }
    assert!(full);
    assert_eq!(client.sharing_policy(), SharePolicy::default());
    let start = Instant::now();
    client.disconnect();
    wait(&client, |s| s.phase == Phase::Offline);
    assert!(start.elapsed() < Duration::from_secs(3));
}
