//! Platform-neutral extension protocol. No editor pointers or Rust ABI cross it.
mod assets;
mod audio;
mod context;
mod diagnostics;
mod document;
mod editor_actions;
mod exports;
mod features;
mod file_hooks;
mod files;
#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub mod guest;
mod input;
mod inspection;
mod manifest;
pub mod native;
mod network;
mod permissions;
mod plugin_data;
mod preview_effects;
mod protocol;
mod render;
mod services;
mod transaction;
mod ui;
mod websocket;
pub use assets::*;
pub use audio::*;
pub use context::*;
pub use diagnostics::*;
pub use document::*;
pub use editor_actions::*;
pub use features::*;
pub use file_hooks::*;
pub use files::*;
pub use input::*;
pub use inspection::*;
pub use manifest::*;
pub use network::*;
pub use permissions::*;
pub use plugin_data::*;
pub use preview_effects::*;
pub use protocol::*;
pub use render::*;
pub use semver::{Version, VersionReq};
pub use services::*;
pub use transaction::*;
pub use ui::*;
pub use websocket::*;

pub const API_VERSION: u32 = 1;
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PATCH_OPERATIONS: usize = 40_000;

/// Statically linked implementations use precisely the external wire contract.
/// Instances own their state; the host owns every registered contribution.
pub trait Plugin: Send {
    fn call(&mut self, request: &Request) -> Result<Response, String>;
    fn execution_cost(&self) -> ExecutionCost {
        ExecutionCost::default()
    }
}
