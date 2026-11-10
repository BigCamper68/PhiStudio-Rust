use super::*;

impl FileManager {
    pub fn update(&mut self, app: &mut Workbench) {
        app.workspace.root = self.root.clone();
        if let Some(root) = &self.root {
            app.update_clipboard_history(root);
        }
        if let Some(root) = &self.root
            && let Some(error) = app.presets.update(root)
        {
            app.error = Some(error);
        }
        app.skins.root = self.root.as_ref().map(|root| root.join("skins"));
        if let Some(rx) = &self.skins_loading {
            match rx.try_recv() {
                Ok(result) => {
                    self.skins_loading = None;
                    match result {
                        Ok(entries) => {
                            app.skins.entries = entries;
                            app.skins.loaded = true;
                        }
                        Err(e) => app.error = Some(format!("Could not load skins: {e}")),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.skins_loading = None;
                    app.error = Some("Skin library load interrupted".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        self.preferences.update(app);
        self.checkpoints.update(app);
        if let (Some(root), Some(path)) = (&self.root, &app.file)
            && !app.editor.is_dirty()
        {
            let key = (
                app.session,
                app.editor.revision(),
                path.clone(),
                app.asset_root().map(std::path::Path::to_owned),
            );
            if self.recorded.as_ref() != Some(&key) {
                let result = phistudio_core::remember_project(
                    root,
                    phistudio_core::ProjectLocation {
                        chart: key.2.clone(),
                        assets: key.3.clone(),
                    },
                );
                if let Err(error) = result {
                    app.error = Some(format!("Could not remember the project: {error}"));
                }
                self.recorded = Some(key);
            }
        }
        if let Some(query) = &self.query {
            let result = match query.result.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("File operation interrupted".into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                let query = self.query.take().unwrap();
                app.workspace.loading = false;
                match result {
                    Ok(result) if query.view == app.workspace.view => {
                        app.workspace_result(query.session, result)
                    }
                    Err(error)
                        if query.session == app.session && query.view == app.workspace.view =>
                    {
                        app.error = Some(error)
                    }
                    _ => {}
                }
            }
        }
        if let Some(receiver) = &self.worker {
            match receiver.try_recv() {
                Ok(result) => {
                    self.worker = None;
                    app.files.busy = false;
                    match result {
                        Ok(WorkerResult::ProviderSource { operation, result }) => {
                            app.receive_plugin_provider_source(operation, result)
                        }
                        Ok(WorkerResult::ProviderImported { operation, result }) => {
                            app.receive_plugin_provider_import(operation, result)
                        }
                        #[cfg(not(any(target_os = "android", target_os = "ios")))]
                        Ok(WorkerResult::ProviderWritten { operation, result }) => {
                            app.receive_plugin_provider_write(operation, result)
                        }
                        Ok(WorkerResult::ProviderResource {
                            operation,
                            kind,
                            result,
                        }) => app.receive_plugin_provider_resource(operation, kind, result),
                        Ok(WorkerResult::Workspace { session, result }) => {
                            app.workspace_result(session, result);
                            self.recorded = None;
                        }
                        Ok(WorkerResult::SoundCatalog { catalog, target }) => {
                            if app.sound_target_valid(&target)
                                && let Some(draft) = &mut app.inspector.sound
                            {
                                draft.catalog = catalog;
                            }
                        }
                        Ok(WorkerResult::Sound { reference, target }) => {
                            if let Some(draft) = &mut app.inspector.sound {
                                draft.reference = reference.clone();
                                if !draft.catalog.contains(&reference) {
                                    draft.catalog.push(reference.clone());
                                }
                            }
                            app.apply_sound(target, Some(reference));
                        }
                        Ok(WorkerResult::NoteVolume { gain, target }) => {
                            if app.sound_target_valid(&target) && target.ids.len() == 1 {
                                app.execute(phistudio_core::Command::SetNoteVolume {
                                    id: target.ids[0],
                                    gain: Some(gain),
                                });
                            }
                        }
                        Ok(WorkerResult::Imported(project)) => app.offer_project(project),
                        Ok(WorkerResult::Created(location)) => app.open_created_project(location),
                        Ok(WorkerResult::Skins { entries, selected }) => {
                            self.skins_loading = None;
                            app.skins.entries = entries;
                            if let Some(id) = selected {
                                app.options.skin = Some(id);
                            }
                            if app
                                .options
                                .skin
                                .as_ref()
                                .is_some_and(|id| !app.skins.entries.iter().any(|e| &e.id == id))
                            {
                                app.options.skin = None;
                            }
                            app.skins.loaded = true;
                        }
                        Ok(WorkerResult::SoundPack(pack)) => {
                            if app.options.editor_sounds.add_pack(pack) {
                                app.status = "Sound pack imported".into();
                                app.error = None;
                            } else {
                                app.error = Some("Sound pack library is full (32 packs)".into());
                            }
                        }
                        Ok(WorkerResult::Media { kind, file }) => {
                            app.status = "Media selected".into();
                            app.error = None;
                            app.accept_media(kind, file);
                        }
                        Ok(WorkerResult::Deleted { trash, library }) => {
                            app.project_deleted(trash, library);
                            self.recorded = None;
                        }
                        Ok(WorkerResult::Restored(library)) => {
                            app.library = Some(library);
                            app.files.last_deleted = None;
                            app.error = None;
                            app.status = "Project restored".into();
                            self.recorded = None;
                        }
                        Ok(WorkerResult::Exported {
                            package,
                            version,
                            content,
                        }) => {
                            if version != app.plugin_version() {
                                return;
                            }
                            let Some(package) = app.plugin_before_export(package, content) else {
                                return;
                            };
                            #[cfg(not(any(target_os = "android", target_os = "ios")))]
                            {
                                app.files.export_path = package.name.clone();
                                app.files.export_package = Some(package);
                            }
                            #[cfg(any(target_os = "android", target_os = "ios"))]
                            if let Err(error) = self.pick_export(app, package) {
                                app.error = Some(format!("{error:#}"));
                            }
                        }
                        #[cfg(not(any(target_os = "android", target_os = "ios")))]
                        Ok(WorkerResult::ExportWritten) => {
                            app.status = "Project package exported".into()
                        }
                        Ok(WorkerResult::Startup {
                            library,
                            recovery,
                            error,
                        }) => {
                            app.files.recovery_loading = false;
                            app.library = Some(library);
                            app.files.recovery = recovery;
                            app.error = error;
                            app.status = "Projects loaded".into();
                        }
                        Ok(WorkerResult::Library(library)) => {
                            app.library = Some(library);
                            app.status = "Projects loaded".into();
                            app.error = None;
                        }
                        Ok(WorkerResult::Saved {
                            location,
                            session,
                            revision,
                        }) => {
                            if app.session == session {
                                app.accept_saved_workspace(
                                    location,
                                    app.editor.revision() == revision,
                                );
                            }
                        }
                        Err(error) => {
                            app.line_tools.importing = false;
                            app.files.recovery_loading = false;
                            app.error = Some(error);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    app.files.busy = false;
                    app.files.recovery_loading = false;
                    app.error = Some("File operation interrupted".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        for result in mobile::document_results() {
            if self
                .pending
                .as_ref()
                .is_none_or(|p| p.token != result.token)
            {
                continue;
            }
            let mut pending = self.pending.take().unwrap();
            if let Some(movie) = pending.movie.take() {
                app.files.busy = false;
                if result.canceled || result.error.is_some() {
                    app.video.completed = Some(movie);
                    app.video.open = true;
                    app.video.message = Some("MP4 is ready. Choose where to save it.");
                    app.error = result.error;
                } else {
                    app.status = "MP4 exported".into();
                }
                continue;
            }
            app.files.busy = false;
            if let Some(operation) = pending.provider {
                if result.canceled {
                    app.cancel_plugin_provider_ui();
                } else if pending.artifact.is_some() {
                    let outcome = if let Some(error) = result.error {
                        Err(error)
                    } else if result.canceled {
                        Err("Plugin export canceled".into())
                    } else {
                        Ok(())
                    };
                    app.receive_plugin_provider_write(operation, outcome);
                } else if let Some(path) = result.path {
                    app.start_plugin_provider_source(
                        operation,
                        MediaFile::new(path, result.name, true),
                    );
                } else {
                    app.receive_plugin_provider_source(
                        operation,
                        Err(result
                            .error
                            .unwrap_or_else(|| "File selection canceled".into())),
                    );
                }
                continue;
            }
            if let Some(error) = result.error {
                app.error = Some(error);
            } else if pending.export.is_some() {
                app.status = if result.canceled {
                    "Project export canceled"
                } else {
                    "Project package exported"
                }
                .into();
            } else if let Some(path) = result.path {
                if pending.plugin {
                    app.install_plugin(MediaFile::new(path, result.name, true));
                } else if let Some(workspace) = pending.repository {
                    app.repository_request(
                        workspace,
                        crate::workspace_ui::repository::Action::Import(path, true),
                    );
                } else if let Some(kind) = pending.media {
                    self.media(app, kind, MediaFile::new(path, result.name, true));
                } else {
                    self.import(app, path, result.name, true);
                }
            } else if result.canceled {
                app.status = "File selection canceled".into();
            }
        }
        if app.files.busy {
            return;
        }
        if let Some(request) = app.files.request.take() {
            self.request(app, request);
        }
    }
}
