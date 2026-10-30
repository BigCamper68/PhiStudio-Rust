//! Explicit, staged authoring from passive markers; analysis remains reusable.
mod audio;
mod controls;
mod worker;
use crate::{
    scroll::{TouchCombo as _, TouchScroll as _},
    workbench::Workbench,
};
use phistudio_core::{
    Beat,
    tools::{
        AudioMarker, AudioNoteOptions, AudioNotePreview, AudioNoteSpec, insert_audio_notes,
        preview_audio_notes,
    },
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Range {
    #[default]
    Song,
    Cursor,
    Visible,
    Custom,
}
#[derive(Clone, PartialEq)]
struct Key {
    session: u64,
    revision: u64,
    assets: u64,
    source: (u64, u64),
    spec: AudioNoteSpec,
}
struct Ghosts {
    notes: Vec<(usize, phistudio_core::Note)>,
    index: crate::note_index::NoteIndex,
}
struct Prepared {
    plan: AudioNotePreview,
    ghosts: BTreeMap<usize, Ghosts>,
    audio: audio::Audio,
}
struct Pending {
    id: u64,
    key: Key,
    cancel: Arc<AtomicBool>,
}
#[derive(Default)]
pub(crate) struct State {
    pub show: bool,
    range: Range,
    start: String,
    end: String,
    line: usize,
    pub(crate) multiple: bool,
    pub(crate) hide_ghosts: bool,
    worker: Option<worker::Planner>,
    pending: Option<Pending>,
    next_id: u64,
    preview: Option<(Key, Prepared)>,
    error: Option<String>,
}
impl State {
    pub fn close(&mut self) {
        if let Some(job) = self.pending.take() {
            job.cancel.store(true, Ordering::Release);
        }
        self.preview = None;
        self.show = false;
        self.error = None;
    }
    pub fn clear_project(&mut self) {
        self.close();
        self.range = Range::Song;
        self.line = 0;
        self.multiple = false;
        self.start.clear();
        self.end.clear();
        self.hide_ghosts = false;
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.close();
    }
}
impl Workbench {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub(crate) fn audio_note_report(&self) -> serde_json::Value {
        let valid = self
            .audio_notes
            .preview
            .as_ref()
            .is_some_and(|(key, _)| self.audio_note_key().as_ref().ok() == Some(key));
        let preview = self.audio_notes.preview.as_ref().map(|(_, p)| &p.plan);
        serde_json::json!({"valid":valid,"pending":self.audio_notes.pending.is_some(),"count":preview.map_or(0,|p|p.notes.len()),"stats":preview.map(|p|&p.stats),"notes":preview.map(|p|p.notes.iter().map(|n|serde_json::json!([n.line,n.note.start_time.as_f64(),n.note.end_time.as_f64(),n.note.x,n.note.kind as u8,n.note.above])).collect::<Vec<_>>()),"error":self.audio_notes.error})
    }
    pub(crate) fn open_audio_notes(&mut self) {
        self.audio_notes.show = true;
        self.audio_notes.line = self.line;
        self.audio_notes.error = None;
        self.audio_guide.show = false;
        self.show_settings = false;
        if self.audio_notes.start.is_empty() {
            self.audio_notes.start = Beat::from_grid(self.beat.max(0.), 1000000)
                .unwrap_or(Beat::ZERO)
                .to_string();
            self.audio_notes.end = Beat::from_grid(self.beat.max(0.) + 8., 1000000)
                .unwrap_or(Beat::ONE)
                .to_string();
        }
        self.audio_guide.generation_requested = true;
    }
    fn audio_note_key(&self) -> Result<Key, String> {
        if !self.audio_notes.show
            || !self.audio_guide.current(
                self.session,
                self.asset_revision,
                self.project_options.music.as_deref(),
            )
        {
            return Err("Load music to see audio guides.".into());
        }
        let (_, source) = self
            .audio_guide
            .generation_markers(&self.options.audio_guide)
            .ok_or("Wait for attack analysis to finish before building a note preview.")?;
        let tempo = self.editor.document().tempo();
        let offset = self
            .project_options
            .music_offset_seconds(self.editor.document());
        let end = tempo
            .beat_at(
                self.audio_guide
                    .source_duration()
                    .ok_or("Load music to see audio guides.")?
                    - offset,
            )
            .max(0.);
        let beat = |v: f64| Beat::from_grid(v.max(0.), 1000000).map_err(|e| e.to_string());
        let range = match self.audio_notes.range {
            Range::Song => [Beat::ZERO, beat(end)?],
            Range::Cursor => [beat(self.beat)?, beat(end)?],
            Range::Visible => {
                let g = self
                    .multi_line
                    .base
                    .ok_or("Open the note grid before using its visible range.")?;
                [
                    beat(g.raw_beat(g.rect.bottom()).max(0.))?,
                    beat(g.raw_beat(g.rect.top()).max(0.))?,
                ]
            }
            Range::Custom => [
                self.audio_notes
                    .start
                    .parse::<Beat>()
                    .map_err(|e| e.to_string())?,
                self.audio_notes
                    .end
                    .parse::<Beat>()
                    .map_err(|e| e.to_string())?,
            ],
        };
        let lines = if self.audio_notes.multiple {
            if !self.multi_line.enabled || self.multi_line.lines.is_empty() {
                return Err("Enable Multiple lines and choose target lines first.".into());
            }
            self.multi_line.lines.clone()
        } else {
            vec![self.audio_notes.line]
        };
        Ok(Key {
            session: self.session,
            revision: self.editor.revision(),
            assets: self.asset_revision,
            source,
            spec: AudioNoteSpec {
                options: self.options.audio_notes.clone(),
                range,
                lines,
                audio_offset: offset,
            },
        })
    }
    pub(crate) fn poll_audio_notes(&mut self) {
        let key = self.audio_note_key().ok();
        if self
            .audio_notes
            .pending
            .as_ref()
            .is_some_and(|p| Some(&p.key) != key.as_ref())
            && let Some(p) = self.audio_notes.pending.take()
        {
            p.cancel.store(true, Ordering::Release);
        }
        while let Some(reply) = self
            .audio_notes
            .worker
            .as_ref()
            .and_then(|w| w.replies.try_recv().ok())
        {
            if self
                .audio_notes
                .pending
                .as_ref()
                .is_none_or(|p| p.id != reply.id)
            {
                continue;
            }
            let pending = self.audio_notes.pending.take().unwrap();
            if Some(&pending.key) != key.as_ref() || pending.cancel.load(Ordering::Acquire) {
                continue;
            }
            match reply.result {
                Ok(prepared) => {
                    self.audio_notes.preview = Some((pending.key, prepared));
                    self.audio_notes.error = None;
                }
                Err(error) => self.audio_notes.error = Some(error),
            }
        }
    }
    pub(crate) fn build_audio_note_preview(&mut self) {
        let result = (|| -> Result<(), String> {
            let key = self.audio_note_key()?;
            if matches!(self.audio_notes.range, Range::Visible | Range::Cursor) {
                self.audio_notes.start = key.spec.range[0].to_string();
                self.audio_notes.end = key.spec.range[1].to_string();
                self.audio_notes.range = Range::Custom;
            }
            if self.audio_notes.worker.is_none() {
                self.audio_notes.worker = Some(worker::Planner::new()?);
            }
            let markers = self
                .audio_guide
                .generation_markers(&self.options.audio_guide)
                .unwrap()
                .0
                .iter()
                .map(|m| AudioMarker {
                    seconds: m.seconds,
                    strength: m.strength,
                    band: m.band,
                })
                .collect();
            self.audio_notes.next_id = self.audio_notes.next_id.wrapping_add(1);
            let id = self.audio_notes.next_id;
            let cancel = Arc::new(AtomicBool::new(false));
            self.audio_notes
                .worker
                .as_ref()
                .unwrap()
                .send(worker::Request {
                    id,
                    document: self.editor.document().clone(),
                    markers,
                    spec: key.spec.clone(),
                    cancel: Arc::clone(&cancel),
                })?;
            self.audio_notes.pending = Some(Pending { id, key, cancel });
            self.audio_notes.preview = None;
            Ok(())
        })();
        self.audio_notes.error = result.err();
    }
    pub(crate) fn apply_audio_note_preview(&mut self) {
        let key = self.audio_note_key().ok();
        if !self
            .audio_notes
            .preview
            .as_ref()
            .is_some_and(|(old, _)| Some(old) == key.as_ref())
        {
            self.audio_notes.error =
                Some("The preview is out of date. Build it again before inserting notes.".into());
            return;
        }
        let (_, prepared) = self.audio_notes.preview.take().unwrap();
        if prepared.plan.notes.is_empty() {
            return;
        }
        let edit = insert_audio_notes(self.editor.document(), &prepared.plan.notes);
        if self.apply_tool(edit) {
            self.audio_notes.close();
        } else {
            self.audio_notes.error = self.error.take();
        }
    }
    pub(crate) fn paint_audio_note_preview(
        &self,
        painter: &egui::Painter,
        line: usize,
        grid: crate::grid_input::Grid,
    ) {
        if self.audio_notes.hide_ghosts {
            return;
        }
        let Some((old, prepared)) = &self.audio_notes.preview else {
            return;
        };
        if self.audio_note_key().as_ref().ok() != Some(old) {
            return;
        }
        let Some(ghosts) = prepared.ghosts.get(&line) else {
            return;
        };
        let skin = self
            .skin
            .as_ref()
            .map(|s| s.grid.as_ref().map_or(&s.normal, |g| &g.normal));
        let mut batch = crate::timeline::NoteBatch::new(painter);
        let padding = crate::timeline::note_margin(self.skin.as_ref(), grid.note_width);
        for index in ghosts.index.query(
            grid.raw_beat(grid.rect.bottom() + padding),
            grid.raw_beat(grid.rect.top() - padding),
        ) {
            crate::timeline::draw_note(
                &mut batch,
                &grid,
                &ghosts.notes[index].1,
                false,
                true,
                skin,
            );
        }
        batch.flush();
    }
    pub(crate) fn focus_audio_note_preview(&mut self) {
        if !self
            .audio_notes
            .preview
            .as_ref()
            .is_some_and(|(old, _)| self.audio_note_key().as_ref().ok() == Some(old))
        {
            return;
        }
        if let Some(item) = self
            .audio_notes
            .preview
            .as_ref()
            .and_then(|(_, p)| p.plan.notes.first())
        {
            self.line = item.line;
            let offset = self
                .multi_line
                .base
                .map_or(1., |g| ((g.baseline() - g.rect.center().y) / g.zoom) as f64);
            self.beat = (item.note.start_time.as_f64() - offset).max(0.);
            self.camera_x = if item.note.x.abs() > 675. {
                item.note.x
            } else {
                0.
            };
            if self.multi_line.enabled && !self.multi_line.lines.contains(&self.line) {
                self.multi_line.lines.push(self.line);
            }
            // A visible/cursor range follows navigation, so freeze the approved
            // range before the explicit focus action moves the viewport.
            if matches!(self.audio_notes.range, Range::Visible | Range::Cursor)
                && let Some((old, _)) = &self.audio_notes.preview
            {
                self.audio_notes.start = old.spec.range[0].to_string();
                self.audio_notes.end = old.spec.range[1].to_string();
                self.audio_notes.range = Range::Custom;
            }
        }
    }
    pub(crate) fn audio_notes_window(&mut self, ctx: &egui::Context) {
        if !self.audio_notes.show || self.show_projects {
            return;
        }
        let mut open = true;
        let mut preview = false;
        let mut apply = false;
        let mut focus = false;
        let key = self.audio_note_key().ok();
        let valid = self
            .audio_notes
            .preview
            .as_ref()
            .is_some_and(|(old, _)| Some(old) == key.as_ref());
        let count = if valid {
            self.audio_notes
                .preview
                .as_ref()
                .unwrap()
                .1
                .plan
                .notes
                .len()
        } else {
            0
        };
        let max = egui::vec2(
            (ctx.screen_rect().width() - 32.).max(180.),
            (ctx.screen_rect().height() - 90.).max(110.),
        );
        crate::i18n::window("Notes from audio markers")
            .open(&mut open)
            .default_width(520_f32.min(max.x))
            .min_width(320_f32.min(max.x))
            .default_height(520_f32.min(max.y))
            .default_pos([(ctx.screen_rect().right() - 536.).max(16.), 48.])
            .resize(|r| r.max_size(max))
            .show(ctx, |ui| {
                ui.set_enabled(!self.modal_open());
                ui.style_mut().wrap = Some(true);
                let height = (ui.available_height() - 90.).max(20.);
                let scroll = egui::ScrollArea::vertical()
                    .id_source("audio-note-generator-scroll")
                    .max_height(height)
                    .touch_show_y(ui, |ui| {
                        self.audio_note_controls(ui);
                        focus = ui
                            .add_enabled(
                                valid && count > 0,
                                egui::Button::new(crate::i18n::tr("Show first preview note")),
                            )
                            .clicked();
                        ui.small(crate::i18n::tr(
                            "Collapse this window to inspect the preview on the grid.",
                        ));
                    });
                ui.separator();
                if self.audio_notes.pending.is_some() {
                    ui.horizontal_wrapped(|ui| {
                        ui.spinner();
                        ui.label(crate::i18n::tr("Building note preview…"));
                    });
                    ctx.request_repaint_after(std::time::Duration::from_millis(50));
                } else if self.audio_notes.error.is_some() {
                    if ui
                        .small_button(crate::i18n::tr("Preview failed — show details"))
                        .clicked()
                    {
                        let mut state = scroll.state;
                        state.offset.y = 0.;
                        state.store(ui.ctx(), scroll.id);
                    }
                } else if valid {
                    ui.label(crate::i18n::format(
                        "Preview: {0} notes",
                        &[count.to_string()],
                    ));
                } else if self.audio_notes.preview.is_some() {
                    ui.small(crate::i18n::tr(
                        "The preview is out of date. Build it again before inserting notes.",
                    ));
                }
                ui.horizontal_wrapped(|ui| {
                    if self.audio_notes.pending.is_some() {
                        if ui.button(crate::i18n::tr("Cancel preview")).clicked()
                            && let Some(p) = self.audio_notes.pending.take()
                        {
                            p.cancel.store(true, Ordering::Release);
                        }
                    } else {
                        preview = ui
                            .add_enabled(
                                key.is_some(),
                                egui::Button::new(crate::i18n::tr("Build note preview")),
                            )
                            .clicked();
                    }
                    apply = ui
                        .add_enabled(
                            valid && count > 0,
                            egui::Button::new(crate::i18n::format(
                                "Insert {0} notes",
                                &[count.to_string()],
                            )),
                        )
                        .clicked();
                });
            });
        if !open {
            self.audio_notes.close();
            return;
        }
        if preview {
            self.build_audio_note_preview();
        }
        if focus {
            self.focus_audio_note_preview();
        }
        if apply {
            self.apply_audio_note_preview();
        }
    }
}
#[cfg(test)]
mod tests;
