//! Feature discovery is metadata-only and does not require service permission.
use crate::{Capability, Version};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HostFeatures {
    pub api: u32,
    pub editor_version: Version,
    pub platform: String,
    pub granted: BTreeSet<Capability>,
    /// Only services implemented by this host/platform are advertised.
    pub services: BTreeMap<String, Version>,
    pub widgets: BTreeSet<String>,
    #[serde(default)]
    pub extensions: BTreeSet<String>,
    pub limits: BTreeMap<String, u64>,
}
