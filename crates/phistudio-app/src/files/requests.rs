use super::*;

impl FileManager {
    pub(super) fn request(&mut self, app: &mut Workbench, request: FileRequest) {
        match request {
            FileRequest::ImportRepository(workspace) => {
                #[cfg(any(target_os = "android", target_os = "ios"))]
                {
                    if let Err(error) = self.pick(app) {
                        app.error = Some(format!("{error:#}"));
                    } else if let Some(pending) = &mut self.pending {
                        pending.repository = Some(workspace);
                    }
                }
                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                {
                    let path = PathBuf::from(&app.workspace.repository.import_path);
                    app.repository_request(
                        workspace,
                        crate::workspace_ui::repository::Action::Import(path, false),
                    );
                }
            }
            #[cfg(any(target_os = "android", target_os = "ios"))]
            FileRequest::SaveMovie => {
                let Some(movie) = app.video.completed.take() else {
                    return;
                };
                self.token += 1;
                let result = self
                    .checkpoint(app, false)
                    .map_err(anyhow::Error::msg)
                    .and_then(|_| {
                        mobile::request_export_video(&movie.path(), &movie.name, self.token)
                    });
                match result {
                    Ok(()) => {
                        self.pending = Some(Pending {
                            repository: None,
                            token: self.token,
                            export: None,
                            movie: Some(movie),
                            media: None,
                        });
                        app.video.open = false;
                        app.files.busy = true;
                    }
                    Err(error) => {
                        app.video.completed = Some(movie);
                        app.error = Some(format!("Could not save MP4: {error:#}"));
                    }
                }
            }
            FileRequest::Workspace(job) => {
                let Some(root) = self.root.clone() else {
                    app.error = Some("Open the private project library first".into());
                    app.workspace.loading = false;
                    return;
                };
                if job.restoring()
                    && let Err(error) = self.checkpoint(app, false)
                {
                    app.error = Some(error);
                    return;
                }
                let resource_references =
                    if matches!(job, crate::workspace_ui::WorkspaceJob::Resources) {
                        match phistudio_core::resources::references(
                            app.editor.document(),
                            &app.project_options,
                        ) {
                            Ok(refs) => Some(refs),
                            Err(error) => {
                                app.workspace.loading = false;
                                app.error = Some(error.to_string());
                                return;
                            }
                        }
                    } else {
                        None
                    };
                let context = crate::workspace_ui::WorkspaceContext {
                    resource_references,
                    root,
                    document: job.needs_document().then(|| app.editor.document().clone()),
                    location: app
                        .file
                        .clone()
                        .map(|chart| phistudio_core::ProjectLocation {
                            chart,
                            assets: app.asset_root().map(std::path::Path::to_owned),
                        }),
                    options: app.project_options.clone(),
                    session: app.session,
                };
                if job.read_only() {
                    let (sender, result) = mpsc::channel();
                    self.query = Some(WorkspaceQuery {
                        session: app.session,
                        view: app.workspace.view.clone(),
                        result,
                    });
                    app.workspace.loading = true;
                    std::thread::spawn(move || {
                        let _ = sender.send(
                            context
                                .run(job)
                                .map(|(_, result)| result)
                                .map_err(|e| e.to_string()),
                        );
                    });
                    return;
                }
                let (sender, receiver) = mpsc::channel();
                self.worker = Some(receiver);
                app.files.busy = true;
                app.status = "Working…".into();
                std::thread::spawn(move || {
                    let result = context.run(job).map(|(session, result)| match result {
                        crate::workspace_ui::WorkspaceResult::RepositoryExport(package) => {
                            WorkerResult::Exported(package)
                        }
                        result => WorkerResult::Workspace { session, result },
                    });
                    let _ = sender.send(result);
                });
            }
            FileRequest::ImportSkin(_) | FileRequest::DeleteSkin(_) => {
                let Some(root) = app.skins.root.clone() else {
                    app.error = Some("Open the project library before managing skins".into());
                    return;
                };
                let (sender, receiver) = mpsc::channel();
                self.worker = Some(receiver);
                app.files.busy = true;
                std::thread::spawn(move || {
                    let result = (|| -> Result<WorkerResult, String> {
                        let selected = match request {
                            FileRequest::ImportSkin(file) => Some(
                                phistudio_core::skins::import(file.path(), &root, |path| {
                                    phistudio_prpr::inspect_skin(path).map_err(|e| {
                                        phistudio_core::ChartError::Invalid(format!("{e:#}"))
                                    })
                                })
                                .map_err(|e| e.to_string())?
                                .id,
                            ),
                            FileRequest::DeleteSkin(id) => {
                                phistudio_core::skins::delete(&root, &id)
                                    .map_err(|e| e.to_string())?;
                                None
                            }
                            _ => None,
                        };
                        Ok(WorkerResult::Skins {
                            entries: phistudio_core::skins::list(&root)
                                .map_err(|e| e.to_string())?,
                            selected,
                        })
                    })();
                    let _ = sender.send(result);
                });
            }
            FileRequest::SoundCatalog { .. }
            | FileRequest::ImportHitSound { .. }
            | FileRequest::NoteVolume { .. }
            | FileRequest::CheckHitSound { .. } => {
                let (sender, receiver) = mpsc::channel();
                self.worker = Some(receiver);
                app.files.busy = true;
                std::thread::spawn(move || {
                    let result = (|| -> Result<WorkerResult, String> {
                        match request {
                            FileRequest::NoteVolume {
                                root,
                                skin,
                                source,
                                kind,
                                volume,
                                target,
                            } => {
                                let wav = pollster::block_on(phistudio_prpr::render_hitsound_gain(
                                    &root,
                                    skin.as_deref(),
                                    source.as_deref(),
                                    kind,
                                    volume,
                                ))
                                .map_err(|e| format!("{e:#}"))?;
                                let rendered =
                                    phistudio_core::write_generated_hitsound(&root, &wav)
                                        .map_err(|e| e.to_string())?;
                                Ok(WorkerResult::NoteVolume {
                                    gain: phistudio_core::NoteSoundGain {
                                        source,
                                        rendered,
                                        volume,
                                    },
                                    target,
                                })
                            }
                            FileRequest::SoundCatalog {
                                root,
                                exclude,
                                target,
                            } => Ok(WorkerResult::SoundCatalog {
                                catalog: phistudio_core::list_hitsound_assets(
                                    &root,
                                    exclude.as_deref(),
                                )
                                .map_err(|e| e.to_string())?,
                                target,
                            }),
                            FileRequest::ImportHitSound { root, file, target } => {
                                let reference = phistudio_core::import_hitsound_asset(
                                    &root,
                                    file.path(),
                                    file.name(),
                                )
                                .map_err(|e| e.to_string())?;
                                Ok(WorkerResult::Sound { reference, target })
                            }
                            FileRequest::CheckHitSound {
                                root,
                                reference,
                                target,
                            } => {
                                let path = phistudio_core::hitsound_path(&root, &reference)
                                    .map_err(|e| e.to_string())?;
                                MediaFile::new(path, reference.clone(), false)
                                    .validate(MediaKind::HitSound)?;
                                Ok(WorkerResult::Sound { reference, target })
                            }
                            _ => unreachable!(),
                        }
                    })();
                    let _ = sender.send(result);
                });
            }

            FileRequest::RestoreRecovery => app.restore_recovery(),
            FileRequest::DiscardAndOpen(path) => match self.checkpoint(app, true) {
                Ok(()) => app.discard_and_open(path),
                Err(error) => app.error = Some(error),
            },
            FileRequest::DiscardRecovery => match self.checkpoint(app, true) {
                Ok(()) => {
                    app.files.recovery = None;
                    app.status = "Saved project kept; recovery discarded".into();
                }
                Err(error) => app.error = Some(error),
            },
            FileRequest::Startup
            | FileRequest::Projects
            | FileRequest::OpenProject(_)
            | FileRequest::SaveWorkspace
            | FileRequest::CreateProject { .. }
            | FileRequest::TrashProject { .. }
            | FileRequest::UndoDelete(_) => {
                let Some(root) = self.root.clone() else {
                    app.error =
                        Some("Use --workspace DIR to enable the project library on desktop".into());
                    return;
                };
                if let FileRequest::TrashProject { workspace, .. } = &request
                    && app.file.as_ref().is_some_and(|p| p.starts_with(workspace))
                    && let Err(error) = self.checkpoint(app, true)
                {
                    app.error = Some(error);
                    return;
                }
                let snapshot = matches!(request, FileRequest::SaveWorkspace)
                    .then(|| app.editor.document().clone());
                let session = app.session;
                let revision = app.editor.revision();
                app.transport.playing = false;
                app.timeline.cancel_gesture();
                app.files.busy = true;
                app.status = "Loading projects…".into();
                let (sender, receiver) = mpsc::channel();
                self.worker = Some(receiver);
                std::thread::spawn(move || {
                    let result = match request {
                        FileRequest::Startup => {
                            let (recovery, error) = match phistudio_core::load_recovery(&root) {
                                Ok(snapshot) => (snapshot, None),
                                Err(error) => {
                                    (None, Some(format!("Could not read recovery: {error}")))
                                }
                            };
                            phistudio_core::list_projects(&root).map(|library| {
                                WorkerResult::Startup {
                                    library,
                                    recovery,
                                    error,
                                }
                            })
                        }
                        FileRequest::Projects => {
                            phistudio_core::list_projects(&root).map(WorkerResult::Library)
                        }
                        FileRequest::OpenProject(workspace) => {
                            phistudio_core::open_project(&root, &workspace)
                                .map(WorkerResult::Imported)
                        }
                        FileRequest::CreateProject { document, media } => {
                            phistudio_core::create_project_with_media(&root, &document, &media)
                                .map(WorkerResult::Created)
                        }
                        FileRequest::TrashProject { workspace, name } => {
                            phistudio_core::trash_project(&root, &workspace, &name).map(|trash| {
                                WorkerResult::Deleted {
                                    trash,
                                    library: library_after_change(&root),
                                }
                            })
                        }
                        FileRequest::UndoDelete(trash) => {
                            phistudio_core::restore_project(&root, &trash)
                                .map(|_| WorkerResult::Restored(library_after_change(&root)))
                        }
                        FileRequest::SaveWorkspace => {
                            phistudio_core::create_project(&root, &snapshot.unwrap()).map(
                                |location| WorkerResult::Saved {
                                    location,
                                    session,
                                    revision,
                                },
                            )
                        }
                        _ => unreachable!(),
                    }
                    .map_err(|e| e.to_string());
                    let _ = sender.send(result);
                });
            }
            FileRequest::ExportCurrent | FileRequest::ExportProject(_) => {
                let snapshot = app.editor.document().clone();
                let options = app.project_options.clone();
                let chart = app.file.clone();
                let assets = app.asset_root().map(ToOwned::to_owned);
                let root = self.root.clone();
                let current = match &request {
                    FileRequest::ExportProject(workspace) => {
                        chart.as_ref().is_some_and(|p| p.starts_with(workspace))
                    }
                    _ => true,
                };
                let (sender, receiver) = mpsc::channel();
                self.worker = Some(receiver);
                app.files.busy = true;
                app.transport.playing = false;
                app.timeline.input.cancel();
                app.status = "Preparing project export…".into();
                std::thread::spawn(move || {
                    let result = (|| {
                        if current {
                            return phistudio_core::prepare_project_export(
                                &snapshot,
                                &options,
                                assets.as_deref(),
                                chart.as_deref(),
                            );
                        }
                        let FileRequest::ExportProject(workspace) = request else {
                            unreachable!()
                        };
                        let root = root.ok_or_else(|| {
                            phistudio_core::ChartError::Invalid(
                                "Project library is unavailable".into(),
                            )
                        })?;
                        let project = phistudio_core::open_project(&root, &workspace)?;
                        let selected = project.suggested.ok_or_else(|| {
                            phistudio_core::ChartError::Invalid(
                                "Open this project and select a chart before exporting".into(),
                            )
                        })?;
                        let chart = &project.charts[selected].path;
                        let document = phistudio_core::load_chart(chart)?;
                        let options = phistudio_core::load_project_options_with_assets(
                            chart,
                            Some(&project.assets),
                        )?;
                        phistudio_core::prepare_project_export(
                            &document,
                            &options,
                            Some(&project.assets),
                            Some(chart),
                        )
                    })()
                    .map(WorkerResult::Exported)
                    .map_err(|e| e.to_string());
                    let _ = sender.send(result);
                });
            }
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            FileRequest::WriteExport(path) => {
                if let Some(package) = app.files.export_package.take() {
                    let (sender, receiver) = mpsc::channel();
                    self.worker = Some(receiver);
                    app.files.busy = true;
                    std::thread::spawn(move || {
                        let _ = sender.send(
                            package
                                .write_to(&path)
                                .map(|()| WorkerResult::ExportWritten)
                                .map_err(|e| e.to_string()),
                        );
                    });
                }
            }
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            FileRequest::MediaPath { kind, path } => {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                self.media(app, kind, MediaFile::new(path, name, false));
            }
            #[cfg(any(target_os = "android", target_os = "ios"))]
            FileRequest::Media(kind) => {
                if let Err(error) = self.pick_media(app, kind) {
                    app.files.busy = false;
                    app.error = Some(format!("{error:#}"));
                }
            }
            FileRequest::ImportPath { path, name } => self.import(app, path, name, false),
            #[cfg(any(target_os = "android", target_os = "ios"))]
            FileRequest::Package => {
                if let Err(error) = self.pick(app) {
                    app.files.busy = false;
                    app.error = Some(format!("{error:#}"));
                }
            }
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            _ => {}
        }
    }
}
