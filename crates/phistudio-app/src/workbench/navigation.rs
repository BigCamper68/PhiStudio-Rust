use super::*;

impl Workbench {
    pub fn modal_open(&self) -> bool {
        self.show_projects || self.modal_open_except_projects()
    }

    pub(crate) fn modal_open_except_projects(&self) -> bool {
        self.plugin_editor_action_modal() || self.workflow_modal_open()
    }

    pub(crate) fn modal_open_except_editor_action(&self) -> bool {
        self.show_projects || self.workflow_modal_open()
    }

    fn workflow_modal_open(&self) -> bool {
        self.plugin_dialog_modal() || self.workflow_modal_open_without_plugin_dialogs()
    }

    pub(crate) fn workflow_modal_open_without_plugin_dialogs(&self) -> bool {
        self.plugins.permissions_open()
            || self.plugin_provider_modal()
            || self.plugin_file_hook_modal()
            || self.workspace.view.is_some()
            || self.line_tools.show
            || self.video.open
            || self.playtest.setup
            || self.playtest.active()
            || self.palette.is_some()
            || self.files.modal_open()
            || self.close_requested
    }

    pub(crate) fn preferences(&self) -> phistudio_core::EditorPreferences {
        phistudio_core::EditorPreferences {
            multi_line: self
                .multi_line
                .preferences(self.editor.document().line_count()),
            version: 1,
            video: self.video.settings.clone(),
            lanes: self.timeline.lanes,
            division: self.timeline.division,
            zoom: self.timeline.zoom,
            timeline_split: self.timeline.split,
            sidebar: self.windows.sidebar,
            floating_preview: self.preview.floating,
            floating_properties: self.windows.floating_properties,
            grid_window: self.windows.grid,
            music_volume: self.transport.music_volume,
            hitsound_volume: self.transport.hitsound_volume,
            options: self.options.clone(),
        }
        .normalized()
    }

    pub(crate) fn apply_preferences(&mut self, value: &phistudio_core::EditorPreferences) {
        let value = value.clone().normalized();
        self.multi_line
            .apply_preferences(&value.multi_line, self.editor.document().line_count());
        self.video.settings = value.video;
        self.timeline.lanes = value.lanes;
        self.timeline.division = value.division;
        self.timeline.zoom = value.zoom;
        self.timeline.split = value.timeline_split;
        self.windows.sidebar = value.sidebar;
        self.preview.floating = value.floating_preview;
        self.windows.floating_properties = value.floating_properties;
        self.windows.grid = value.grid_window;
        self.transport.music_volume = value.music_volume;
        self.transport.hitsound_volume = value.hitsound_volume;
        self.options = value.options;
    }

    pub fn back(&mut self) {
        if self.cancel_plugin_editor_action() {
            return;
        }
        self.cancel_plugin_input();
        if self.cancel_plugin_file_hook() {
            return;
        }
        if self.cancel_plugin_provider_ui() {
            return;
        }
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
        self.timeline.cancel_gesture();
        self.transport.playing = false;
        if self.files.busy {
            return;
        }
        if self.files.recovery.is_some() {
            return;
        }
        if self.files.media.take().is_some() || self.files.delete_project.take().is_some() {
            return;
        }
        if std::mem::take(&mut self.line_tools.show) {
            return;
        }
        if self.files.export_package.take().is_some() || self.files.new_chart.take().is_some() {
            return;
        }
        if self.workspace.view.take().is_some() {
            self.workspace.audition = Some(None);
            return;
        }
        if self.show_projects && !self.files.modal_open() && self.plugins.close_top(false) {
            return;
        }
        if std::mem::take(&mut self.show_projects) {
            return;
        }
        if std::mem::take(&mut self.files.package_dialog)
            || self.files.imported_project.take().is_some()
        {
            return;
        }
        if self.files.cancel_pending_open() {
            return;
        }
        if self.close_advanced_window() {
            return;
        }
        if self.windows.edit {
            self.windows.edit = false;
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
        if self.windows.close_auxiliary() {
            return;
        }
        if self.plugins.close_top(true) {
            return;
        }
        if self.preview.floating {
            self.preview.floating = false;
        } else if !self.noise_back() && !self.shader_back() {
            self.close_requested = true;
        }
    }
}
