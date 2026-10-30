use super::*;

impl Workbench {
    pub fn is_scrubbing(&self) -> bool {
        self.timeline_scrub
            || self.grid_input.is_scrubbing()
            || self.preview.drag.as_ref().is_some_and(|d| !d.exit)
            || (self.preview.fullscreen && self.preview.blocked && !self.preview.touches.is_empty())
    }

    pub(super) fn seek_max(&mut self) -> f64 {
        let revision = self.editor.revision();
        if self.seek_extent.is_none_or(|(r, _)| r != revision) {
            let document = self.editor.document();
            let last = (0..document.line_count())
                .flat_map(|line| document.notes(line))
                .map(|(_, note)| note.end_time.as_f64().max(note.start_time.as_f64()))
                .fold(0_f64, f64::max);
            self.seek_extent = Some((revision, ((last + 8.) / 16.).ceil().max(4.) * 16.));
        }
        let notes = self.seek_extent.unwrap().1;
        if self.options.seek_notes_only {
            notes
        } else {
            self.music_duration
                .filter(|s| s.is_finite() && *s > 0.)
                .map(|s| self.editor.document().tempo().beat_at(s).max(1.))
                .unwrap_or_else(|| notes.max((self.beat / 16.).ceil() * 16.))
        }
    }

    pub(super) fn seek_bar(&mut self, ui: &mut egui::Ui, width: f32) {
        let max = self.seek_max();
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, 32.), Sense::click_and_drag());
        let track = rect.shrink2(egui::vec2(7., 0.));
        let active = response.is_pointer_button_down_on();
        let release = ui.input(|i| {
            i.events.iter().rev().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                } => Some(*pos),
                _ => None,
            })
        });
        if (active || response.clicked() || self.timeline_scrub) && ui.is_enabled() {
            let position = if self.timeline_scrub { release } else { None }
                .or_else(|| response.interact_pointer_pos());
            if let Some(pos) = position {
                self.beat =
                    (((pos.x - track.left()) / track.width()).clamp(0., 1.) as f64 * max).max(0.);
            }
        }
        self.timeline_scrub = active && ui.input(|i| i.pointer.primary_down()) && ui.is_enabled();
        let y = rect.center().y;
        ui.painter().line_segment(
            [egui::pos2(track.left(), y), egui::pos2(track.right(), y)],
            egui::Stroke::new(3_f32, border()),
        );
        let x = track.left() + (self.beat / max).clamp(0., 1.) as f32 * track.width();
        ui.painter().line_segment(
            [egui::pos2(track.left(), y), egui::pos2(x, y)],
            egui::Stroke::new(3_f32, accent()),
        );
        ui.painter()
            .circle_filled(egui::pos2(x, y), if active { 6. } else { 4. }, accent());
        response.on_hover_text({
            let __i18n_arg_0 = &(crate::i18n::tr(if self.options.seek_notes_only {
                "Notes range"
            } else {
                "Whole chart"
            }));
            let __i18n_arg_1 = &(self.beat);
            crate::i18n::format(
                "{0} · Beat {1} / {2}",
                &[
                    __i18n_arg_0.to_string(),
                    format!("{:.2}", __i18n_arg_1),
                    format!("{:.2}", max),
                ],
            )
        });
    }
}
