use crate::protocol::{Envelope, Message, PeerState, ReceiverState, SharePolicy, VERSION};
use crate::transport::{self, ALPN, Invite, TIMEOUT};
use crate::{
    Command, Control, NetworkError, NetworkSnapshot, Phase, Privacy, SharedInput, State, update,
    validate_nickname,
};
use async_channel::Receiver;
use iroh::{
    Endpoint, EndpointId,
    endpoint::{Connection, RecvStream, SendStream},
};
use std::time::{Duration, Instant};
use tokio::sync::watch;

pub(crate) async fn run(
    commands: Receiver<Command>,
    mut control: watch::Receiver<Control>,
    privacy: watch::Sender<Privacy>,
    state: State,
) {
    loop {
        let command = tokio::select! {
            biased;
            result = control.changed() => {
                if result.is_err() || control.borrow_and_update().shutdown { break; }
                while commands.try_recv().is_ok() {}
                update(&state, |s| *s = NetworkSnapshot { revision: s.revision, ..Default::default() });
                continue;
            }
            command = commands.recv() => match command { Ok(c) => c, Err(_) => break },
        };
        let (options, invitation) = match command {
            Command::Create(options) => (options, None),
            Command::Join {
                invitation,
                options,
            } => (options, Some(invitation)),
            _ => {
                update(&state, |s| s.error = Some(NetworkError::Connection));
                continue;
            }
        };
        update(&state, |s| {
            *s = NetworkSnapshot {
                revision: s.revision,
                phase: Phase::Starting,
                ..Default::default()
            }
        });
        privacy.send_modify(|p| {
            p.revision += 1;
            p.policy = SharePolicy::default();
        });
        let invite = invitation.as_deref().map(Invite::decode).transpose();
        let result = match invite {
            Ok(invite) => {
                let direct_only =
                    options.direct_only || invite.as_ref().is_some_and(|i| i.direct_only);
                let bound = tokio::select! {
                    biased;
                    _ = control.changed() => None,
                    bound = transport::bind(direct_only) => Some(bound),
                };
                match bound {
                    Some(Ok(endpoint)) => {
                        let result = tokio::select! {
                            biased;
                            _ = control.changed() => Ok(()),
                            result = room(&endpoint, invite, options.nickname, direct_only, &commands, &privacy, &state) => result,
                        };
                        transport::close(endpoint).await;
                        result
                    }
                    Some(Err(error)) => Err(error),
                    None => Ok(()),
                }
            }
            Err(error) => Err(error),
        };
        if control.borrow_and_update().shutdown {
            break;
        }
        while commands.try_recv().is_ok() {}
        update(&state, |s| {
            let revision = s.revision;
            *s = NetworkSnapshot {
                revision,
                phase: if result.is_err() {
                    Phase::Failed
                } else {
                    Phase::Offline
                },
                error: result.err(),
                ..Default::default()
            };
        });
    }
    commands.close();
    update(&state, |s| {
        *s = NetworkSnapshot {
            revision: s.revision,
            ..Default::default()
        }
    });
}

async fn room(
    endpoint: &Endpoint,
    invitation: Option<Invite>,
    nickname: String,
    direct_only: bool,
    commands: &Receiver<Command>,
    privacy: &watch::Sender<Privacy>,
    state: &State,
) -> Result<(), NetworkError> {
    let host = invitation.is_none();
    let invite = match invitation {
        Some(invite) => invite,
        None => Invite::new(endpoint.addr(), direct_only)?,
    };
    if host {
        let code = invite.encode()?;
        update(state, |s| {
            s.phase = Phase::Waiting;
            s.invitation = Some(code);
        });
    }
    let mut pinned = None;
    let mut reconnect = 0;
    loop {
        let connected = if host {
            accept(endpoint, &invite, &nickname, pinned, state).await
        } else {
            update(state, |s| {
                s.phase = if reconnect == 0 {
                    Phase::Connecting
                } else {
                    Phase::Reconnecting
                }
            });
            connect(endpoint, &invite, &nickname, direct_only).await
        };
        let (connection, send, recv, peer_name) = match connected {
            Ok(connected) => connected,
            Err(error) if !host && error != NetworkError::Unauthorized && reconnect < 3 => {
                reconnect += 1;
                update(state, |s| {
                    s.phase = Phase::Reconnecting;
                    s.error = Some(error);
                    s.reconnect_attempt = reconnect;
                });
                tokio::time::sleep(Duration::from_secs(1 << (reconnect - 1))).await;
                continue;
            }
            Err(error) => return Err(error),
        };
        pinned = Some(connection.remote_id());
        update(state, |s| {
            s.phase = Phase::Connected;
            s.peer_nickname = Some(peer_name);
            s.peer_info = None;
            s.error = None;
            s.remote = PeerState::default();
            s.local_share = SharePolicy::default();
            s.reconnect_attempt = reconnect;
        });
        privacy.send_modify(|p| {
            p.revision += 1;
            p.policy = SharePolicy::default();
        });
        let result = session(
            endpoint,
            &connection,
            send,
            recv,
            &invite,
            commands,
            privacy.subscribe(),
            state,
        )
        .await;
        connection.close(0u32.into(), b"session ended");
        update(state, |s| {
            s.remote = PeerState::default();
            s.local_share = SharePolicy::default();
            s.path = Default::default();
            s.rtt_ms = None;
        });
        match result {
            Ok(()) => return Ok(()),
            Err(NetworkError::Protocol | NetworkError::Unauthorized) => return result,
            Err(error) => {
                reconnect += 1;
                if !host && reconnect > 3 {
                    return Err(error);
                }
                update(state, |s| {
                    s.phase = Phase::Reconnecting;
                    s.error = Some(error);
                    s.reconnect_attempt = reconnect;
                });
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

type Connected = (Connection, SendStream, RecvStream, String);

async fn connect(
    endpoint: &Endpoint,
    invite: &Invite,
    nickname: &str,
    direct_only: bool,
) -> Result<Connected, NetworkError> {
    let mut address = invite.address.clone();
    if direct_only {
        address.addrs.retain(|a| a.is_ip());
    }
    tokio::time::timeout(TIMEOUT, async {
        let connection = endpoint.connect(address, ALPN).await.map_err(|_| NetworkError::Connection)?;
        let (mut send, mut recv) = connection.open_bi().await.map_err(|_| NetworkError::Connection)?;
        transport::write(&mut send, &Envelope { version: VERSION, session: invite.session, sequence: 0,
            message: Message::Join { authorization: invite.authorization, nickname: nickname.into() } }).await?;
        let reply = transport::read(&mut recv).await.map_err(|error| {
            if matches!(connection.close_reason(), Some(iroh::endpoint::ConnectionError::ApplicationClosed(ref close)) if close.error_code == 1u32.into()) {
                NetworkError::Unauthorized
            } else { error }
        })?;
        if reply.version != VERSION || reply.session != invite.session || reply.sequence != 0 {
            return Err(NetworkError::Protocol);
        }
        match reply.message {
            Message::Accept { nickname } => {
                validate_nickname(&nickname).map_err(|_| NetworkError::Protocol)?;
                Ok((connection, send, recv, nickname))
            }
            _ => Err(NetworkError::Unauthorized),
        }
    }).await.map_err(|_| NetworkError::Timeout)?
}

async fn accept(
    endpoint: &Endpoint,
    invite: &Invite,
    nickname: &str,
    pinned: Option<EndpointId>,
    state: &State,
) -> Result<Connected, NetworkError> {
    loop {
        if pinned.is_none() && transport::now_seconds() >= invite.expires_at {
            return Err(NetworkError::InvalidInvite);
        }
        let incoming = tokio::time::timeout(Duration::from_secs(1), endpoint.accept()).await;
        let incoming = match incoming {
            Ok(Some(incoming)) => incoming,
            Ok(None) => return Err(NetworkError::Endpoint),
            Err(_) => continue,
        };
        let authenticated = tokio::time::timeout(Duration::from_secs(3), async {
            let connection = incoming.await.map_err(|_| NetworkError::Connection)?;
            let (mut send, mut recv) = connection
                .accept_bi()
                .await
                .map_err(|_| NetworkError::Connection)?;
            let envelope = transport::read(&mut recv).await?;
            let peer_name = match envelope.message {
                Message::Join {
                    authorization,
                    nickname: peer_name,
                } if envelope.version == VERSION
                    && envelope.session == invite.session
                    && envelope.sequence == 0
                    && authorization == invite.authorization
                    && pinned.is_none_or(|id| id == connection.remote_id())
                    && validate_nickname(&peer_name).is_ok() =>
                {
                    peer_name
                }
                _ => {
                    let _ = transport::write(
                        &mut send,
                        &Envelope {
                            version: VERSION,
                            session: invite.session,
                            sequence: 0,
                            message: Message::Reject,
                        },
                    )
                    .await;
                    connection.close(1u32.into(), b"authorization rejected");
                    return Err(NetworkError::Unauthorized);
                }
            };
            transport::write(
                &mut send,
                &Envelope {
                    version: VERSION,
                    session: invite.session,
                    sequence: 0,
                    message: Message::Accept {
                        nickname: nickname.into(),
                    },
                },
            )
            .await?;
            Ok((connection, send, recv, peer_name))
        })
        .await;
        match authenticated {
            Ok(Ok(connected)) => return Ok(connected),
            _ => {
                // Anonymous code only; no endpoint IDs, invite or nickname in logs.
                update(state, |s| s.error = Some(NetworkError::Unauthorized));
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn session(
    endpoint: &Endpoint,
    connection: &Connection,
    mut send: SendStream,
    mut recv: RecvStream,
    invite: &Invite,
    commands: &Receiver<Command>,
    mut privacy: watch::Receiver<Privacy>,
    state: &State,
) -> Result<(), NetworkError> {
    let (incoming_tx, incoming_rx) = async_channel::bounded(64);
    // Reading an entire frame in its own task is essential: cancelling a
    // read_exact halfway through a frame on each metrics tick corrupts framing.
    let reader = tokio::spawn(async move {
        loop {
            let item = transport::read(&mut recv).await;
            let failed = item.is_err();
            if incoming_tx.send(item).await.is_err() || failed {
                break;
            }
        }
    });
    // Cancellation also drops this guard, so the reader never outlives its
    // session. Code 100 distinguishes deliberate room exit from network loss.
    struct SessionGuard {
        reader: tokio::task::JoinHandle<()>,
        connection: Option<Connection>,
    }
    impl Drop for SessionGuard {
        fn drop(&mut self) {
            self.reader.abort();
            if let Some(c) = self.connection.take() {
                c.close(100u32.into(), b"room closed");
            }
        }
    }
    let mut guard = SessionGuard {
        reader,
        connection: Some(connection.clone()),
    };
    let result = async {
        let mut sequence = 0u64;
        let mut receive = ReceiverState::new(invite.session);
        let mut metrics = tokio::time::interval(Duration::from_millis(250));
        metrics.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last_received = Instant::now();
        let mut last_heartbeat = Instant::now();
        let mut mouse_tick = tokio::time::interval(Duration::from_millis(33));
        mouse_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut mouse_sequence = 0u64;
        let mut remote_mouse_sequence = 0u64;
        let mut rate_start = Instant::now();
        let mut rate = 0u32;
        let mut sent_info = None;
        let mut last_privacy = *privacy.borrow();
        let mut local_text = String::new();
        send_message(&mut send, invite, &mut sequence, Message::StateSnapshot(PeerState::default())).await?;
        loop {
            tokio::select! {
                biased;
                error = connection.closed() => {
                    return if matches!(error, iroh::endpoint::ConnectionError::ApplicationClosed(ref close) if close.error_code == 100u32.into()) {
                        Ok(())
                    } else { Err(NetworkError::Connection) };
                }
                _ = privacy.changed() => {
                    let current = *privacy.borrow_and_update();
                    let policy = current.policy;
                    // Unrelated sharing switches must not blank enabled text.
                    // A missed intermediate revision may have disabled text:
                    // erase the cache rather than implicitly redisclose it.
                    let keep_text = current.revision == last_privacy.revision.saturating_add(1)
                        && last_privacy.policy.text && !last_privacy.policy.paused
                        && policy.text && !policy.paused;
                    if !keep_text { local_text.clear(); }
                    last_privacy = current;
                    update(state, |s| {
                        s.local_share = policy;
                        if s.error != Some(NetworkError::IndicatorUnavailable) { s.error = None; }
                    });
                    send_message(&mut send, invite, &mut sequence, Message::ShareStateChanged(policy, current.revision)).await?;
                    if keep_text && !local_text.is_empty() {
                        send_message(&mut send, invite, &mut sequence, Message::TextUpdate(local_text.clone())).await?;
                    }
                }
                incoming = endpoint.accept() => {
                    if let Some(incoming) = incoming { incoming.refuse(); }
                }
                datagram = connection.read_datagram() => {
                    let bytes = datagram.map_err(|_| NetworkError::Connection)?;
                    if rate_start.elapsed() >= Duration::from_secs(1) { rate = 0; rate_start = Instant::now(); }
                    rate += 1;
                    if rate > 240 { return Err(NetworkError::Protocol); }
                    if bytes.len() > 256 { return Err(NetworkError::Protocol); }
                    let datagram: crate::protocol::MouseDatagram = serde_json::from_slice(&bytes).map_err(|_| NetworkError::Protocol)?;
                    if datagram.version != VERSION || datagram.session != invite.session || datagram.position.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)) { return Err(NetworkError::Protocol); }
                    if datagram.sequence > remote_mouse_sequence {
                        remote_mouse_sequence = datagram.sequence;
                        if receive.peer.share.mouse && !receive.peer.share.paused && datagram.share_revision == receive.peer.share_revision {
                            receive.peer.cursor = Some(datagram.position);
                            update(state, |s| s.remote = receive.peer.clone());
                        }
                    }
                }
                _ = mouse_tick.tick() => {
                    let pending = state.motion.lock().unwrap_or_else(|p| p.into_inner()).take();
                    if let Some((revision, position)) = pending {
                        let current = *privacy.borrow();
                        if current.revision == revision && current.policy.mouse && !current.policy.paused {
                            mouse_sequence += 1;
                            let bytes = serde_json::to_vec(&crate::protocol::MouseDatagram { share_revision: current.revision, version: VERSION, session: invite.session, sequence: mouse_sequence, position }).map_err(|_| NetworkError::Protocol)?;
                            let _ = connection.send_datagram(bytes.into());
                        }
                    }
                }
                item = incoming_rx.recv() => {
                    let envelope = item.map_err(|_| NetworkError::Connection)??;
                    if rate_start.elapsed() >= Duration::from_secs(1) { rate = 0; rate_start = Instant::now(); }
                    rate += 1;
                    if rate > 240 { return Err(NetworkError::Protocol); }
                    if !receive.apply(&envelope)? { continue; }
                    last_received = Instant::now();
                    match envelope.message {
                        Message::Disconnect => return Ok(()),
                        Message::Probe => update(state, |s| s.probes_received += 1),
                        Message::PeerInfo(info) => update(state, |s| s.peer_info = Some(info)),
                        _ => {}
                    }
                    update(state, |s| s.remote = receive.peer.clone());
                }
                command = commands.recv() => {
                    let message = match command.map_err(|_| NetworkError::Stopped)? {
                        Command::Probe => Message::Probe,
                        #[cfg(test)]
                        Command::TransientDisconnect => return Err(NetworkError::Connection),
                        Command::Text { value, privacy_revision } => {
                            let current = *privacy.borrow();
                            if current.revision != privacy_revision || !current.policy.text || current.policy.paused { continue; }
                            local_text.clone_from(&value);
                            Message::TextUpdate(value)
                        }
                        Command::Input { event, privacy_revision } => {
                            let current = *privacy.borrow();
                            if current.revision != privacy_revision || !event.allowed(current.policy) { continue; }
                            match event {
                                SharedInput::Key { usage, down: true } => Message::KeyboardDown(usage),
                                SharedInput::Key { usage, down: false } => Message::KeyboardUp(usage),
                                SharedInput::MouseButton { button, down: true } => Message::MouseButtonDown(button),
                                SharedInput::MouseButton { button, down: false } => Message::MouseButtonUp(button),
                                SharedInput::Wheel(delta) => Message::MouseWheel(delta),
                                SharedInput::AbstractKeyboard(down) => Message::AbstractKeyboard(down),
                                SharedInput::Reset => Message::InputReset,
                            }
                        }
                        _ => { update(state, |s| s.error = Some(NetworkError::Busy)); continue; }
                    };
                    send_message(&mut send, invite, &mut sequence, message).await?;
                }
                _ = metrics.tick() => {
                    let info = state.info.lock().unwrap_or_else(|p| p.into_inner()).clone();
                    if sent_info.as_ref() != Some(&info) {
                        send_message(&mut send, invite, &mut sequence, Message::PeerInfo(info.clone())).await?;
                        sent_info = Some(info);
                    }
                    let (path, rtt_ms) = transport::path(connection);
                    update(state, |s| { s.path = path; s.rtt_ms = rtt_ms; });
                    if last_received.elapsed() > TIMEOUT { return Err(NetworkError::Timeout); }
                    if last_heartbeat.elapsed() >= Duration::from_secs(2) {
                        send_message(&mut send, invite, &mut sequence, Message::Heartbeat).await?;
                        last_heartbeat = Instant::now();
                    }
                }
            }
        }
    }.await;
    guard.connection = None;
    result
}

async fn send_message(
    send: &mut SendStream,
    invite: &Invite,
    sequence: &mut u64,
    message: Message,
) -> Result<(), NetworkError> {
    *sequence = sequence.checked_add(1).ok_or(NetworkError::Protocol)?;
    transport::write(
        send,
        &Envelope {
            version: VERSION,
            session: invite.session,
            sequence: *sequence,
            message,
        },
    )
    .await
}
