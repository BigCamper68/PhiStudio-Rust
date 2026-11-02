use super::*;

impl Workbench {
    pub fn restore_recovery(&mut self) {
        let Some(snapshot) = self.files.recovery.take() else {
            return;
        };
        let (file, assets) = snapshot
            .location
            .map_or((None, None), |p| (Some(p.chart), p.assets));
        self.replace_document(snapshot.document, file, assets);
        self.editor = Editor::recovered(self.editor.document().clone());
        self.transport.beat = snapshot.beat;
        self.line = snapshot.line;
        self.status = "Unsaved changes restored; save to keep them in the project".into();
    }

    pub fn accept_saved_workspace(
        &mut self,
        location: phistudio_core::ProjectLocation,
        unchanged: bool,
    ) {
        self.file = Some(location.chart);
        self.asset_directory = location.assets;
        if unchanged {
            self.editor.mark_saved();
        }
        self.error = None;
        self.status = if unchanged {
            "Project saved in the library"
        } else {
            "Project saved; newer edits are still unsaved"
        }
        .into();
    }

    pub fn asset_root(&self) -> Option<&Path> {
        if let Some(root) = &self.asset_directory {
            return Some(root);
        }
        self.file.as_deref().and_then(Path::parent).map(|p| {
            if p.as_os_str().is_empty() {
                Path::new(".")
            } else {
                p
            }
        })
    }

    pub fn open(&mut self, path: PathBuf) {
        if self.editor.is_dirty() {
            self.files.pending_open = Some(path);
            return;
        }
        self.load(path);
    }

    pub fn discard_and_open(&mut self, path: PathBuf) {
        self.files.pending_open = None;
        self.load(path);
    }

    pub(super) fn load(&mut self, path: PathBuf) {
        match load_chart(&path) {
            Ok(document) => {
                let assets = self.files.pending_asset_directory.take();
                self.replace_document(document, Some(path), assets);
                self.status = "Project opened".into();
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub(crate) fn replace_document(
        &mut self,
        document: ChartDocument,
        file: Option<PathBuf>,
        assets: Option<PathBuf>,
    ) {
        let mut workspace = std::mem::take(&mut self.workspace);
        let mut audio_guide = std::mem::take(&mut self.audio_guide);
        audio_guide.clear_project();
        let mut audio_notes = std::mem::take(&mut self.audio_notes);
        audio_notes.clear_project();
        workspace.view = None;
        workspace.resources.clear();
        workspace.resource_undo = None;
        workspace.inspection = None;
        workspace.image = None;
        workspace.audition = Some(None);
        let session = self.session + 1;
        let texture = self.preview.texture;
        let skin = self.skin.clone();
        let skins = std::mem::take(&mut self.skins);
        let presets = std::mem::take(&mut self.presets);
        let clipboard_history = std::mem::take(&mut self.clipboard_history);
        let preferences = self.preferences();
        let clipboard = self.tools.clipboard.clone();
        let last_deleted = self.files.last_deleted.clone();
        let floating = self.preview.floating;
        let show_context = self.windows.sidebar;
        let managed_storage = self.managed_storage;
        let timeline_split = self.timeline.split;
        *self = Self::new(document, file);
        if self
            .file
            .as_deref()
            .is_some_and(phistudio_core::vcs::is_working_copy)
        {
            self.editor = Editor::recovered(self.editor.document().clone());
        }
        self.session = session;
        self.workspace = workspace;
        self.audio_guide = audio_guide;
        self.audio_notes = audio_notes;
        self.multi_line.lines = vec![self.line];
        self.preview.texture = texture;
        self.skin = skin;
        self.skins = skins;
        self.presets = presets;
        self.clipboard_history = clipboard_history;
        self.preview.floating = floating;
        self.windows.sidebar = show_context;
        self.asset_directory = assets;
        self.reload_project_options();
        self.managed_storage = managed_storage;
        self.timeline.split = timeline_split;
        self.apply_preferences(&preferences);
        self.files.last_deleted = last_deleted;
        self.tools.clipboard = clipboard;
    }

    pub(super) fn reload_project_options(&mut self) {
        let Some(path) = &self.file else {
            return;
        };
        match phistudio_core::load_project_options_with_assets(
            path,
            self.asset_directory.as_deref(),
        ) {
            Ok(options) => self.project_options = options,
            Err(error) => self.error = Some(format!("Could not load package settings: {error}")),
        }
    }

    pub(crate) fn offer_version_copy(&mut self, copy: phistudio_core::vcs::WorkingCopy) {
        self.workspace.view = None;
        self.show_projects = false;
        self.files.pending_asset_directory = copy.location.assets;
        self.open(copy.location.chart);
    }

    pub fn offer_project(&mut self, project: phistudio_core::ImportedProject) {
        self.show_projects = false;
        if let Some(index) = project.suggested {
            self.files.pending_asset_directory = Some(project.assets);
            self.open(project.charts[index].path.clone());
        } else {
            self.files.imported_project = Some(project);
        }
    }

    pub fn save(&mut self, path: PathBuf) {
        let result = (|| {
            if let Some(root) = &self.workspace.root
                && phistudio_core::vcs::save_working_copy(
                    root,
                    &path,
                    self.editor.document(),
                    &self.project_options,
                )?
            {
                return Ok(());
            }
            save_chart(self.editor.document(), &path)
        })();
        match result {
            Ok(()) => {
                self.editor.mark_saved();
                self.file = Some(path);
                self.status = "Chart saved".into();
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub(crate) fn save_action(&mut self) {
        if !self.flush_shader_draft() {
            return;
        }
        if self.files.busy {
            return;
        }
        if let Some(path) = self.file.clone() {
            self.save(path);
        } else {
            self.files.request = Some(crate::files::FileRequest::SaveWorkspace);
        }
    }

    pub(crate) fn open_package_action(&mut self) {
        if !self.flush_shader_draft() {
            return;
        }
        if self.files.busy {
            return;
        }
        self.transport.playing = false;
        self.timeline.cancel_gesture();
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            self.files.request = Some(crate::files::FileRequest::Package);
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            self.files.path_input.clear();
            self.files.package_dialog = true;
        }
    }
}
