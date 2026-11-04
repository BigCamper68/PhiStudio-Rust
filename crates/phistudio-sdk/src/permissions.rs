//! Requested access is distinct from a user's persisted permission decision.
use crate::{Backend, Capability, Manifest};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkPermissions {
    #[serde(default)]
    pub origins: BTreeSet<String>,
    #[serde(default)]
    pub local_network: bool,
}
impl NetworkPermissions {
    pub fn is_empty(&self) -> bool {
        self.origins.is_empty() && !self.local_network
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.origins.len() > 64 {
            return Err("A plugin can request at most 64 network origins".into());
        }
        for origin in &self.origins {
            let url = url::Url::parse(origin).map_err(|_| "Invalid network origin")?;
            if origin.len() > 512
                || !matches!(url.scheme(), "https" | "http" | "wss" | "ws")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || url.origin().ascii_serialization() != *origin
                || (matches!(url.scheme(), "http" | "ws") && !self.local_network)
            {
                return Err("Network origins must be canonical scheme://host[:port] addresses; cleartext requires local-network permission".into());
            }
        }
        Ok(())
    }
    pub fn is_subset(&self, other: &Self) -> bool {
        self.origins.is_subset(&other.origins) && (!self.local_network || other.local_network)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionRequest {
    pub capabilities: BTreeSet<Capability>,
    #[serde(default)]
    pub network: NetworkPermissions,
    #[serde(default)]
    pub native: bool,
}
impl PermissionRequest {
    pub fn from_manifest(manifest: &Manifest) -> Self {
        Self {
            capabilities: manifest.capabilities.clone(),
            network: manifest.network.clone(),
            native: matches!(manifest.backend, Backend::Native { .. }),
        }
    }
    pub fn is_subset(&self, other: &Self) -> bool {
        self.capabilities.is_subset(&other.capabilities)
            && self.network.is_subset(&other.network)
            && (!self.native || other.native)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionDecision {
    /// Remember what was shown, including permissions the user declined.
    pub requested: PermissionRequest,
    pub granted: PermissionRequest,
}
impl PermissionDecision {
    pub fn validate(&self, manifest: &Manifest) -> Result<(), String> {
        let requested = PermissionRequest::from_manifest(manifest);
        self.requested.network.validate()?;
        self.granted.network.validate()?;
        if !requested.is_subset(&self.requested)
            || !self.granted.is_subset(&self.requested)
            || (self.granted.native && !self.requested.native)
            || (!self.granted.capabilities.contains(&Capability::Network)
                && !self.granted.network.is_empty())
        {
            return Err("Plugin permissions changed; review access before enabling it".into());
        }
        Ok(())
    }
    pub fn effective(&self, manifest: &Manifest) -> BTreeSet<Capability> {
        self.granted
            .capabilities
            .intersection(&manifest.capabilities)
            .cloned()
            .collect()
    }
    pub fn effective_network(&self, manifest: &Manifest) -> NetworkPermissions {
        if !self.effective(manifest).contains(&Capability::Network) {
            return NetworkPermissions::default();
        }
        NetworkPermissions {
            origins: self
                .granted
                .network
                .origins
                .intersection(&manifest.network.origins)
                .cloned()
                .collect(),
            local_network: self.granted.network.local_network && manifest.network.local_network,
        }
    }
}
