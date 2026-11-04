use crate::{
    Capability, ContextTarget, DocumentVersion, DrawLayer, FormatBridge, FormatOperation,
    InputBatch, InputButton, InputSurfaceKind, Manifest, ProviderFiles, RenderFrame, ResourceKind,
    ServiceCall, ServiceReply, ToolGesture, Transaction, UiEvent, Widget,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EditorContext {
    pub version: DocumentVersion,
    /// Present only when DocumentRead was granted and the caller requested it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart: Option<Value>,
    #[serde(default)]
    pub selection: Value,
    pub line: usize,
    pub beat: f64,
    pub playing: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Activate {
        manifest: Manifest,
        settings: BTreeMap<String, Value>,
        #[serde(default)]
        storage: Value,
    },
    Deactivate,
    Command {
        id: String,
        arguments: Value,
        context: EditorContext,
    },
    Event {
        topic: String,
        payload: Value,
        context: EditorContext,
    },
    Hook {
        point: String,
        payload: Value,
        context: EditorContext,
    },
    Ui {
        event: UiEvent,
        context: EditorContext,
    },
    Service {
        id: String,
        method: String,
        arguments: Value,
        context: EditorContext,
    },
    ServiceResult {
        reply: ServiceReply,
        context: EditorContext,
    },
    Render {
        frame: RenderFrame,
        context: EditorContext,
    },
    Input {
        tool: String,
        input: InputBatch,
        context: EditorContext,
    },
    Format {
        id: String,
        operation: FormatOperation,
        #[serde(default, skip_serializing_if = "FormatBridge::is_chart")]
        bridge: FormatBridge,
        files: ProviderFiles,
        context: EditorContext,
    },
    Resource {
        id: String,
        resource: ResourceKind,
        files: ProviderFiles,
        context: EditorContext,
    },
    SettingsChanged {
        key: String,
        value: Value,
    },
}

impl Request {
    pub fn context(&self) -> Option<&EditorContext> {
        match self {
            Self::Command { context, .. }
            | Self::Event { context, .. }
            | Self::Hook { context, .. }
            | Self::Ui { context, .. }
            | Self::Service { context, .. }
            | Self::ServiceResult { context, .. }
            | Self::Render { context, .. }
            | Self::Input { context, .. }
            | Self::Format { context, .. }
            | Self::Resource { context, .. } => Some(context),
            _ => None,
        }
    }
    pub fn context_mut(&mut self) -> Option<&mut EditorContext> {
        match self {
            Self::Command { context, .. }
            | Self::Event { context, .. }
            | Self::Hook { context, .. }
            | Self::Ui { context, .. }
            | Self::Service { context, .. }
            | Self::ServiceResult { context, .. } => Some(context),
            Self::Render { context, .. }
            | Self::Input { context, .. }
            | Self::Format { context, .. }
            | Self::Resource { context, .. } => Some(context),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Contribution {
    /// An explicit editor menu entry that opens this plugin's window panel.
    Menu {
        id: String,
        title: String,
        location: EditorMenu,
        panel: String,
    },
    Command {
        id: String,
        title: String,
        #[serde(default)]
        shortcut: Option<String>,
        /// Opt into the legacy whole-chart context; queries are preferable.
        #[serde(default)]
        snapshot: bool,
    },
    Panel {
        id: String,
        title: String,
        location: PanelLocation,
        #[serde(default)]
        snapshot: bool,
    },
    Tool {
        id: String,
        title: String,
        #[serde(default = "crate::default_input_surfaces")]
        surfaces: Vec<InputSurfaceKind>,
        #[serde(default = "crate::default_input_buttons")]
        buttons: Vec<InputButton>,
        #[serde(default)]
        gesture: ToolGesture,
    },
    Track {
        id: String,
        title: String,
    },
    /// A column inside the editor timeline, opened from Windows.
    TimelineTrack {
        id: String,
        title: String,
        width: f32,
    },
    /// A contextual entry invoking an owned command with ContextArguments.
    ContextMenu {
        id: String,
        title: String,
        target: ContextTarget,
        command: String,
    },
    Preview {
        id: String,
    },
    PreviewEffect {
        id: String,
        #[serde(default)]
        priority: i32,
    },
    Format {
        id: String,
        title: String,
        extensions: Vec<String>,
        #[serde(default, skip_serializing_if = "FormatBridge::is_chart")]
        bridge: FormatBridge,
        #[serde(default = "default_true")]
        decode: bool,
        #[serde(default)]
        encode: bool,
    },
    Resource {
        id: String,
        extensions: Vec<String>,
        #[serde(default)]
        title: String,
        #[serde(default)]
        resource: ResourceKind,
    },
    Hook {
        point: String,
        #[serde(default)]
        priority: i32,
    },
    Event {
        topic: String,
    },
    Service {
        id: String,
        version: crate::Version,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelLocation {
    Window,
    Sidebar,
    Inspector,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorMenu {
    File,
    Windows,
}

impl Contribution {
    pub fn id(&self) -> &str {
        match self {
            Self::Menu { id, .. }
            | Self::Command { id, .. }
            | Self::Panel { id, .. }
            | Self::Tool { id, .. }
            | Self::Track { id, .. }
            | Self::TimelineTrack { id, .. }
            | Self::ContextMenu { id, .. }
            | Self::Preview { id }
            | Self::PreviewEffect { id, .. }
            | Self::Format { id, .. }
            | Self::Resource { id, .. }
            | Self::Service { id, .. } => id,
            Self::Hook { point, .. } => point,
            Self::Event { topic } => topic,
        }
    }
    pub fn capability(&self) -> Option<Capability> {
        Some(match self {
            Self::Command { .. } | Self::Event { .. } => return None,
            Self::Panel { .. } | Self::Menu { .. } | Self::ContextMenu { .. } => Capability::Ui,
            Self::Tool { .. } => Capability::Input,
            Self::Track { .. } | Self::TimelineTrack { .. } => Capability::Timeline,
            Self::Preview { .. } | Self::PreviewEffect { .. } => Capability::Preview,
            Self::Format { .. } => Capability::Formats,
            Self::Resource { .. } => Capability::Resources,
            Self::Hook { .. } => Capability::Hooks,
            Self::Service { .. } => Capability::Services,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    Complete {
        operation: u64,
        file: u64,
        name: String,
    },
    Fail {
        operation: u64,
        message: String,
    },
    Edit {
        transaction: Transaction,
    },
    Notify {
        message: String,
    },
    Panel {
        id: String,
        widgets: Vec<Widget>,
    },
    /// Display an owned window panel as a modal dialog after an explicit action.
    Dialog {
        panel: String,
        open: bool,
        version: DocumentVersion,
    },
    Draw {
        frame: u64,
        layers: Vec<DrawLayer>,
    },
    PreviewEffects {
        frame: u64,
        passes: Vec<crate::PreviewPass>,
    },
    Emit {
        topic: String,
        payload: Value,
    },
    Store {
        value: Value,
    },
    /// Answered by a targeted ServiceResult callback, without a subscription.
    Call {
        call: ServiceCall,
    },
    SelectTool {
        id: Option<String>,
    },
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HookDecision {
    Continue,
    Cancel { reason: String },
    Replace { payload: Value },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    #[serde(default)]
    pub contributions: Vec<Contribution>,
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default)]
    pub hook: Option<HookDecision>,
    #[serde(default)]
    pub result: Value,
}
