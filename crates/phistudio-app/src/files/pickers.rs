use super::*;

impl FileManager {
    pub(super) fn media(&mut self, app: &mut Workbench, kind: MediaKind, file: MediaFile) {
        let (sender, receiver) = mpsc::channel();
        self.worker = Some(receiver);
        app.files.busy = true;
        app.error = None;
        app.status = kind.checking_message().into();
        std::thread::spawn(move || {
            let result = file
                .validate(kind)
                .map(|file| WorkerResult::Media { kind, file });
            let _ = sender.send(result);
        });
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub(super) fn pick_media(
        &mut self,
        app: &mut Workbench,
        kind: MediaKind,
    ) -> anyhow::Result<()> {
        self.checkpoint(app, false).map_err(anyhow::Error::msg)?;
        self.token += 1;
        mobile::request_media(kind.protocol(), self.token)?;
        self.pending = Some(Pending {
            repository: None,
            token: self.token,
            export: None,
            movie: None,
            media: Some(kind),
        });
        app.files.busy = true;
        Ok(())
    }

    pub(super) fn import(
        &mut self,
        app: &mut Workbench,
        path: PathBuf,
        name: String,
        remove_source: bool,
    ) {
        app.transport.playing = false;
        app.timeline.cancel_gesture();
        let projects = match self
            .root
            .clone()
            .map(Ok)
            .unwrap_or_else(std::env::current_dir)
        {
            Ok(p) => p.join("projects"),
            Err(e) => {
                app.error = Some(e.to_string());
                return;
            }
        };
        let (sender, receiver) = mpsc::channel();
        self.worker = Some(receiver);
        app.files.busy = true;
        app.status = "Importing…".into();
        std::thread::spawn(move || {
            let result = import_project(&path, &projects, &name, true)
                .map(WorkerResult::Imported)
                .map_err(|e| e.to_string());
            if remove_source {
                let _ = std::fs::remove_file(path);
            }
            let _ = sender.send(result);
        });
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub(super) fn pick_export(
        &mut self,
        app: &mut Workbench,
        package: phistudio_core::ProjectExport,
    ) -> anyhow::Result<()> {
        self.checkpoint(app, false).map_err(anyhow::Error::msg)?;
        self.token += 1;
        mobile::request_export_package(package.path(), &package.name, self.token)?;
        self.pending = Some(Pending {
            repository: None,
            token: self.token,
            export: Some(package),
            movie: None,
            media: None,
        });
        app.files.busy = true;
        Ok(())
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub(super) fn pick(&mut self, app: &mut Workbench) -> anyhow::Result<()> {
        // Flush recovery before the external picker can suspend this process.
        self.checkpoint(app, false).map_err(anyhow::Error::msg)?;
        self.token += 1;
        mobile::request_package(self.token)?;
        self.pending = Some(Pending {
            repository: None,
            token: self.token,
            export: None,
            movie: None,
            media: None,
        });
        app.files.busy = true;
        app.transport.playing = false;
        app.timeline.drag = None;
        Ok(())
    }
}
