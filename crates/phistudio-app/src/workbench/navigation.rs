use super::*;

impl Workbench {
    pub fn modal_open(&self) -> bool {
        self.workspace.view.is_some()
            || self.line_tools.show
            || self.video.open
            || self.playtest.setup
            || self.playtest.active()
            || self.palette.is_some()
            || self.file_busy
            || self.show_projects
            || self.media_dialog.is_some()
            || self.delete_project.is_some()
            || self.new_chart.is_some()
            || self.export_package.is_some()
            || self.recovery.is_some()
            || self.package_dialog
            || self.pending_open.is_some()
            || self.imported_project.is_some()
            || self.close_requested
    }

    pub(crate) fn preferences(&self) -> phistudio_core::EditorPreferences {
        phistudio_core::EditorPreferences {
            multi_line: self
                .multi_line
                .preferences(self.editor.document().line_count()),
            version: 1,
            video: self.video.settings.clone(),
            lanes: self.lanes,
            division: self.division,
            zoom: self.zoom,
            timeline_split: self.timeline_split,
            sidebar: self.show_context,
            floating_preview: self.preview.floating,
            floating_properties: self.compact_properties,
            grid_window: self.show_grid_settings,
            music_volume: self.music_volume,
            hitsound_volume: self.hitsound_volume,
            options: self.options.clone(),
        }
        .normalized()
    }

    pub(crate) fn apply_preferences(&mut self, value: &phistudio_core::EditorPreferences) {
        let value = value.clone().normalized();
        self.multi_line
            .apply_preferences(&value.multi_line, self.editor.document().line_count());
        self.video.settings = value.video;
        self.lanes = value.lanes;
        self.division = value.division;
        self.zoom = value.zoom;
        self.timeline_split = value.timeline_split;
        self.show_context = value.sidebar;
        self.preview.floating = value.floating_preview;
        self.compact_properties = value.floating_properties;
        self.show_grid_settings = value.grid_window;
        self.music_volume = value.music_volume;
        self.hitsound_volume = value.hitsound_volume;
        self.options = value.options;
    }

    pub fn back(&mut self) {
        if self.video_back() {
            return;
        }
        if self.playtest_back() {
            return;
        }
        if self.preview.fullscreen {
            self.close_fullscreen_preview();
            return;
        }
        self.drag = None;
        self.grid_input.cancel();
        self.playing = false;
        if self.file_busy {
            return;
        }
        if self.recovery.is_some() {
            return;
        }
        if self.media_dialog.take().is_some() || self.delete_project.take().is_some() {
            return;
        }
        if std::mem::take(&mut self.line_tools.show) {
            return;
        }
        if self.export_package.take().is_some() || self.new_chart.take().is_some() {
            return;
        }
        if self.workspace.view.take().is_some() {
            self.workspace.audition = Some(None);
            return;
        }
        if std::mem::take(&mut self.show_projects) {
            return;
        }
        if std::mem::take(&mut self.package_dialog) || self.imported_project.take().is_some() {
            return;
        }
        if self.pending_open.take().is_some() {
            self.pending_asset_directory = None;
            return;
        }
        if self.close_advanced_window() {
            return;
        }
        if self.show_edit {
            self.show_edit = false;
            return;
        }
        if self.inspector.close_top_window() {
            return;
        }
        if self.audio_notes.show {
            self.audio_notes.close();
            return;
        }
        if self.tools.show {
            self.tools.show = false;
            return;
        }
        if std::mem::take(&mut self.audio_guide.show) {
            return;
        }
        if std::mem::take(&mut self.multi_line.show) {
            return;
        }
        if self.show_settings {
            self.show_settings = false;
        } else if self.show_help {
            self.show_help = false;
        } else if self.show_bpm {
            self.show_bpm = false;
        } else if self.show_line_properties {
            self.show_line_properties = false;
        } else if self.compact_properties {
            self.compact_properties = false;
        } else if self.show_grid_settings {
            self.show_grid_settings = false;
        } else if self.preview.floating {
            self.preview.floating = false;
        } else if !self.shader_back() {
            self.close_requested = true;
        }
    }
}
