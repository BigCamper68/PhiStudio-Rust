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
use crate::{theme::*, timeline};
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

struct NoteCache {
    revision: u64,
    notes: Arc<Vec<(usize, Note)>>,
    index: crate::note_index::NoteIndex,
}

pub struct Workbench {
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
    pub(crate) shaders: crate::shader_ui::ShaderState,
    pub(crate) line_activity: crate::line_activity::ActivityCache,
    pub(crate) workspace: crate::workspace_ui::WorkspaceState,
    pub(crate) asset_revision: u64,
    pub(crate) playtest: crate::playtest_ui::PlaytestState,
    pub(crate) theme_role: usize,
    pub(crate) properties_scroll: bool,
    pub(crate) palette: Option<crate::color_ui::ColorPicker>,
    pub(crate) skins: crate::skins_ui::SkinsState,
    pub(crate) camera_x: f64,
    pub(crate) navigation_axis: Option<bool>,
    pub(crate) advanced: crate::advanced_ui::AdvancedState,
    pub(crate) story_tools: crate::story_tools::StoryToolsState,
    pub editor: Editor,
    pub(crate) options: phistudio_core::EditorOptions,
    pub(crate) inspector: crate::inspector::InspectorState,
    pub(crate) show_edit: bool,
    pub project_options: phistudio_core::ProjectOptions,
    pub line: usize,
    pub selected: Option<NoteId>,
    pub(crate) tools: crate::toolkit::ToolsState,
    pub tool: Tool,
    pub beat: f64,
    pub lanes: usize,
    pub division: i32,
    pub zoom: f32,
    pub timeline_split: f32,
    pub playing: bool,
    pub(crate) music_volume: f32,
    pub(crate) hitsound_volume: f32,
    pub(crate) show_settings: bool,
    pub skin: Option<phistudio_prpr::EditorSkin>,
    pub(crate) note_highlights: std::collections::BTreeSet<NoteId>,
    pub grid_area: egui::Rect,
    pub grid_hover: Option<egui::Pos2>,
    pub grid_input: crate::grid_input::GridInput,
    pub drag: Option<timeline::Gesture>,
    pub draft: Option<NoteDraft>,
    pub events: crate::authoring_ui::EventState,
    pub(crate) line_draft: Option<crate::authoring_ui::LineDraft>,
    pub(crate) bpm_draft: Option<crate::authoring_ui::BpmDraft>,
    pub(crate) show_line_properties: bool,
    pub(crate) show_bpm: bool,
    pub file: Option<PathBuf>,
    pub status: String,
    pub error: Option<String>,
    pub render_error: Option<String>,
    pub resource_warning: Option<String>,
    pub path_input: String,
    pub package_dialog: bool,
    pub pending_open: Option<PathBuf>,
    pub show_help: bool,
    pub pixels_per_point: f32,
    pub session: u64,
    pub close_requested: bool,
    pub close_confirmed: bool,
    pub(crate) compact_properties: bool,
    show_grid_settings: bool,
    pub(crate) show_context: bool,
    pub file_busy: bool,
    pub native_request: Option<crate::files::FileRequest>,
    pub imported_project: Option<phistudio_core::ImportedProject>,
    pub managed_storage: bool,
    pub library: Option<phistudio_core::ProjectLibrary>,
    pub(crate) project_covers: Option<crate::covers::Covers>,
    pub show_projects: bool,
    pub(crate) new_chart: Option<project_dialogs::NewChart>,
    pub(crate) export_package: Option<phistudio_core::ProjectExport>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub(crate) export_path: String,
    pub(crate) timeline_scrub: bool,
    pub(crate) music_duration: Option<f64>,
    seek_extent: Option<(u64, f64)>,
    pub recovery: Option<phistudio_core::RecoverySnapshot>,
    pub recovery_loading: bool,
    pub(crate) media_dialog: Option<crate::media::MediaKind>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub(crate) media_path: String,
    pub(crate) delete_project: Option<(PathBuf, String)>,
    pub(crate) last_deleted: Option<phistudio_core::TrashedProject>,
    asset_directory: Option<PathBuf>,
    pending_asset_directory: Option<PathBuf>,
    note_cache: std::collections::BTreeMap<usize, NoteCache>,
    line_cache: Option<summaries::LineCache>,
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
            multi_line: Default::default(),
            audio_guide: Default::default(),
            audio_notes: Default::default(),
            preview: Default::default(),
            video: Default::default(),
            shaders: Default::default(),
            line_activity: Default::default(),
            workspace: Default::default(),
            asset_revision: 0,
            playtest: Default::default(),
            theme_role: 0,
            properties_scroll: false,
            palette: None,
            skins: Default::default(),
            camera_x: 0.,
            navigation_axis: None,
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
            show_edit: false,
            project_options,
            line: 0,
            selected: None,
            tools: crate::toolkit::ToolsState::default(),
            tool: Tool::Select,
            beat: 0.,
            division: 4,
            lanes: 11,
            zoom: 96.,
            timeline_split: 0.54,
            playing: false,
            music_volume: 1.,
            hitsound_volume: 1.,
            show_settings: false,
            skin: None,
            note_highlights: Default::default(),
            grid_area: egui::Rect::NOTHING,
            grid_hover: None,
            grid_input: crate::grid_input::GridInput::default(),
            drag: None,
            draft: None,
            events: crate::authoring_ui::EventState::default(),
            line_draft: None,
            line_tools: crate::line_tools::State::default(),
            bpm_draft: None,
            show_line_properties: false,
            show_bpm: false,
            file,
            status: "Select a note or a placement tool".into(),
            error,
            render_error: None,
            resource_warning: None,
            path_input: String::new(),
            package_dialog: false,
            pending_open: None,
            show_help: false,
            pixels_per_point: 1.,
            session: 0,
            close_requested: false,
            close_confirmed: false,
            compact_properties: false,
            show_grid_settings: false,
            show_context: true,
            file_busy: false,
            native_request: None,
            imported_project: None,
            managed_storage: false,
            library: None,
            project_covers: None,
            show_projects: false,
            new_chart: None,
            export_package: None,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            export_path: String::new(),
            timeline_scrub: false,
            seek_extent: None,
            music_duration: None,
            recovery: None,
            recovery_loading: false,
            media_dialog: None,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            media_path: String::new(),
            delete_project: None,
            last_deleted: None,
            asset_directory: None,
            pending_asset_directory: None,
            note_cache: Default::default(),
            line_cache: None,
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, canceled: bool) {
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
                self.playing = false;
            }
            self.fullscreen_preview(ctx);
            return;
        }
        self.line = self.line.min(self.editor.document().line_count() - 1);
        if !self.editor.is_dirty()
            && let Some(path) = self.pending_open.take()
        {
            self.load(path);
        }
        self.pixels_per_point = ctx.pixels_per_point();
        if canceled {
            if self.timeline_scrub {
                self.playing = false;
            }
            self.timeline_scrub = false;
            self.drag = None;
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
                timeline::show(self, ui, canceled, resizing_sidebar);
            });
    }
}

#[cfg(test)]
mod tests;
