//! Host-owned HTTP operations. IDs are private to one plugin activation.
use crate::{MAX_FILE_CHUNK_BYTES, valid_id};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_NETWORK_BODY_BYTES: usize = 1024 * 1024;
pub const MAX_NETWORK_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_NETWORK_INLINE_BYTES: usize = 256 * 1024;

fn get() -> String {
    "GET".into()
}
fn timeout() -> u64 {
    30_000
}
fn max_bytes() -> u64 {
    8 * 1024 * 1024
}
fn redirects() -> u8 {
    3
}
fn authorization() -> String {
    "authorization".into()
}
fn bearer() -> String {
    "Bearer ".into()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkRequest {
    pub id: String,
    pub url: String,
    #[serde(default = "get")]
    pub method: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body_base64: Option<String>,
    #[serde(default)]
    pub credential: Option<NetworkCredential>,
    #[serde(default = "timeout")]
    pub timeout_ms: u64,
    #[serde(default = "max_bytes")]
    pub max_bytes: u64,
    #[serde(default = "redirects")]
    pub redirects: u8,
}
impl NetworkRequest {
    pub fn get(id: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            url: url.into(),
            method: get(),
            headers: BTreeMap::new(),
            body_base64: None,
            credential: None,
            timeout_ms: timeout(),
            max_bytes: max_bytes(),
            redirects: redirects(),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.id)
            || self.url.chars().any(char::is_control)
            || self.url.len() > 8192
            || !matches!(
                self.method.as_str(),
                "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS"
            )
            || !(100..=120_000).contains(&self.timeout_ms)
            || self.max_bytes == 0
            || self.max_bytes > MAX_NETWORK_RESPONSE_BYTES
            || self.redirects > 5
            || self.headers.len() > 32
            || self
                .headers
                .iter()
                .any(|(k, v)| k.len() > 128 || v.len() > 8192)
            || self
                .headers
                .iter()
                .map(|(k, v)| k.len() + v.len())
                .sum::<usize>()
                > 32 * 1024
        {
            return Err("Network request exceeds its supported limits".into());
        }
        let url = url::Url::parse(&self.url).map_err(|_| "Invalid request URL")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err("Requests require an HTTP(S) URL without credentials or a fragment".into());
        }
        if let Some(value) = &self.body_base64
            && (value.len() > MAX_NETWORK_BODY_BYTES.div_ceil(3) * 4
                || STANDARD
                    .decode(value)
                    .map_err(|_| "Invalid request body encoding")?
                    .len()
                    > MAX_NETWORK_BODY_BYTES)
        {
            return Err("Network request body exceeds 1 MiB".into());
        }
        if let Some(credential) = &self.credential {
            credential.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkCredential {
    pub key: String,
    #[serde(default = "authorization")]
    pub header: String,
    #[serde(default = "bearer")]
    pub prefix: String,
}
impl NetworkCredential {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.key)
            || self.header.len() > 128
            || self.header.is_empty()
            || !self
                .header
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || self.prefix.len() > 64
            || !self.prefix.bytes().all(|b| (32..=126).contains(&b))
        {
            return Err("Invalid credential reference".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkId {
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkRead {
    pub id: String,
    pub offset: u64,
    pub limit: usize,
}
impl NetworkRead {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.id) || self.limit == 0 || self.limit > MAX_FILE_CHUNK_BYTES {
            return Err("Network reads require 1–256 KiB and a valid request ID".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkChunk {
    pub base64: String,
    pub next_offset: u64,
    pub eof: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkState {
    Running,
    Ready,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkProgress {
    pub id: String,
    pub state: NetworkState,
    pub received: u64,
    pub total: Option<u64>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkResponse {
    pub id: String,
    pub status: u16,
    pub headers: BTreeMap<String, Vec<String>>,
    pub bytes: u64,
    /// Present for responses up to 256 KiB; larger bodies use network.read.
    pub body_base64: Option<String>,
}
impl NetworkResponse {
    pub fn inline_bytes(&self) -> Result<Option<Vec<u8>>, String> {
        self.body_base64
            .as_ref()
            .map(|s| {
                STANDARD
                    .decode(s)
                    .map_err(|_| "Invalid response encoding".into())
            })
            .transpose()
    }
}

/// Secret values deliberately have no Debug representation containing the value.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretWrite {
    pub key: String,
    pub value: String,
    pub origins: BTreeSet<String>,
}
impl std::fmt::Debug for SecretWrite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretWrite")
            .field("key", &self.key)
            .field("value", &"[redacted]")
            .field("origins", &self.origins)
            .finish()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretKey {
    pub key: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretInfo {
    pub key: String,
    pub exists: bool,
    pub origins: BTreeSet<String>,
}
