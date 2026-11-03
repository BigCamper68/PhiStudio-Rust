use super::*;

impl FileManager {
    pub(super) fn provider_request(&mut self, app: &mut Workbench, request: FileRequest) {
        if request
            .provider_operation()
            .is_some_and(|o| !app.provider_operation_current(o))
        {
            return;
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        if let FileRequest::ProviderPick(operation) = &request {
            let result = (|| -> anyhow::Result<()> {
                self.checkpoint(app, false).map_err(anyhow::Error::msg)?;
                self.token += 1;
                mobile::request_media("plugin", self.token)?;
                self.pending = Some(Pending {
                    plugin: false,
                    repository: None,
                    token: self.token,
                    export: None,
                    movie: None,
                    media: None,
                    provider: Some(operation.clone()),
                    artifact: None,
                });
                app.files.busy = true;
                Ok(())
            })();
            if let Err(error) = result {
                app.receive_plugin_provider_source(operation.clone(), Err(error.to_string()));
            }
            return;
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        if let FileRequest::ProviderExport(artifact) = request {
            let operation = artifact.operation().clone();
            let result = self
                .checkpoint(app, false)
                .map_err(anyhow::Error::msg)
                .and_then(|_| {
                    self.token += 1;
                    mobile::request_export_file(artifact.path(), artifact.name(), self.token)
                });
            match result {
                Ok(()) => {
                    self.pending = Some(Pending {
                        plugin: false,
                        repository: None,
                        token: self.token,
                        export: None,
                        movie: None,
                        media: None,
                        provider: Some(operation),
                        artifact: Some(artifact),
                    });
                    app.files.busy = true;
                }
                Err(error) => app.receive_plugin_provider_write(operation, Err(error.to_string())),
            }
            return;
        }
        let root = self.root.clone();
        let snapshot = matches!(request, FileRequest::ProviderSnapshot(_))
            .then(|| app.editor.document().clone());
        let options = app.project_options.clone();
        let assets = app.asset_root().map(ToOwned::to_owned);
        let chart = app.file.clone();
        let (sender, receiver) = mpsc::channel();
        self.worker = Some(receiver);
        app.files.busy = true;
        std::thread::spawn(move || {
            let result = match request {
                FileRequest::ProviderSnapshot(operation) => {
                    let result = (|| -> Result<MediaFile, String> {
                        if !operation.is_current() {
                            return Err("Provider operation was cancelled".into());
                        }
                        if operation.format_bridge() == Some(phistudio_sdk::FormatBridge::Project) {
                            let document =
                                snapshot.ok_or("Provider export snapshot is unavailable")?;
                            let package =
                                phistudio_core::prepare_project_export_with_plugin_data_checked(
                                    &document,
                                    &options,
                                    assets.as_deref(),
                                    chart.as_deref(),
                                    operation.plugin_data_export(),
                                    || operation.is_current(),
                                )
                                .map_err(|e| e.to_string())?;
                            return Ok(MediaFile::from_export(package));
                        }
                        let file = if let Some(root) = &root {
                            let work = root.join("plugins-work");
                            std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
                            tempfile::Builder::new()
                                .prefix("snapshot-")
                                .tempfile_in(work)
                        } else {
                            tempfile::NamedTempFile::new()
                        }
                        .map_err(|e| e.to_string())?;
                        let document = snapshot.ok_or("Provider export snapshot is unavailable")?;
                        let exported = document
                            .with_plugin_data_export(operation.plugin_data_export())
                            .map_err(|e| e.to_string())?;
                        phistudio_core::save_chart(&exported, file.path())
                            .map_err(|e| e.to_string())?;
                        if !operation.is_current() {
                            return Err("Provider operation was cancelled".into());
                        }
                        let (_file, path) = file.keep().map_err(|e| e.to_string())?;
                        Ok(MediaFile::new(path, "chart.rpe.json".into(), true))
                    })();
                    WorkerResult::ProviderSource { operation, result }
                }
                FileRequest::ProviderImport(artifact) => {
                    let operation = artifact.operation().clone();
                    let result = (|| -> Result<ImportedProject, String> {
                        if !operation.is_current() {
                            return Err("Provider operation was cancelled".into());
                        }
                        let package =
                            operation.format_bridge() == Some(phistudio_sdk::FormatBridge::Project);
                        let name = if package {
                            artifact.name().to_owned()
                        } else {
                            phistudio_core::load_chart(artifact.path())
                                .map_err(|e| e.to_string())?
                                .name()
                                .to_owned()
                        };
                        if !operation.is_current() {
                            return Err("Provider operation was cancelled".into());
                        }
                        let root = root.ok_or("Project library is unavailable")?;
                        let project = phistudio_core::import_project_checked(
                            artifact.path(),
                            &root.join("projects"),
                            &name,
                            package,
                            || operation.is_current(),
                        )
                        .map_err(|e| e.to_string())?;
                        if !operation.is_current() {
                            let _ = std::fs::remove_dir_all(project.workspace);
                            return Err("Provider operation was cancelled".into());
                        }
                        Ok(project)
                    })();
                    WorkerResult::ProviderImported { operation, result }
                }
                FileRequest::ProviderResource(artifact) => {
                    let operation = artifact.operation().clone();
                    let kind = if matches!(
                        operation.provider(),
                        phistudio_plugins::Provider::Resource {
                            resource: phistudio_sdk::ResourceKind::Shader,
                            ..
                        }
                    ) {
                        MediaKind::Shader
                    } else {
                        MediaKind::Resource
                    };
                    let result = MediaFile::from_provider(artifact).validate(kind);
                    WorkerResult::ProviderResource {
                        operation,
                        kind,
                        result,
                    }
                }
                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                FileRequest::ProviderWrite { artifact, path } => {
                    let operation = artifact.operation().clone();
                    let result = artifact.write_to(&path);
                    WorkerResult::ProviderWritten { operation, result }
                }
                _ => return,
            };
            let _ = sender.send(Ok(result));
        });
    }
}
