//! Platform pickers exchange paths to bounded private snapshots, never chart
//! JSON over JNI. ZIP decoding and chart discovery run off the rendering thread.
mod edit_time;
mod pickers;
mod providers;
mod requests;
mod update;
#[cfg(target_os = "android")]
use crate::android as mobile;
#[cfg(target_os = "ios")]
use crate::ios as mobile;
use crate::media::{MediaFile, MediaKind};
use crate::workbench::Workbench;
use phistudio_core::{ImportedProject, import_project};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};

#[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(dead_code))]
pub enum FileRequest {
    AfterHook {
        request: Box<FileRequest>,
        approval: crate::plugins::FileApproval,
    },
    ImportSource(MediaFile),
    #[cfg(any(target_os = "android", target_os = "ios"))]
    ReadyExport(phistudio_core::ProjectExport),
    #[cfg(any(target_os = "android", target_os = "ios"))]
    ReadyFileExport(phistudio_core::ProjectExport),
    ProviderPick(phistudio_plugins::Operation),
    ProviderSnapshot(phistudio_plugins::Operation),
    ProviderImport(phistudio_plugins::Artifact),
    ProviderResource(phistudio_plugins::Artifact),
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    ProviderWrite {
        artifact: phistudio_plugins::Artifact,
        path: PathBuf,
    },
    #[cfg(any(target_os = "android", target_os = "ios"))]
    ProviderExport(phistudio_plugins::Artifact),
    PluginPackage,
    ImportRepository(PathBuf),
    Workspace(crate::workspace_ui::WorkspaceJob),
    ImportSkin(MediaFile),
    DeleteSkin(String),
    SoundCatalog {
        root: PathBuf,
        exclude: Option<String>,
        target: crate::inspector::SoundTarget,
    },
    ImportHitSound {
        root: PathBuf,
        file: MediaFile,
        target: crate::inspector::SoundTarget,
    },
    CheckHitSound {
        root: PathBuf,
        reference: String,
        target: crate::inspector::SoundTarget,
    },
    NoteVolume {
        root: PathBuf,
        skin: Option<PathBuf>,
        source: Option<String>,
        kind: phistudio_core::NoteKind,
        volume: f64,
        target: crate::inspector::SoundTarget,
    },
    PrepareNoteCreation(Box<crate::note_creation::Request>),
    Startup,
    Projects,
    CreateProject {
        document: phistudio_core::ChartDocument,
        media: phistudio_core::NewProjectMedia,
    },
    Media(MediaKind),
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    MediaPath {
        kind: MediaKind,
        path: PathBuf,
    },
    TrashProject {
        workspace: PathBuf,
        name: String,
    },
    UndoDelete(phistudio_core::TrashedProject),
    #[cfg(any(target_os = "android", target_os = "ios"))]
    SaveMovie,
    ExportCurrent,
    ExportProject(PathBuf),
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    WriteExport(PathBuf),
    OpenProject(PathBuf),
    SaveWorkspace,
    RestoreRecovery,
    DiscardRecovery,
    DiscardAndOpen(PathBuf),
    Package,
    ImportPath {
        path: PathBuf,
        name: String,
    },
}

impl FileRequest {
    pub(crate) fn provider_operation(&self) -> Option<&phistudio_plugins::Operation> {
        match self {
            Self::ProviderPick(operation) | Self::ProviderSnapshot(operation) => Some(operation),
            Self::ProviderImport(artifact) | Self::ProviderResource(artifact) => {
                Some(artifact.operation())
            }
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            Self::ProviderWrite { artifact, .. } => Some(artifact.operation()),
            #[cfg(any(target_os = "android", target_os = "ios"))]
            Self::ProviderExport(artifact) => Some(artifact.operation()),
            _ => None,
        }
    }
}

enum WorkerResult {
    SoundPack(phistudio_core::EditorSoundPack),
    ProviderResource {
        operation: phistudio_plugins::Operation,
        kind: MediaKind,
        result: Result<MediaFile, String>,
    },
    ProviderSource {
        operation: phistudio_plugins::Operation,
        result: Result<MediaFile, String>,
    },
    ProviderImported {
        operation: phistudio_plugins::Operation,
        result: Result<ImportedProject, String>,
    },
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    ProviderWritten {
        operation: phistudio_plugins::Operation,
        result: Result<(), String>,
    },
    Workspace {
        session: u64,
        result: crate::workspace_ui::WorkspaceResult,
    },
    Skins {
        entries: Vec<phistudio_core::skins::SkinEntry>,
        selected: Option<String>,
    },
    SoundCatalog {
        catalog: Vec<String>,
        target: crate::inspector::SoundTarget,
    },
    Sound {
        reference: String,
        target: crate::inspector::SoundTarget,
    },
    NoteVolume {
        gain: phistudio_core::NoteSoundGain,
        target: crate::inspector::SoundTarget,
    },
    PreparedNoteCreation {
        target: Box<crate::note_creation::Target>,
        result: Result<crate::note_creation::Prepared, String>,
    },
    Imported(ImportedProject),
    Created(phistudio_core::ProjectLocation),
    Media {
        kind: MediaKind,
        file: MediaFile,
    },
    Deleted {
        trash: phistudio_core::TrashedProject,
        library: phistudio_core::ProjectLibrary,
    },
    Restored(phistudio_core::ProjectLibrary),
    Exported {
        package: phistudio_core::ProjectExport,
        version: phistudio_sdk::DocumentVersion,
        content: phistudio_sdk::FileContent,
    },
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    ExportWritten {
        templates: bool,
    },
    Library(phistudio_core::ProjectLibrary),
    Startup {
        library: phistudio_core::ProjectLibrary,
        recovery: Option<phistudio_core::RecoverySnapshot>,
        error: Option<String>,
    },
    Saved {
        location: phistudio_core::ProjectLocation,
        session: u64,
        revision: u64,
    },
}
struct WorkspaceQuery {
    session: u64,
    view: Option<crate::workspace_ui::View>,
    result: Receiver<Result<crate::workspace_ui::WorkspaceResult, String>>,
}
pub struct FileManager {
    query: Option<WorkspaceQuery>,
    root: Option<PathBuf>,
    checkpoints: crate::persistence::Checkpoints,
    edit_time: edit_time::EditTime,
    preferences: crate::settings::Preferences,
    skins_loading: Option<Receiver<Result<Vec<phistudio_core::skins::SkinEntry>, String>>>,
    worker: Option<Receiver<Result<WorkerResult, String>>>,
    #[cfg(any(target_os = "android", target_os = "ios"))]
    token: u64,
    #[cfg(any(target_os = "android", target_os = "ios"))]
    pending: Option<Pending>,
    recorded: Option<(u64, u64, PathBuf, Option<PathBuf>)>,
}
#[cfg(any(target_os = "android", target_os = "ios"))]
struct Pending {
    provider: Option<phistudio_plugins::Operation>,
    artifact: Option<phistudio_plugins::Artifact>,
    plugin: bool,
    repository: Option<PathBuf>,
    token: u64,
    export: Option<phistudio_core::ProjectExport>,
    movie: Option<crate::video_ui::MovieFile>,
    media: Option<MediaKind>,
}

impl FileManager {
    pub fn new(root: Option<PathBuf>) -> Self {
        let skins_loading = root.clone().map(|root| {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                #[cfg(target_os = "android")]
                crate::video_ui::clear_stale_movies(&root);
                let _ = tx.send(
                    phistudio_core::skins::list(&root.join("skins")).map_err(|e| e.to_string()),
                );
            });
            rx
        });
        Self {
            query: None,
            skins_loading,
            checkpoints: crate::persistence::Checkpoints::new(root.clone()),
            edit_time: edit_time::EditTime::new(root.clone()),
            preferences: crate::settings::Preferences::new(root.clone()),
            root,
            worker: None,
            recorded: None,
            #[cfg(any(target_os = "android", target_os = "ios"))]
            token: 0,
            #[cfg(any(target_os = "android", target_os = "ios"))]
            pending: None,
        }
    }
    pub fn checkpoint(&mut self, app: &Workbench, discard: bool) -> Result<(), String> {
        self.edit_time.flush(app)?;
        self.preferences.flush(app)?;
        self.checkpoints.flush(app, discard)?;
        app.presets.flush()?;
        app.curve_templates.flush()?;
        app.clipboard_history.flush()
    }
}

fn library_after_change(root: &std::path::Path) -> phistudio_core::ProjectLibrary {
    phistudio_core::list_projects(root).unwrap_or_else(|error| phistudio_core::ProjectLibrary {
        projects: Vec::new(),
        issues: vec![error.to_string()],
    })
}

#[cfg(test)]
mod tests;
