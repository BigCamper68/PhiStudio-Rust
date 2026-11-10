mod document_view;
mod state;
use state::{FileWorkflow, TimelineState, TransportState, Windows};
mod dialogs;
mod document;
mod editing;
mod header;
mod navigation;
mod properties;
mod resize;
mod sidebar;
mod summaries;
mod toolbar;
mod transport;
use crate::scroll::{TouchCombo as _, TouchScroll as _};
use crate::theme::*;
#[cfg(test)]
use crate::timeline;
use egui::{self, Align, Frame, RichText, Sense, Vec2};
use phistudio_core::{
    Beat, ChartDocument, Command, Editor, Note, NoteId, NoteKind, load_chart, save_chart,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod project_dialogs;

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Select,
    Place(NoteKind),
    Event,
    BoxSelect,
    Erase,
}
pub struct NoteDraft {
    pub id: NoteId,
    pub note: Note,
    pub start: String,
    pub end: String,
    pub volume: f64,
    revision: u64,
}

pub struct Workbench {
    pub(crate) sounds: crate::editor_sounds::State,
    pub(crate) plugins: crate::plugins::State,
    pub(crate) timeline: TimelineState,
    pub(crate) transport: TransportState,
    pub(crate) windows: Windows,
    pub(crate) files: FileWorkflow,
    pub(crate) line_tools: crate::line_tools::State,
    pub(crate) multi_line: crate::multi_line::MultiLine,
    pub(crate) audio_guide: crate::audio_guide::Guide,
    pub(crate) audio_notes: crate::audio_notes_ui::State,
    pub(crate) preview: crate::preview_ui::PreviewState,
    pub(crate) video: crate::video_ui::VideoState,
    pub(crate) text_composition: crate::text_composition_ui::TextState,
    pub(crate) clipboard_history: crate::clipboard_ui::ClipboardState,
    pub(crate) presets: crate::tool_presets_ui::PresetsState,
    pub(crate) trajectory: crate::trajectory_ui::TrajectoryState,
    pub(crate) noise: crate::noise_ui::State,
    pub(crate) shaders: crate::shader_ui::ShaderState,
    pub(crate) line_activity: crate::line_activity::ActivityCache,
    pub(crate) workspace: crate::workspace_ui::WorkspaceState,
    pub(crate) asset_revision: u64,
    pub(crate) playtest: crate::playtest_ui::PlaytestState,
    pub(crate) theme_role: usize,
    pub(crate) palette: Option<crate::color_ui::ColorPicker>,
    pub(crate) skins: crate::skins_ui::SkinsState,
    pub(crate) advanced: crate::advanced_ui::AdvancedState,
    pub(crate) story_tools: crate::story_tools::StoryToolsState,
    pub editor: Editor,
    pub(crate) options: phistudio_core::EditorOptions,
    pub(crate) inspector: crate::inspector::InspectorState,
    pub project_options: phistudio_core::ProjectOptions,
    pub line: usize,
    pub selected: Option<NoteId>,
    pub(crate) tools: crate::toolkit::ToolsState,
    pub skin: Option<phistudio_prpr::EditorSkin>,
    pub(crate) note_highlights: std::collections::BTreeSet<NoteId>,
    pub draft: Option<NoteDraft>,
    pub events: crate::authoring_ui::EventState,
    pub(crate) line_draft: Option<crate::authoring_ui::LineDraft>,
    pub(crate) bpm_draft: Option<crate::authoring_ui::BpmDraft>,
    pub file: Option<PathBuf>,
    pub status: String,
    pub error: Option<String>,
    pub render_error: Option<String>,
    pub resource_warning: Option<String>,
    pub pixels_per_point: f32,
    pub session: u64,
    pub close_requested: bool,
    pub close_confirmed: bool,
    pub managed_storage: bool,
    pub library: Option<phistudio_core::ProjectLibrary>,
    pub(crate) project_covers: Option<crate::covers::Covers>,
    pub show_projects: bool,
    asset_directory: Option<PathBuf>,
    view: document_view::DocumentView,
}

fn sidebar_widths(screen_width: f32) -> std::ops::RangeInclusive<f32> {
    let maximum = (screen_width - 260.).clamp(140., 400.);
    190_f32.min(maximum)..=maximum
}
impl Workbench {
    pub fn new(document: ChartDocument, file: Option<PathBuf>) -> Self {
        let options = file
            .as_deref()
            .map(phistudio_core::load_project_options)
            .transpose();
        let (project_options, error) = match options {
            Ok(options) => (options.unwrap_or_default(), None),
            Err(error) => (
                phistudio_core::ProjectOptions::default(),
                Some(format!("Could not load package settings: {error}")),
            ),
        };
        Self {
            plugins: Default::default(),
            timeline: TimelineState::default(),
            transport: TransportState::default(),
            windows: Windows::default(),
            files: FileWorkflow::default(),
            multi_line: Default::default(),
            sounds: Default::default(),
            audio_guide: Default::default(),
            audio_notes: Default::default(),
            preview: Default::default(),
            video: Default::default(),
            shaders: Default::default(),
            noise: Default::default(),
            line_activity: Default::default(),
            workspace: Default::default(),
            asset_revision: 0,
            playtest: Default::default(),
            theme_role: 0,
            palette: None,
            skins: Default::default(),
            advanced: crate::advanced_ui::AdvancedState {
                playback_rate: 1.,
                ..Default::default()
            },
            story_tools: Default::default(),
            text_composition: Default::default(),
            clipboard_history: Default::default(),
            presets: Default::default(),
            trajectory: Default::default(),
            editor: Editor::new(document),
            options: phistudio_core::EditorOptions::default(),
            inspector: crate::inspector::InspectorState::default(),
            project_options,
            line: 0,
            selected: None,
            tools: crate::toolkit::ToolsState::default(),
            skin: None,
            note_highlights: Default::default(),
            draft: None,
            events: crate::authoring_ui::EventState::default(),
            line_draft: None,
            line_tools: crate::line_tools::State::default(),
            bpm_draft: None,
            file,
            status: "Select a note or a placement tool".into(),
            error,
            render_error: None,
            resource_warning: None,
            pixels_per_point: 1.,
            session: 0,
            close_requested: false,
            close_confirmed: false,
            managed_storage: false,
            library: None,
            project_covers: None,
            show_projects: false,
            asset_directory: None,
            view: Default::default(),
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, canceled: bool) {
        if canceled
            || ctx.input(|i| {
                !i.raw.has_focus
                    || i.events.iter().any(|event| {
                        matches!(
                            event,
                            egui::Event::Touch {
                                phase: egui::TouchPhase::Cancel,
                                ..
                            }
                        )
                    })
            })
        {
            self.cancel_plugin_editor_action();
        }
        self.plugin_editor_action_preflight();
        self.plugin_file_hook_preflight();
        self.plugin_input_preflight(ctx, canceled);
        self.poll_plugins();
        self.plugin_provider_preflight();
        if self.plugins.working() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        if canceled {
            self.cancel_noise_bounds_drag();
        }
        let _language = crate::i18n::Scope::enter(self.options.language);
        crate::numeric::configure(ctx, self.options.numeric_drag);
        self.poll_audio_notes();
        self.pixels_per_point = ctx.pixels_per_point();
        crate::theme::apply(ctx, &self.options.theme);
        crate::scroll::begin_frame(ctx);
        if self.playtest.active() {
            self.playtest_ui(ctx);
            return;
        }
        self.capture_shortcut(ctx);
        self.color_window(ctx);
        if self.preview.fullscreen {
            if canceled
                && !ctx.input(|i| {
                    i.events
                        .iter()
                        .any(|e| matches!(e, egui::Event::Touch { .. }))
                })
            {
                self.preview.drag = None;
                self.preview.touches.clear();
                self.preview.blocked = false;
                self.transport.playing = false;
            }
            self.fullscreen_preview(ctx);
            return;
        }
        self.line = self.line.min(self.editor.document().line_count() - 1);
        if !self.editor.is_dirty()
            && let Some(path) = self.files.pending_open.take()
        {
            self.load(path);
        }
        self.pixels_per_point = ctx.pixels_per_point();
        if canceled {
            if self.transport.scrubbing {
                self.transport.playing = false;
            }
            self.transport.scrubbing = false;
            self.timeline.drag = None;
            self.draft = None;
        }
        if self.show_projects {
            self.projects_home(ctx);
            self.dialogs(ctx);
            return;
        }
        if !ctx.wants_keyboard_input() && !ctx.memory(|m| m.any_popup_open()) && !self.modal_open()
        {
            self.tool_shortcuts(ctx);
        }
        // Windows are registered before the grid so an opening window also owns
        // its input on the first frame; painting still follows egui layer order.
        self.dialogs(ctx);
        self.header(ctx);
        self.toolbar(ctx);
        let resizing_sidebar = self.sidebar(ctx, canceled);
        egui::CentralPanel::default()
            .frame(Frame::none())
            .show(ctx, |ui| {
                self.plugin_timeline(ui, canceled, resizing_sidebar);
            });
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod performance;
