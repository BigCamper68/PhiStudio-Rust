mod dialogs;
mod home;
use super::Workbench;
use crate::media::{MediaFile, MediaKind};
use crate::scroll::TouchScroll as _;
use crate::{files::FileRequest, theme::*};
use egui::{Align2, RichText};
use phistudio_core::ChartDocument;

pub(crate) struct NewChart {
    name: String,
    bpm: f64,
    level: String,
    composer: String,
    charter: String,
    audio: Option<MediaFile>,
    illustration: Option<MediaFile>,
}
impl Default for NewChart {
    fn default() -> Self {
        Self {
            name: "Untitled".into(),
            bpm: 120.,
            level: String::new(),
            composer: String::new(),
            charter: String::new(),
            audio: None,
            illustration: None,
        }
    }
}
impl NewChart {
    pub(crate) fn ready(&self) -> bool {
        !self.name.trim().is_empty()
            && self.bpm.is_finite()
            && self.bpm > 0.
            && self.audio.is_some()
            && self.illustration.is_some()
    }
    fn media(&self) -> Option<phistudio_core::NewProjectMedia> {
        Some(phistudio_core::NewProjectMedia {
            audio: self.audio.as_ref()?.path().to_owned(),
            audio_name: self.audio.as_ref()?.name().into(),
            illustration: self.illustration.as_ref()?.path().to_owned(),
            illustration_name: self.illustration.as_ref()?.name().into(),
        })
    }
}
impl Workbench {
    pub(crate) fn accept_media(&mut self, kind: MediaKind, file: MediaFile) {
        if let MediaKind::EditorSound(event) = kind {
            self.accept_editor_sound(event, file);
            return;
        }
        if kind == MediaKind::CurveTemplates {
            if let Err(error) = self.curve_templates.import(file) {
                self.error = Some(error);
            }
            return;
        }
        if kind == MediaKind::Shader {
            self.accept_shader_file(file);
            return;
        }
        if kind == MediaKind::Resource || kind == MediaKind::LineTexture {
            self.line_tools.importing = kind == MediaKind::LineTexture;
            self.accept_resource_file(file);
            return;
        }
        if kind == MediaKind::Skin {
            self.accept_skin_file(file);
            return;
        }
        if kind == MediaKind::HitSound {
            self.accept_hitsound(file);
            return;
        }
        if let Some(draft) = &mut self.files.new_chart {
            match kind {
                MediaKind::SoundPack
                | MediaKind::CurveTemplates
                | MediaKind::EditorSound(_)
                | MediaKind::Shader
                | MediaKind::Resource
                | MediaKind::LineTexture
                | MediaKind::HitSound
                | MediaKind::Skin => {
                    unreachable!()
                }
                MediaKind::Audio => draft.audio = Some(file),
                MediaKind::Illustration => draft.illustration = Some(file),
            }
        }
    }
    pub(crate) fn open_media_picker(&mut self, kind: MediaKind) {
        self.transport.playing = false;
        self.error = None;
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            self.files.request = Some(FileRequest::Media(kind));
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            self.files.media_path.clear();
            self.files.media = Some(kind);
        }
    }
    pub(crate) fn project_deleted(
        &mut self,
        trash: phistudio_core::TrashedProject,
        library: phistudio_core::ProjectLibrary,
    ) {
        if self
            .file
            .as_ref()
            .is_some_and(|path| path.starts_with(&trash.original))
        {
            self.replace_document(
                ChartDocument::empty("Untitled", 120.).expect("valid blank project"),
                None,
                None,
            );
        }
        self.files.delete_project = None;
        self.files.last_deleted = Some(trash);
        self.library = Some(library);
        self.show_projects = true;
        self.error = None;
        self.status = "Project moved to Trash".into();
    }
    pub fn open_projects(&mut self) {
        if !self.flush_shader_draft() {
            return;
        }
        self.transport.playing = false;
        self.transport.scrubbing = false;
        self.timeline.input.cancel();
        self.show_projects = true;
        self.files.request = Some(FileRequest::Projects);
    }
    pub fn open_created_project(&mut self, location: phistudio_core::ProjectLocation) {
        self.files.new_chart = None;
        self.show_projects = false;
        self.files.pending_asset_directory = location.assets;
        self.open(location.chart);
    }
}

#[cfg(test)]
mod tests;
