//! Native action continuations. Built-in tools and their history stay in place.
use super::*;
use crate::{advanced_ui::RightMode, toolkit::Scope, workbench::Tool};
use phistudio_core::{NoteKind, objects::ObjectClipboard, story_tools::StoryId, tools::Selection};
use phistudio_plugins::{Invocation, Output};
use phistudio_sdk::*;
use std::sync::Arc;

enum Source {
    User,
    Service(Box<HostCall>, Option<EditorReveal>),
}
struct Snapshot {
    selection: Selection,
    story: Vec<StoryId>,
    clipboard: Option<Arc<ObjectClipboard>>,
    layer: usize,
    tool: Tool,
    scope: Scope,
    division: i32,
    lanes: usize,
    xy_link: bool,
    right_mode: RightMode,
    multi_line: bool,
    lines: Vec<usize>,
    assets: u64,
}
impl Snapshot {
    fn capture(app: &Workbench) -> Self {
        Self {
            selection: app.selection(),
            story: app.story_selection(),
            clipboard: app.tools.clipboard.clone(),
            layer: app.events.layer,
            tool: app.timeline.tool,
            scope: app.tools.scope,
            division: app.timeline.division,
            lanes: app.timeline.lanes,
            xy_link: app.options.xy_link,
            right_mode: app.advanced.right_mode,
            multi_line: app.multi_line.enabled,
            lines: app.multi_line.lines.clone(),
            assets: app.asset_revision,
        }
    }
    fn current(&self, app: &Workbench) -> bool {
        self.selection == app.selection()
            && self.story == app.story_selection()
            && match (&self.clipboard, &app.tools.clipboard) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && self.layer == app.events.layer
            && self.tool == app.timeline.tool
            && self.scope == app.tools.scope
            && self.division == app.timeline.division
            && self.lanes == app.timeline.lanes
            && self.xy_link == app.options.xy_link
            && self.right_mode == app.advanced.right_mode
            && self.multi_line == app.multi_line.enabled
            && self.lines == app.multi_line.lines
            && self.assets == app.asset_revision
    }
}
struct Pending {
    job: u64,
    invocation: Invocation,
    payload: ActionIntercept,
    snapshot: Snapshot,
    expected: Vec<Origin>,
    source: Source,
    flush_draft: bool,
}
impl Pending {
    fn context_current(&self, app: &Workbench) -> bool {
        !app.modal_open_except_editor_action()
            && self.payload.version == app.plugin_version()
            && self.payload.line == app.line
            && self.payload.beat == app.transport.beat
            && self.payload.playing == app.transport.playing
            && self.snapshot.current(app)
    }
    fn current(&self, app: &Workbench) -> bool {
        self.context_current(app)
            && app.plugins.runtime.as_ref().is_some_and(|runtime| {
                runtime.is_invocation_current(&self.invocation, &self.expected)
            })
    }
}
#[derive(Default)]
pub(super) struct EditorActions {
    pending: Option<Pending>,
    pub(super) jobs: BTreeSet<u64>,
    applying: bool,
}

impl Workbench {
    pub(crate) fn plugin_editor_action_modal(&self) -> bool {
        self.plugins.editor_actions.pending.is_some()
    }
    pub(crate) fn plugin_editor_action_holds_interaction(
        &self,
        interaction: &phistudio_plugins::Interaction,
    ) -> bool {
        self.plugins.editor_actions.pending.as_ref().is_some_and(|p| {
            matches!(&p.source, Source::Service(call, _) if call.interaction() == Some(interaction))
        })
    }
    pub(crate) fn plugin_editor_action_preflight(&mut self) {
        if self
            .plugins
            .editor_actions
            .pending
            .as_ref()
            .is_some_and(|p| {
                !p.current(self)
                    && !(p.context_current(self) && self.editor_action_hook_failed(&p.expected))
            })
        {
            self.cancel_plugin_editor_action();
        }
    }
    fn editor_action_hook_failed(&self, expected: &[Origin]) -> bool {
        self.plugins
            .runtime
            .as_ref()
            .and_then(|r| r.registry().ok())
            .is_some_and(|registry| {
                registry.plugins.iter().any(|info| {
                    expected
                        .iter()
                        .any(|origin| origin.plugin == info.manifest.id)
                        && matches!(info.status, phistudio_plugins::Status::Failed(_))
                })
            })
    }
    pub(crate) fn cancel_plugin_editor_action(&mut self) -> bool {
        let Some(pending) = self.plugins.editor_actions.pending.take() else {
            return false;
        };
        pending.invocation.cancel();
        self.reject_editor_action(pending.source, "Plugin operation canceled", true);
        true
    }
    fn selection_counts(&self) -> EditorSelectionCounts {
        let selection = &self.tools.selection;
        let fallback = selection.is_empty();
        EditorSelectionCounts {
            notes: selection.notes.len() + usize::from(fallback && self.selected.is_some()),
            events: selection.events.len()
                + usize::from(fallback && self.events.selected.is_some()),
            story: if self.advanced.story_selection.is_empty() {
                usize::from(self.advanced.story_selected.is_some())
            } else {
                self.advanced.story_selection.len()
            },
        }
    }
    fn editor_status(&self) -> EditorStatus {
        EditorStatus {
            version: self.plugin_version(),
            line: self.line,
            beat: self.transport.beat,
            playing: self.transport.playing,
            can_undo: self.editor.can_undo(),
            can_redo: self.editor.can_redo(),
            selection: self.selection_counts(),
            clipboard_available: self.tools.clipboard.is_some(),
            tool: self.timeline.tool.into(),
        }
    }
    pub(crate) fn request_editor_action(&mut self, action: EditorAction) {
        self.begin_editor_action(action, Source::User, false);
    }
    pub(crate) fn request_editor_playback(&mut self, playing: bool, flush_draft: bool) {
        self.begin_editor_action(
            EditorAction::SetPlayback { playing },
            Source::User,
            flush_draft,
        );
    }
    fn reject_editor_action(&mut self, source: Source, message: &str, canceled: bool) {
        match source {
            Source::User => {
                if canceled {
                    self.status = message.into()
                } else {
                    self.error = Some(message.into())
                }
            }
            Source::Service(call, _) => {
                let call = *call;
                if canceled && let Some(interaction) = call.interaction() {
                    interaction.cancel();
                }
                let call = if canceled {
                    call.after_editor_action().readonly_reply()
                } else {
                    call
                };
                self.reply_plugin_service(
                    call,
                    ServiceOutcome::error(
                        if canceled {
                            "action_canceled"
                        } else {
                            "action_rejected"
                        },
                        message,
                    ),
                );
            }
        }
    }
    fn begin_editor_action(&mut self, action: EditorAction, source: Source, flush_draft: bool) {
        if self.plugins.editor_actions.applying {
            self.run_editor_action_now(action, source, flush_draft);
            return;
        }
        if let Err(error) = action.validate() {
            self.reject_editor_action(source, &error, false);
            return;
        }
        if self.plugin_editor_action_modal() {
            self.reject_editor_action(
                source,
                "Another editor action is waiting for a plugin",
                false,
            );
            return;
        }
        if matches!(source, Source::Service(..)) && (self.modal_open() || self.show_projects) {
            self.reject_editor_action(source, "Editor is busy with another workflow", false);
            return;
        }
        let expected: Vec<_> = self.plugins.registry.plugins.iter().filter_map(|info| {
            info.origin.as_ref().filter(|origin| {
                self.plugins.runtime.as_ref().is_some_and(|r| r.is_current(origin))
                    && info.contributions.iter().any(|c|
                        matches!(c, Contribution::Hook { point, .. } if point == ACTION_BEFORE_HOOK))
            }).cloned()
        }).collect();
        if expected.is_empty() {
            self.run_editor_action_now(action, source, flush_draft);
            return;
        }
        let payload = ActionIntercept {
            action,
            version: self.plugin_version(),
            source: match &source {
                Source::User => None,
                Source::Service(c, _) => Some(c.origin.plugin.clone()),
            },
            line: self.line,
            beat: self.transport.beat,
            playing: self.transport.playing,
            selection: self.selection_counts(),
        };
        let Some(runtime) = &self.plugins.runtime else {
            return;
        };
        let invocation = runtime.invocation(payload.version);
        let snapshot = Snapshot::capture(self);
        let context = self.plugin_metadata_context();
        match self.plugins.submit(
            Action::ScopedHook {
                invocation: invocation.clone(),
                point: ACTION_BEFORE_HOOK.into(),
                payload: json!(payload),
                context,
            },
            Some(payload.version),
        ) {
            Ok(job) => {
                self.plugins.editor_actions.jobs.insert(job);
                self.plugins.editor_actions.pending = Some(Pending {
                    job,
                    invocation,
                    payload,
                    snapshot,
                    expected,
                    source,
                    flush_draft,
                });
            }
            Err(error) => {
                invocation.cancel();
                self.reject_editor_action(source, &error, false);
            }
        }
    }
    pub(super) fn complete_plugin_editor_action(
        &mut self,
        job: u64,
        result: Result<Output, String>,
    ) {
        if self
            .plugins
            .editor_actions
            .pending
            .as_ref()
            .is_none_or(|p| p.job != job)
        {
            return;
        }
        let pending = self.plugins.editor_actions.pending.take().unwrap();
        if !pending.current(self) {
            pending.invocation.cancel();
            let failed = self.editor_action_hook_failed(&pending.expected);
            if let Err(error) = result
                && failed
            {
                self.reject_editor_action(pending.source, &error, false);
            } else {
                self.reject_editor_action(pending.source, "Editor action context changed", true);
            }
            return;
        }
        let output = match result {
            Ok(output) => output,
            Err(error) => {
                pending.invocation.cancel();
                self.reject_editor_action(pending.source, &error, false);
                return;
            }
        };
        if self
            .plugins
            .runtime
            .as_ref()
            .is_none_or(|r| !r.is_output_current(&output))
        {
            pending.invocation.cancel();
            self.reject_editor_action(pending.source, "Editor action hook was revoked", true);
            return;
        }
        if let Some(reason) = output.cancelled {
            for effect in output.effects {
                self.apply_plugin_effect(effect);
            }
            pending.invocation.cancel();
            self.reject_editor_action(
                pending.source,
                if reason.is_empty() {
                    "Plugin operation canceled"
                } else {
                    &reason
                },
                true,
            );
            return;
        }
        let payload = serde_json::from_value::<ActionIntercept>(output.result)
            .map_err(|e| e.to_string())
            .and_then(|value| {
                value.validate_replacement(&pending.payload)?;
                Ok(value)
            });
        let payload = match payload {
            Ok(payload) => payload,
            Err(error) => {
                pending.invocation.cancel();
                self.reject_editor_action(pending.source, &error, false);
                return;
            }
        };
        self.plugins.calls.extend(output.calls);
        for effect in output.effects {
            self.apply_plugin_effect(effect);
        }
        if !pending.current(self)
            || self
                .plugins
                .runtime
                .as_ref()
                .is_none_or(|r| !r.is_invocation_current(&pending.invocation, &output.required))
        {
            pending.invocation.cancel();
            self.reject_editor_action(pending.source, "Editor action hook was revoked", true);
            return;
        }
        self.run_editor_action_now(payload.action, pending.source, pending.flush_draft);
    }
    fn run_editor_action_now(&mut self, action: EditorAction, source: Source, flush_draft: bool) {
        if let Source::Service(call, reveal) = &source {
            let authorized = self
                .plugins
                .runtime
                .as_ref()
                .ok_or_else(|| "Plugin host stopped".into())
                .and_then(|r| {
                    if reveal.is_some() {
                        r.authorize_inspection(
                            call,
                            self.plugin_version(),
                            "editor",
                            Capability::DocumentRead,
                        )?;
                    }
                    r.authorize_editor_service(call, self.plugin_version(), Some(&action), true)
                });
            if let Err(error) = authorized {
                self.reject_editor_action(source, &error, false);
                return;
            }
        }
        let version = self.plugin_version();
        let previous_error = if matches!(source, Source::Service(..)) {
            self.error.take()
        } else {
            None
        };
        let applying = self.plugins.editor_actions.applying;
        self.plugins.editor_actions.applying = true;
        match action {
            EditorAction::Undo => self.history_step_now(false),
            EditorAction::Redo => self.history_step_now(true),
            EditorAction::Copy => self.copy_selection_now(false),
            EditorAction::Cut => self.copy_selection_now(true),
            EditorAction::Paste {
                mirrored,
                original_time,
            } => self.paste_selection_now(mirrored, original_time),
            EditorAction::Duplicate => self.duplicate_selection_now(),
            EditorAction::DeleteSelection => self.delete_selected_now(),
            EditorAction::ClearSelection => self.select(None),
            EditorAction::SelectScope { scope, invert } => {
                self.select_scope_now(scope.into(), invert)
            }
            EditorAction::SetTool { tool } => self.set_tool_now(tool.into()),
            EditorAction::SetPlayback { playing } => {
                if !flush_draft || self.flush_shader_draft() {
                    self.transport.playing = playing;
                }
            }
            EditorAction::Seek { beat } => self.transport.beat = beat,
        }
        self.plugins.editor_actions.applying = applying;
        if let Source::Service(call, reveal) = source {
            if let Some(reveal) = reveal {
                self.reveal_plugin_location(&reveal);
            }
            let call = *call;
            let outcome = match self.error.take() {
                Some(error) => ServiceOutcome::error("action_failed", error),
                None => ServiceOutcome::Success {
                    value: json!({
                        "document_changed": version != self.plugin_version(), "status": self.editor_status(),
                    }),
                },
            };
            self.error = previous_error;
            self.reply_plugin_service(call.after_editor_action(), outcome);
        }
    }
    pub(super) fn plugin_editor_service(&mut self, call: HostCall) {
        if call.request.method == "reveal" {
            let parsed = serde_json::from_value::<EditorReveal>(call.request.arguments.clone())
                .map_err(|e| e.to_string())
                .and_then(|request| {
                    if request.version != self.plugin_version() {
                        return Err("Reveal document changed".into());
                    }
                    self.editor
                        .document()
                        .source_location(&request.path)
                        .map_err(|e| e.to_string())?;
                    let beat = request.beat.unwrap_or(self.transport.beat).max(0.);
                    let action = EditorAction::Seek { beat };
                    let runtime = self.plugins.runtime.as_ref().ok_or("Plugin host stopped")?;
                    runtime.authorize_inspection(
                        &call,
                        request.version,
                        "editor",
                        Capability::DocumentRead,
                    )?;
                    runtime.authorize_editor_service(
                        &call,
                        request.version,
                        Some(&action),
                        false,
                    )?;
                    Ok((request, action))
                });
            match parsed {
                Ok((request, action)) => self.begin_editor_action(
                    action,
                    Source::Service(Box::new(call), Some(request)),
                    false,
                ),
                Err(error) => {
                    self.reply_plugin_service(call, ServiceOutcome::error("reveal_rejected", error))
                }
            }
            return;
        }
        let parsed = match call.request.method.as_str() {
            "status" => serde_json::from_value::<EditorStatusQuery>(call.request.arguments.clone())
                .map(|query| (query.version, None)),
            "action" => {
                serde_json::from_value::<EditorActionRequest>(call.request.arguments.clone())
                    .map(|query| (query.version, Some(query.action)))
            }
            _ => {
                self.reply_plugin_service(
                    call,
                    ServiceOutcome::error("method_unavailable", "Unknown editor method"),
                );
                return;
            }
        };
        let (version, action) = match parsed {
            Ok(query) => query,
            Err(error) => {
                self.reply_plugin_service(
                    call,
                    ServiceOutcome::error("invalid_arguments", error.to_string()),
                );
                return;
            }
        };
        let authorized = if version != self.plugin_version() {
            Err("Editor service document version changed".into())
        } else {
            self.plugins
                .runtime
                .as_ref()
                .ok_or_else(|| "Plugin host stopped".into())
                .and_then(|r| r.authorize_editor_service(&call, version, action.as_ref(), false))
        };
        if let Err(error) = authorized {
            self.reply_plugin_service(call, ServiceOutcome::error("action_rejected", error));
        } else if let Some(action) = action {
            self.begin_editor_action(action, Source::Service(Box::new(call), None), false);
        } else {
            let outcome = ServiceOutcome::Success {
                value: json!(self.editor_status()),
            };
            self.reply_plugin_service(call, outcome);
        }
    }
    fn reveal_plugin_location(&mut self, reveal: &EditorReveal) {
        use phistudio_core::SourceLocation;
        let Ok(location) = self.editor.document().source_location(&reveal.path) else {
            return;
        };
        self.close_fullscreen_preview();
        self.transport.playing = false;
        self.windows.sidebar = true;
        match location {
            SourceLocation::Note(id) => {
                self.advanced.right_mode = RightMode::Events;
                if let Ok(note) = self.editor.document().note(id.line, id.index) {
                    self.timeline.camera_x = if note.x.abs() <= 675. { 0. } else { note.x };
                }
                self.select(Some(id));
            }
            SourceLocation::Event(id) => {
                self.advanced.right_mode = RightMode::Events;
                self.events.layer = id.layer;
                self.select_event(Some(id));
            }
            SourceLocation::Story(id) => {
                self.line = id.line;
                self.advanced.right_mode = RightMode::Storyboard;
                self.select_story(Some(id));
            }
            SourceLocation::Line(line) => {
                self.line = line;
                self.select(None);
                self.windows.line_properties = true;
            }
            SourceLocation::Bpm => self.windows.bpm = true,
            SourceLocation::Noise(index) => {
                self.advanced.right_mode = RightMode::NoiseFields;
                self.noise.animation = false;
                self.select_noise(Some(index), false);
            }
            SourceLocation::Shader(index) => {
                self.advanced.right_mode = RightMode::Shaders;
                self.select_shader(index);
            }
            SourceLocation::Chart => {}
        }
    }
    pub(crate) fn plugin_editor_action_window(&mut self, ctx: &egui::Context) {
        if !self.plugin_editor_action_modal() {
            return;
        }
        let mut open = true;
        let mut cancel = false;
        crate::i18n::window("Plugins")
            .id(egui::Id::new("plugin-editor-action"))
            .open(&mut open)
            .default_width(280.)
            .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
            .show(ctx, |ui| {
                ui.label(crate::i18n::tr("Working…"));
                cancel = ui.button(crate::i18n::tr("Cancel")).clicked();
            });
        if !open || cancel {
            self.cancel_plugin_editor_action();
        }
    }
}

impl From<Tool> for EditorTool {
    fn from(tool: Tool) -> Self {
        match tool {
            Tool::Select => Self::Select,
            Tool::BoxSelect => Self::Box,
            Tool::Erase => Self::Erase,
            Tool::Event => Self::Event,
            Tool::Place(NoteKind::Tap) => Self::Tap,
            Tool::Place(NoteKind::Hold) => Self::Hold,
            Tool::Place(NoteKind::Flick) => Self::Flick,
            Tool::Place(NoteKind::Drag) => Self::Drag,
        }
    }
}
impl From<EditorTool> for Tool {
    fn from(tool: EditorTool) -> Self {
        match tool {
            EditorTool::Select => Self::Select,
            EditorTool::Box => Self::BoxSelect,
            EditorTool::Erase => Self::Erase,
            EditorTool::Event => Self::Event,
            EditorTool::Tap => Self::Place(NoteKind::Tap),
            EditorTool::Hold => Self::Place(NoteKind::Hold),
            EditorTool::Flick => Self::Place(NoteKind::Flick),
            EditorTool::Drag => Self::Place(NoteKind::Drag),
        }
    }
}
impl From<Scope> for EditorSelectionScope {
    fn from(scope: Scope) -> Self {
        match scope {
            Scope::Notes => Self::Notes,
            Scope::Events => Self::Events,
            Scope::Both => Self::Both,
        }
    }
}
impl From<EditorSelectionScope> for Scope {
    fn from(scope: EditorSelectionScope) -> Self {
        match scope {
            EditorSelectionScope::Notes => Self::Notes,
            EditorSelectionScope::Events => Self::Events,
            EditorSelectionScope::Both => Self::Both,
        }
    }
}

#[cfg(test)]
mod tests;
