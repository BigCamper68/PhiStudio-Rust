//! Import into a new private workspace before switching the active editor.
//! Chart entries share the document size limit. No archive member can escape its workspace.
mod paths;
use paths::{common_directory, safe_inside};
pub(crate) use paths::{inventory, private_vcs_path, relative};
mod import;
pub use import::import_project;

mod metadata;
use metadata::{described_project, read_description};
pub(crate) use metadata::{export_chart_path, inspect_workspace, workspace_source_name};
pub use metadata::{load_project_options, load_project_options_with_assets};
mod generation;
use crate::{
    ChartDocument, ChartError, ChartFormat, ProjectOptions, decode_chart, load_chart, save_chart,
};

pub(crate) use generation::{
    active_root, describe_created_project, generation_description, publish_generation,
};
use serde::{Deserialize, Serialize};
#[path = "../package_manifest.rs"]
mod manifest;
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

const MIB: u64 = 1024 * 1024;
#[derive(Debug)]
pub struct ChartChoice {
    pub path: PathBuf,
    pub name: String,
    pub notes: usize,
    pub format: ChartFormat,
    pub source: PathBuf,
}
#[derive(Debug)]
pub struct ImportedProject {
    pub workspace: PathBuf,
    pub assets: PathBuf,
    pub name: String,
    pub charts: Vec<ChartChoice>,
    pub suggested: Option<usize>,
}
fn invalid(message: impl Into<String>) -> ChartError {
    ChartError::Invalid(message.into())
}

const DESCRIPTION: &str = ".phistudio-workspace.json";
#[derive(Serialize, Deserialize)]
struct Description {
    phistudio_workspace: u32,
    source_name: String,
    assets: PathBuf,
    charts: Vec<StoredChart>,
    suggested: Option<usize>,
}
#[derive(Serialize, Deserialize)]
struct StoredChart {
    source: PathBuf,
    working: PathBuf,
    format: ChartFormat,
    options: ProjectOptions,
}

// Preserve the selected source chart's path when it already uses RPE JSON.
