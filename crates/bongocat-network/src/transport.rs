use crate::{
    ConnectionPath, NetworkError,
    protocol::{Envelope, MAX_FRAME, VERSION},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use iroh::{
    Endpoint, EndpointAddr, RelayMode,
    endpoint::{Connection, RecvStream, SendStream, presets},
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) const ALPN: &[u8] = b"bongocat/together/1";
pub(crate) const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Invite {
    pub version: u16,
    pub address: EndpointAddr,
    pub session: [u8; 16],
    pub authorization: [u8; 32],
    pub expires_at: u64,
    pub direct_only: bool,
}

impl Invite {
    pub fn new(address: EndpointAddr, direct_only: bool) -> Result<Self, NetworkError> {
        let mut authorization = [0; 32];
        let mut session = [0; 16];
        getrandom::fill(&mut authorization).map_err(|_| NetworkError::Endpoint)?;
        getrandom::fill(&mut session).map_err(|_| NetworkError::Endpoint)?;
        Ok(Self {
            version: VERSION,
            address,
            session,
            authorization,
            expires_at: now_seconds() + 600,
            direct_only,
        })
    }

    pub fn encode(&self) -> Result<String, NetworkError> {
        let data = serde_json::to_vec(self).map_err(|_| NetworkError::Protocol)?;
        Ok(format!("bct1_{}", URL_SAFE_NO_PAD.encode(data)))
    }

    pub fn decode(value: &str) -> Result<Self, NetworkError> {
        let value = value.trim();
        if value.len() > MAX_FRAME || !value.starts_with("bct1_") {
            return Err(NetworkError::InvalidInvite);
        }
        let data = URL_SAFE_NO_PAD
            .decode(&value[5..])
            .map_err(|_| NetworkError::InvalidInvite)?;
        let invite: Self =
            serde_json::from_slice(&data).map_err(|_| NetworkError::InvalidInvite)?;
        if invite.version != VERSION
            || invite.expires_at <= now_seconds()
            || invite.expires_at > now_seconds() + 900
            || invite.authorization == [0; 32]
            || invite.session == [0; 16]
            || invite.address.addrs.len() > 16
            || invite
                .address
                .relay_urls()
                .any(|url| url.scheme() != "https")
        {
            return Err(NetworkError::InvalidInvite);
        }
        Ok(invite)
    }
}

pub(crate) fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) async fn bind(direct_only: bool) -> Result<Endpoint, NetworkError> {
    let builder = if direct_only {
        // Minimal avoids public DNS publication and disables every relay path.
        Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled)
    } else {
        Endpoint::builder(presets::N0)
    };
    let endpoint = tokio::time::timeout(TIMEOUT, builder.alpns(vec![ALPN.to_vec()]).bind())
        .await
        .map_err(|_| NetworkError::Timeout)?
        .map_err(|_| NetworkError::Endpoint)?;
    if !direct_only {
        // A public relay may be unavailable; retain direct address hints so LAN
        // connectivity can still work, and report actual connection failure later.
        let _ = tokio::time::timeout(Duration::from_secs(5), endpoint.online()).await;
    }
    Ok(endpoint)
}

pub(crate) async fn write(send: &mut SendStream, envelope: &Envelope) -> Result<(), NetworkError> {
    let bytes = serde_json::to_vec(envelope).map_err(|_| NetworkError::Protocol)?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(NetworkError::Protocol);
    }
    tokio::time::timeout(Duration::from_secs(3), async {
        send.write_all(&(bytes.len() as u32).to_be_bytes())
            .await
            .map_err(|_| NetworkError::Connection)?;
        send.write_all(&bytes)
            .await
            .map_err(|_| NetworkError::Connection)
    })
    .await
    .map_err(|_| NetworkError::Timeout)?
}

pub(crate) async fn read(recv: &mut RecvStream) -> Result<Envelope, NetworkError> {
    let mut prefix = [0; 4];
    recv.read_exact(&mut prefix)
        .await
        .map_err(|_| NetworkError::Connection)?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length == 0 || length > MAX_FRAME {
        return Err(NetworkError::Protocol);
    }
    let mut bytes = vec![0; length];
    recv.read_exact(&mut bytes)
        .await
        .map_err(|_| NetworkError::Connection)?;
    serde_json::from_slice(&bytes).map_err(|_| NetworkError::Protocol)
}

pub(crate) fn path(connection: &Connection) -> (ConnectionPath, Option<u64>) {
    let paths = connection.paths();
    match paths.iter().find(|path| path.is_selected()) {
        Some(path) => (
            if path.is_ip() {
                ConnectionPath::Direct
            } else if path.is_relay() {
                ConnectionPath::Relay
            } else {
                ConnectionPath::Unknown
            },
            Some(path.rtt().as_millis().min(u128::from(u64::MAX)) as u64),
        ),
        None => (ConnectionPath::Unknown, None),
    }
}

pub(crate) async fn close(endpoint: Endpoint) {
    // Endpoint close is graceful but bounded, including during service drop.
    let _ = tokio::time::timeout(Duration::from_secs(2), endpoint.close()).await;
}
