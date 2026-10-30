//! Private library operations share one file worker and a compact touch window.
mod actions;
mod patterns_panel;
pub(crate) mod repository;
mod repository_panels;
mod resources_panel;
mod versions_panel;
mod window;
mod worker;
use crate::{
    files::FileRequest,
    media::{MediaFile, MediaKind},
    scroll::TouchScroll as _,
    workbench::Workbench,
};
use egui::{RichText, TextureHandle};
use phistudio_core::{
    self as core, Beat, ChartDocument, ChartError, ProjectLocation, ProjectOptions,
    objects::ObjectClipboard, patterns, resources, versions,
};
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum View {
    Patterns,
    Resources,
    Versions(PathBuf),
    Project(PathBuf),
}
pub(crate) struct WorkspaceState {
    #[cfg(test)]
    rect: egui::Rect,
    pub root: Option<PathBuf>,
    pub view: Option<View>,
    pub patterns: Vec<patterns::PatternSummary>,
    pub resources: Vec<resources::Resource>,
    pub loading: bool,
    pub repository: repository::State,
    pub name: String,
    pub selected_pattern: Option<String>,
    pub selected_resource: Option<String>,
    pub confirm_delete: Option<String>,
    pub confirm_restore: bool,
    pub line: usize,
    pub layer: usize,
    pub scale: f64,
    pub offset: [f64; 2],
    pub mirrored: bool,
    pub resource_target: Option<String>,
    pub resource_undo: Option<resources::ResourceUndo>,
    pub inspection: Option<phistudio_prpr::MediaInspection>,
    pub image: Option<TextureHandle>,
    pub audition: Option<Option<phistudio_prpr::ResourceAudioClip>>,
}
impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            #[cfg(test)]
            rect: egui::Rect::NOTHING,
            root: None,
            view: None,
            patterns: vec![],
            resources: vec![],
            loading: false,
            repository: Default::default(),
            name: String::new(),
            selected_pattern: None,
            selected_resource: None,
            confirm_delete: None,
            confirm_restore: false,
            line: 0,
            layer: 0,
            scale: 1.,
            offset: [0.; 2],
            mirrored: false,
            resource_target: None,
            resource_undo: None,
            inspection: None,
            image: None,
            audition: None,
        }
    }
}
#[derive(Clone)]
pub(crate) struct Insertion {
    line: usize,
    layer: usize,
    beat: Beat,
    scale: Beat,
    offset: [f64; 2],
    mirrored: bool,
}
pub(crate) enum WorkspaceJob {
    Repository {
        workspace: PathBuf,
        revision: Option<u64>,
        action: repository::Action,
    },
    Patterns,
    SavePattern {
        name: String,
        clipboard: Arc<ObjectClipboard>,
    },
    Favorite {
        id: String,
        value: bool,
    },
    DeletePattern(String),
    InsertPattern {
        id: String,
        settings: Insertion,
    },
    Resources,
    InspectResource(String),
    ImportResource {
        file: MediaFile,
        replace: Option<String>,
    },
    DeleteResource(String),
    UndoResource(resources::ResourceUndo),
    Rename {
        workspace: PathBuf,
        name: String,
    },
    Duplicate {
        workspace: PathBuf,
        name: String,
    },
}
impl WorkspaceJob {
    pub fn read_only(&self) -> bool {
        matches!(
            self,
            Self::Patterns | Self::Resources | Self::InspectResource(_)
        )
    }
    pub fn needs_document(&self) -> bool {
        matches!(
            self,
            Self::Repository { .. }
                | Self::ImportResource { .. }
                | Self::DeleteResource(_)
                | Self::UndoResource(_)
                | Self::Duplicate { .. }
        )
    }
    pub fn restoring(&self) -> bool {
        matches!(self, Self::Repository { .. })
    }
}
pub(crate) struct WorkspaceContext {
    pub resource_references: Option<std::collections::BTreeMap<String, usize>>,
    pub root: PathBuf,
    pub document: Option<ChartDocument>,
    pub location: Option<ProjectLocation>,
    pub options: ProjectOptions,
    pub session: u64,
}
pub(crate) enum WorkspaceResult {
    Repository {
        workspace: PathBuf,
        status: Box<core::vcs::Status>,
        detail: Option<(String, core::vcs::Difference)>,
        notice: String,
    },
    RepositoryWorking {
        workspace: PathBuf,
        copy: Box<core::vcs::WorkingCopy>,
        status: Box<core::vcs::Status>,
    },
    RepositoryExport(core::ProjectExport),
    Patterns(Vec<patterns::PatternSummary>),
    Insert(ObjectClipboard, Insertion),
    Resources {
        entries: Vec<resources::Resource>,
        undo: Option<resources::ResourceUndo>,
        changed: bool,
    },
    Inspected(phistudio_prpr::MediaInspection),
    Library(core::ProjectLibrary),
}
fn invalid(s: &str) -> ChartError {
    ChartError::Invalid(s.into())
}
type ProjectSnapshot<'a> = (
    Cow<'a, ChartDocument>,
    ProjectLocation,
    ProjectOptions,
    bool,
);

fn core_path(name: &str) -> Result<PathBuf, ChartError> {
    let p = Path::new(name);
    if p.components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(invalid("Invalid resource path"));
    }
    Ok(p.to_owned())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod close_regressions;
