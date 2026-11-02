//! Editor interaction state, separated from document commands and rendering.
use super::{Tool, project_dialogs};
use std::path::PathBuf;

pub(crate) struct TimelineState {
    pub(crate) tool: Tool,
    pub(crate) lanes: usize,
    pub(crate) division: i32,
    pub(crate) zoom: f32,
    pub(crate) split: f32,
    pub(crate) camera_x: f64,
    pub(crate) navigation_axis: Option<bool>,
    pub(crate) area: egui::Rect,
    pub(crate) hover: Option<egui::Pos2>,
    pub(crate) input: crate::grid_input::GridInput,
    pub(crate) drag: Option<crate::timeline::Gesture>,
}
impl Default for TimelineState {
    fn default() -> Self {
        Self {
            tool: Tool::Select,
            lanes: 11,
            division: 4,
            zoom: 96.,
            split: 0.54,
            camera_x: 0.,
            navigation_axis: None,
            area: egui::Rect::NOTHING,
            hover: None,
            input: crate::grid_input::GridInput::default(),
            drag: None,
        }
    }
}

pub(crate) struct TransportState {
    pub(crate) beat: f64,
    pub(crate) playing: bool,
    pub(crate) music_duration: Option<f64>,
    pub(crate) music_volume: f32,
    pub(crate) hitsound_volume: f32,
    pub(crate) scrubbing: bool,
}
impl Default for TransportState {
    fn default() -> Self {
        Self {
            beat: 0.,
            playing: false,
            music_duration: None,
            music_volume: 1.,
            hitsound_volume: 1.,
            scrubbing: false,
        }
    }
}

pub(crate) struct Windows {
    pub(crate) settings: bool,
    pub(crate) help: bool,
    pub(crate) line_properties: bool,
    pub(crate) bpm: bool,
    pub(crate) floating_properties: bool,
    pub(crate) grid: bool,
    pub(crate) sidebar: bool,
    pub(crate) edit: bool,
    pub(crate) scroll_to_properties: bool,
}
impl Default for Windows {
    fn default() -> Self {
        Self {
            settings: false,
            help: false,
            line_properties: false,
            bpm: false,
            floating_properties: false,
            grid: false,
            sidebar: true,
            edit: false,
            scroll_to_properties: false,
        }
    }
}

#[derive(Default)]
pub(crate) struct FileWorkflow {
    pub(crate) busy: bool,
    pub(crate) request: Option<crate::files::FileRequest>,
    pub(crate) package_dialog: bool,
    pub(crate) path_input: String,
    pub(crate) pending_open: Option<PathBuf>,
    pub(crate) imported_project: Option<phistudio_core::ImportedProject>,
    pub(crate) recovery: Option<phistudio_core::RecoverySnapshot>,
    pub(crate) recovery_loading: bool,
    pub(crate) media: Option<crate::media::MediaKind>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub(crate) media_path: String,
    pub(crate) export_package: Option<phistudio_core::ProjectExport>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub(crate) export_path: String,
    pub(crate) delete_project: Option<(PathBuf, String)>,
    pub(crate) new_chart: Option<project_dialogs::NewChart>,
    pub(crate) last_deleted: Option<phistudio_core::TrashedProject>,
    pub(crate) pending_asset_directory: Option<PathBuf>,
}

impl TimelineState {
    pub fn cancel_gesture(&mut self) {
        self.drag = None;
        self.input.cancel();
    }
}
impl Windows {
    /// Match the established Back priority after feature-specific dialogs close.
    pub fn close_auxiliary(&mut self) -> bool {
        for open in [
            &mut self.settings,
            &mut self.help,
            &mut self.bpm,
            &mut self.line_properties,
            &mut self.floating_properties,
            &mut self.grid,
        ] {
            if std::mem::take(open) {
                return true;
            }
        }
        false
    }
}
impl FileWorkflow {
    pub fn modal_open(&self) -> bool {
        self.busy
            || self.media.is_some()
            || self.delete_project.is_some()
            || self.new_chart.is_some()
            || self.export_package.is_some()
            || self.recovery.is_some()
            || self.package_dialog
            || self.pending_open.is_some()
            || self.imported_project.is_some()
    }
    pub fn cancel_pending_open(&mut self) -> bool {
        if self.pending_open.take().is_some() {
            self.pending_asset_directory = None;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::workbench::Workbench;
    use phistudio_core::ChartDocument;
    #[test]
    fn native_back_closes_one_auxiliary_window_at_a_time_in_the_existing_order() {
        let mut app = Workbench::new(ChartDocument::empty("Window priority", 120.).unwrap(), None);
        app.windows.settings = true;
        app.windows.help = true;
        app.windows.bpm = true;
        app.windows.line_properties = true;
        app.windows.floating_properties = true;
        app.windows.grid = true;
        for count in (0..6).rev() {
            app.back();
            let open = [
                app.windows.settings,
                app.windows.help,
                app.windows.bpm,
                app.windows.line_properties,
                app.windows.floating_properties,
                app.windows.grid,
            ];
            assert_eq!(open.iter().filter(|v| **v).count(), count);
            assert!(open[6 - count..].iter().all(|v| *v));
            assert!(!app.close_requested);
        }
        assert!(!app.editor.is_dirty());
        app.back();
        assert!(app.close_requested);
    }
}
