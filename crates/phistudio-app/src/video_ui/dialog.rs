use super::*;

impl Workbench {
    pub(crate) fn open_video(&mut self) {
        if !self.flush_shader_draft() {
            return;
        }
        self.transport.playing = false;
        self.timeline.cancel_gesture();
        self.transport.scrubbing = false;
        let name = self
            .project_options
            .display_field(self.editor.document(), "name");
        self.video.name = movie_name(name);
        self.video.destination = self.video.name.clone();
        let last = (0..self.editor.document().line_count())
            .flat_map(|l| self.editor.document().notes(l))
            .map(|(_, n)| {
                self.editor
                    .document()
                    .tempo()
                    .seconds_at(n.start_time.as_f64().max(n.end_time.as_f64()))
            })
            .fold(0., f64::max);
        self.video.settings.end = self.transport.music_duration.unwrap_or(last + 1.);
        self.video.open = true;
        self.video.error = None;
        self.video.message = None;
    }

    pub(crate) fn video_window(&mut self, ctx: &egui::Context) {
        if !self.video.open {
            return;
        }
        let mut open = true;
        let busy = self.video.job.is_some();
        let width = (ctx.screen_rect().width() - 48.).clamp(200., 760.);
        crate::i18n::window("Export MP4")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
            .default_width(width)
            .show(ctx, |ui| {
                ui.set_width(width);
                if let Some(job) = &self.video.job {
                    let phase = job.phase.load(Ordering::Acquire);
                    let warming = (job.warm as f64 / f64::from(job.plan.fps)) < job.plan.start;
                    let label = if job.cancel.load(Ordering::Relaxed) {
                        "Canceling export…"
                    } else if phase == 0 {
                        "Preparing audio and encoder…"
                    } else if phase == 2 {
                        "Finishing MP4…"
                    } else if warming {
                        "Preparing chart at range start…"
                    } else {
                        "Rendering video…"
                    };
                    ui.label(tr(label));
                    let done = job.encoded.load(Ordering::Relaxed);
                    let progress = if phase == 1 && warming {
                        (job.warm as f64 / (job.plan.start * f64::from(job.plan.fps))) as f32
                    } else {
                        done as f32 / job.plan.frames as f32
                    };
                    ui.add(egui::ProgressBar::new(progress).show_percentage());
                    ui.label(fmt(
                        "Frames: {0} / {1}",
                        &[done.to_string(), job.plan.frames.to_string()],
                    ));
                } else if self.video.completed.is_none() {
                    egui::ScrollArea::vertical()
                        .id_source("mp4-settings-scroll")
                        .max_height((ctx.screen_rect().height() - 190.).max(80.))
                        .touch_show(ui, |ui| {
                            video_settings(ui, &mut self.video, self.transport.music_duration)
                        });
                }
                if let Some(file) = &self.video.completed {
                    ui.label(&file.name);
                }
                if let Some(message) = self.video.message {
                    ui.label(tr(message));
                }
                if let Some(error) = &self.video.error {
                    let text = if let Some(detail) = error.strip_prefix("Could not export MP4: ") {
                        fmt(
                            "Could not export MP4: {0}",
                            &[tr(detail
                                .strip_prefix("java.io.IOException: ")
                                .unwrap_or(detail))],
                        )
                    } else {
                        tr(error)
                    };
                    egui::ScrollArea::vertical()
                        .id_source("mp4-error-scroll")
                        .max_height(64.)
                        .touch_show(ui, |ui| {
                            ui.colored_label(crate::theme::flick(), text);
                        });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if busy {
                        if ui
                            .add_enabled(
                                !self
                                    .video
                                    .job
                                    .as_ref()
                                    .unwrap()
                                    .cancel
                                    .load(Ordering::Relaxed),
                                egui::Button::new(tr("Cancel")),
                            )
                            .clicked()
                        {
                            self.video_back();
                        }
                    } else if self.video.completed.is_some() {
                        if ui.button(tr("Save MP4…")).clicked() {
                            #[cfg(any(target_os = "android", target_os = "ios"))]
                            {
                                self.files.request = Some(crate::files::FileRequest::SaveMovie);
                            }
                            #[cfg(not(any(target_os = "android", target_os = "ios")))]
                            {
                                match self
                                    .video
                                    .completed
                                    .as_ref()
                                    .unwrap()
                                    .save(Path::new(&self.video.destination))
                                {
                                    Ok(()) => {
                                        self.video.completed = None;
                                        self.video.open = false;
                                        self.status = "MP4 exported".into();
                                    }
                                    Err(e) => {
                                        self.video.error =
                                            Some(format!("Could not save MP4: {e:#}"))
                                    }
                                }
                            }
                        }
                        if ui.button(tr("Close")).clicked() {
                            self.video.completed = None;
                            self.video.open = false;
                        }
                    } else {
                        if ui.button(tr("Export MP4")).clicked() {
                            self.video.start = true;
                            self.video.message = None;
                        }
                        if ui.button(tr("Cancel")).clicked() {
                            self.video.open = false;
                        }
                    }
                });
            });
        if !open {
            self.video_back();
        }
    }
}
