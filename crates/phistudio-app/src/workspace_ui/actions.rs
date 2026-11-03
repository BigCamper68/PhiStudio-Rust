use super::*;

impl Workbench {
    pub(crate) fn current_workspace(&self) -> Option<PathBuf> {
        let root = self.workspace.root.as_ref()?.join("projects");
        let relative = self.file.as_ref()?.strip_prefix(&root).ok()?;
        Some(root.join(relative.components().next()?))
    }

    pub(crate) fn workspace_request(&mut self, job: WorkspaceJob) {
        self.transport.playing = false;
        self.files.request = Some(FileRequest::Workspace(job));
    }

    pub(crate) fn open_workspace(&mut self, view: View) {
        self.transport.playing = false;
        self.workspace.confirm_delete = None;
        self.workspace.confirm_restore = false;
        self.workspace.line = self.line;
        self.workspace.layer = self.events.layer;
        let job = match &view {
            View::Patterns => Some(WorkspaceJob::Patterns),
            View::Resources => Some(WorkspaceJob::Resources),
            View::Versions(p) => {
                self.workspace.repository = Default::default();
                Some(WorkspaceJob::Repository {
                    workspace: p.clone(),
                    revision: None,
                    action: repository::Action::Refresh,
                })
            }
            View::Project(_) => None,
        };
        self.workspace.view = Some(view);
        self.workspace.loading = job.as_ref().is_some_and(WorkspaceJob::read_only);
        if let Some(job) = job {
            self.workspace_request(job);
        }
    }

    pub(crate) fn accept_resource_file(&mut self, file: MediaFile) {
        let replace = self.workspace.resource_target.take();
        self.workspace_request(WorkspaceJob::ImportResource { file, replace });
    }

    pub(crate) fn workspace_result(&mut self, session: u64, result: WorkspaceResult) {
        use WorkspaceResult::*;
        if session != self.session && !matches!(&result, Patterns(_) | Library(_)) {
            return;
        }
        self.error = None;
        match result {
            Repository {
                workspace,
                status,
                detail,
                notice,
            } => {
                if self.workspace.view == Some(View::Versions(workspace)) {
                    self.accept_repository(status);
                    if let Some(detail) = detail {
                        self.workspace.repository.detail = Some(detail);
                    }
                    self.workspace.repository.notice = notice;
                }
            }
            RepositoryWorking {
                workspace,
                copy,
                status,
            } => {
                if self.current_workspace().as_ref() != Some(&workspace) {
                    self.offer_version_copy(*copy);
                    return;
                }
                let view = self.workspace.view.clone();
                self.replace_document(
                    copy.document,
                    Some(copy.location.chart),
                    copy.location.assets,
                );
                self.editor = core::Editor::recovered(self.editor.document().clone());
                self.project_options = copy.options;
                self.workspace.view = view.or(Some(View::Versions(workspace)));
                self.accept_repository(status);
                self.workspace.repository.detail = None;
                self.workspace.repository.notice =
                    "Version loaded as unsaved work; press Save to keep it in the project".into();
                self.show_projects = false;
            }
            RepositoryExport(_) => unreachable!("History exports are handled by FileManager"),
            Patterns(entries) => {
                self.workspace.patterns = entries;
                self.status = "Pattern library updated".into();
            }
            Insert(clipboard, settings) => match self.editor.insert_pattern(
                &clipboard,
                settings.line,
                settings.layer,
                settings.beat,
                settings.mirrored,
            ) {
                Ok(selected) => {
                    self.line = settings.line;
                    self.set_object_selection(selected.normal, selected.story);
                    self.status = "Pattern inserted · Undo removes the whole insertion".into();
                    self.workspace.view = None;
                }
                Err(e) => self.error = Some(e.to_string()),
            },
            Resources {
                entries,
                undo,
                changed,
            } => {
                self.workspace.resources = entries;
                let imported = undo.as_ref().map(|u| u.name().to_owned());
                if changed {
                    self.workspace.resource_undo = undo;
                    self.asset_revision = self.asset_revision.wrapping_add(1);
                    self.sync_plugin_project();
                    self.project_covers = None;
                    self.workspace.inspection = None;
                    self.workspace.image = None;
                    self.workspace.audition = Some(None);
                }
                if std::mem::take(&mut self.line_tools.importing)
                    && let Some(name) = imported
                {
                    self.stage_line_texture(name);
                }
                self.status = "Project resources updated".into();
            }
            Inspected(inspection) => {
                self.workspace.image = None;
                self.workspace.inspection = Some(inspection);
            }
            Library(library) => {
                self.library = Some(library);
                self.workspace.view = None;
                self.show_projects = true;
                self.status = "Project library updated".into();
            }
        }
    }
}
