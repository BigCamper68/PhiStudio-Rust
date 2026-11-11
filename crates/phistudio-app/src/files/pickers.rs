use super::*;

impl FileManager {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub(super) fn pick_plugin(&mut self, app: &mut Workbench) -> anyhow::Result<()> {
        self.pick(app)?;
        if let Some(pending) = &mut self.pending {
            pending.plugin = true;
        }
        Ok(())
    }
    pub(super) fn media(&mut self, app: &mut Workbench, kind: MediaKind, file: MediaFile) {
        if kind == MediaKind::CurveTemplates {
            app.status = "Importing curve templates…".into();
            app.error = None;
            if let Err(error) = app.curve_templates.import(file) {
                app.error = Some(error);
            }
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.worker = Some(receiver);
        app.files.busy = true;
        app.error = None;
        app.status = kind.checking_message().into();
        let sound_root = self.root.clone();
        std::thread::spawn(move || {
            let result = if kind == MediaKind::SoundPack {
                sound_library(sound_root).and_then(|root| {
                    crate::editor_sounds::archive::import(file.path(), &root)
                        .map(WorkerResult::SoundPack)
                        .map_err(|e| format!("{e:#}"))
                })
            } else {
                if matches!(kind, MediaKind::EditorSound(_)) {
                    import_editor_sound(sound_root, file)
                } else {
                    file.validate(kind)
                }
                .map(|file| WorkerResult::Media { kind, file })
            };
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
            provider: None,
            artifact: None,
            plugin: false,
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
        self.request(
            app,
            FileRequest::ImportSource(MediaFile::new(path, name, remove_source)),
        );
    }
    pub(super) fn import_now(&mut self, app: &mut Workbench, source: MediaFile) {
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
            let result = import_project(source.path(), &projects, source.name(), true)
                .map(WorkerResult::Imported)
                .map_err(|e| e.to_string());
            drop(source);
            let _ = sender.send(result);
        });
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub(super) fn pick_export(
        &mut self,
        app: &mut Workbench,
        package: phistudio_core::ProjectExport,
    ) -> anyhow::Result<()> {
        self.pick_export_kind(app, package, false)
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub(super) fn pick_export_kind(
        &mut self,
        app: &mut Workbench,
        package: phistudio_core::ProjectExport,
        generic: bool,
    ) -> anyhow::Result<()> {
        self.checkpoint(app, false).map_err(anyhow::Error::msg)?;
        self.token += 1;
        if generic {
            mobile::request_export_file(package.path(), &package.name, self.token)?;
        } else {
            mobile::request_export_package(package.path(), &package.name, self.token)?;
        }
        self.pending = Some(Pending {
            provider: None,
            artifact: None,
            plugin: false,
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
            provider: None,
            artifact: None,
            plugin: false,
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

fn sound_library(root: Option<PathBuf>) -> Result<PathBuf, String> {
    root.map(Ok)
        .unwrap_or_else(|| {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                crate::runtime::desktop_workspace().map_err(|e| e.to_string())
            }
            #[cfg(any(target_os = "android", target_os = "ios"))]
            {
                std::env::current_dir().map_err(|e| e.to_string())
            }
        })
        .map(|root| root.join("editor-sounds"))
}

// Keep imported feedback independent of project lifetime and mobile picker snapshots.
fn import_editor_sound(root: Option<PathBuf>, file: MediaFile) -> Result<MediaFile, String> {
    let root = sound_library(root)?;
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let name = file
        .name()
        .chars()
        .take(48)
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let prefix = format!("{name}-");
    let mut copy = tempfile::Builder::new()
        .prefix(&prefix)
        .tempfile_in(&root)
        .map_err(|e| e.to_string())?;
    let mut source = std::fs::File::open(file.path()).map_err(|e| e.to_string())?;
    use std::io::Read;
    let size = std::io::copy(&mut source.by_ref().take(2 * 1024 * 1024 + 1), &mut copy)
        .map_err(|e| e.to_string())?;
    if size > 2 * 1024 * 1024 {
        return Err("Editor sound exceeds 2 MiB".into());
    }
    phistudio_prpr::load_interface_sound(copy.path())
        .map_err(|e| format!("Could not load editor sound: {e:#}"))?;
    copy.as_file().sync_all().map_err(|e| e.to_string())?;
    let (_, path) = copy.keep().map_err(|e| e.to_string())?;
    Ok(MediaFile::new(path, file.name().into(), false))
}

#[cfg(test)]
mod sound_tests {
    use super::*;
    #[test]
    fn imported_sound_outlives_the_owned_picker_snapshot_and_failed_imports_leave_no_copy() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("picker");
        let bytes = crate::media::tests::wav_bytes();
        std::fs::write(&source, &bytes).unwrap();
        let imported = import_editor_sound(
            Some(root.path().to_owned()),
            MediaFile::new(source.clone(), "click.wav".into(), true),
        )
        .unwrap();
        assert!(!source.exists());
        let path = imported.path().to_owned();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        drop(imported);
        assert!(path.exists());
        std::fs::write(&source, b"invalid audio").unwrap();
        assert!(
            import_editor_sound(
                Some(root.path().to_owned()),
                MediaFile::new(source, "bad.wav".into(), true)
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read_dir(root.path().join("editor-sounds"))
                .unwrap()
                .count(),
            1
        );
    }
}
