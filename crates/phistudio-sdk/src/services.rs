use crate::{VersionReq, valid_id};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SERVICE_BEFORE_HOOK: &str = "host.service.before";

/// `host` addresses application services; other providers are plugin IDs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceRef {
    pub provider: String,
    pub id: String,
    pub version: VersionReq,
}
impl ServiceRef {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.provider) || !valid_id(&self.id) {
            return Err("Invalid service provider or ID".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceCall {
    /// Correlation ID chosen by the caller. Replies are delivered only to it.
    pub request_id: String,
    pub service: ServiceRef,
    pub method: String,
    pub arguments: Value,
}
impl ServiceCall {
    pub fn validate(&self) -> Result<(), String> {
        self.service.validate()?;
        if !valid_id(&self.request_id) || !valid_id(&self.method) {
            return Err("Invalid service request ID or method".into());
        }
        Ok(())
    }
}

/// Payload for host.service.before. Caller identity and correlation are immutable.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceIntercept {
    pub consumer: String,
    pub call: ServiceCall,
}
impl ServiceIntercept {
    pub fn validate_replacement(&self, original: &Self) -> Result<(), String> {
        self.call.validate()?;
        if self.consumer != original.consumer || self.call.request_id != original.call.request_id {
            return Err("Service interception cannot change the caller or correlation ID".into());
        }
        Ok(())
    }
}

/// Expected errors are values, not failures of the calling plugin instance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServiceOutcome {
    Success { value: Value },
    Error { code: String, message: String },
}
impl ServiceOutcome {
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceReply {
    pub request_id: String,
    pub service: ServiceRef,
    pub outcome: ServiceOutcome,
}
