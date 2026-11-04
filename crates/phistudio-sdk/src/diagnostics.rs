//! Bounded metadata only: callback arguments, UI values and response bodies are
//! deliberately absent from execution diagnostics.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct ExecutionCost {
    pub fuel: Option<u64>,
    pub memory_bytes: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CallbackCost {
    pub callback: String,
    pub microseconds: u64,
    pub failed: bool,
    pub execution: ExecutionCost,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PluginDiagnostics {
    pub calls: u64,
    pub failures: u64,
    pub total_microseconds: u64,
    pub maximum_microseconds: u64,
    pub recent: Vec<CallbackCost>,
}
