use crate::{NetworkCredential, NetworkRequest, valid_id};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_WEBSOCKET_MESSAGE_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebSocketOpen {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub credential: Option<NetworkCredential>,
    #[serde(default)]
    pub protocols: Vec<String>,
}
impl WebSocketOpen {
    pub fn validate(&self) -> Result<(), String> {
        if self.url.len() > 8192 || self.url.chars().any(char::is_control) {
            return Err("Invalid WebSocket URL".into());
        }
        let mut url = url::Url::parse(&self.url).map_err(|_| "Invalid WebSocket URL")?;
        let scheme = match url.scheme() {
            "wss" => "https",
            "ws" => "http",
            _ => return Err("WebSockets require wss or explicitly approved local ws".into()),
        };
        url.set_scheme(scheme)
            .map_err(|_| "Invalid WebSocket URL")?;
        let request = NetworkRequest {
            id: self.id.clone(),
            url: url.to_string(),
            headers: self.headers.clone(),
            credential: self.credential.clone(),
            ..NetworkRequest::get(&self.id, url.as_str())
        };
        request.validate()?;
        let mut protocols = std::collections::BTreeSet::new();
        if self.protocols.len() > 8
            || self.protocols.iter().any(|p| {
                p.is_empty()
                    || p.len() > 64
                    || !p
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
                    || !protocols.insert(p)
            })
        {
            return Err("Invalid WebSocket subprotocols".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WebSocketMessage {
    Text { text: String },
    Binary { base64: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebSocketSend {
    pub id: String,
    pub message: WebSocketMessage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebSocketReceive {
    pub id: String,
    pub timeout_ms: u64,
}
impl WebSocketReceive {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.id) || self.timeout_ms > 30_000 {
            return Err("Receive requires a valid socket ID and timeout up to 30 seconds".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebSocketState {
    Connecting,
    Open,
    Closed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WebSocketStatus {
    pub id: String,
    pub state: WebSocketState,
    pub received: u64,
    pub sent: u64,
    pub queued: usize,
    pub error: Option<String>,
}

/// A null message is a timeout or a closed, drained socket; inspect status.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WebSocketReceived {
    pub message: Option<WebSocketMessage>,
    pub status: WebSocketStatus,
}
