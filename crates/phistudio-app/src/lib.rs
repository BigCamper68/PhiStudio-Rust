mod runtime;
use runtime::launch;
mod advanced_ui;
mod audio_guide;
mod audio_notes_ui;
mod authoring_ui;
mod color_ui;
mod covers;
mod editor_sounds;
mod event_grid;
mod files;
mod fonts;
mod form;
mod grid_input;
mod gui;
mod hitsound_ui;
mod i18n;
mod inspector;
#[cfg(test)]
mod layout_regressions;
mod line_activity;
mod line_tools;
mod media;
mod noise_ui;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[doc(hidden)]
pub mod note_grid_probe;
mod note_index;
mod numeric;
mod persistence;
mod picking;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod playback_smoke;
mod playtest_ui;
mod plot;
mod plugins;
mod preview_ui;
mod scroll;
mod secure_store;
mod settings;
mod settings_ui;
mod shader_ui;
mod shortcuts_ui;
mod skins_ui;
mod speed_curve_ui;
mod story_grid;
mod story_tools;
mod theme;
mod timeline;
mod toolkit;
mod touch;
mod trajectory_ui;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[doc(hidden)]
pub mod ui_layout_probe;
mod updates;
mod video_encoder;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[doc(hidden)]
pub mod video_export_probe;
mod video_ui;
mod window;
mod workbench;
mod workspace_ui;

use anyhow::{Context, Result, ensure};
use macroquad::prelude::*;
use phistudio_core::{Beat, ChartDocument, Command, Note, NoteKind, load_chart, save_chart};
use phistudio_prpr::Preview;
use std::path::PathBuf;
use workbench::Workbench;

const DEMO: &str = include_str!("../../../assets/charts/phistudio-demo.rpe.json");

#[derive(Default)]
struct Options {
    chart: Option<PathBuf>,
    package: Option<PathBuf>,
    smoke: Option<PathBuf>,
    playback_smoke: Option<PathBuf>,
    size: Option<(i32, i32)>,
    workspace: Option<PathBuf>,
}

#[cfg(target_os = "ios")]
mod ios;

#[cfg(target_os = "ios")]
pub fn ios_main() {
    launch(Options::default());
}

pub fn desktop_main() {
    let options = match Options::parse() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e:#}");
            std::process::exit(2);
        }
    };
    launch(options);
}

#[cfg(target_os = "android")]
mod android;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn quad_main() {
    launch(Options::default());
}

mod tool_presets_ui;

mod clipboard_ui;

mod selection_scaling;

mod text_composition_ui;

mod multi_line;

#[cfg(test)]
mod multi_line_tests;
