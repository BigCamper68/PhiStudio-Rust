use super::{Guide, Status};
use crate::scroll::{TouchCombo as _, TouchScroll as _};
use phistudio_core::AudioGuideOptions;
use std::sync::atomic::Ordering;

impl Guide {
    pub fn controls(&mut self, ui: &mut egui::Ui, options: &mut AudioGuideOptions) -> bool {
        use crate::i18n::{format, tr};
        ui.style_mut().wrap = Some(true);
        ui.spacing_mut().slider_width = (ui.available_width() - 80.).clamp(40., 280.);
        ui.checkbox(&mut options.waveform, tr("Waveform behind notes"));
        ui.checkbox(&mut options.attacks, tr("Sound attack markers"));
        ui.small(tr("Attacks mark the start of a sound. Use the note generator to preview and insert notes explicitly."));
        ui.label(tr("Waveform opacity"));
        ui.add(egui::Slider::new(&mut options.opacity, 0.05..=0.6));
        ui.label(tr("Waveform gain"));
        ui.add(egui::Slider::new(&mut options.gain, 0.25..=8.).logarithmic(true));
        Self::attack_controls(ui, options);
        match &self.status {
            _ if !options.waveform && !options.attacks && !self.generation_requested => {
                ui.small(tr("Audio guides are disabled."));
            }
            Status::NoMusic => {
                ui.small(tr(if self.source.is_some() {
                    "Audio guides are disabled."
                } else {
                    "Load music to see audio guides."
                }));
            }
            Status::Working => {
                let progress = self.control.progress.load(Ordering::Relaxed) as f32 / 1000.;
                ui.add(
                    egui::ProgressBar::new(progress).text(tr(if self.result.is_some() {
                        "Updating attack markers…"
                    } else {
                        "Analyzing audio…"
                    })),
                );
                if ui.button(tr("Cancel audio analysis")).clicked() {
                    self.cancel();
                }
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(50));
            }
            Status::Ready => {
                if !self.full_data {
                    ui.small(tr("Waveform ready"));
                } else {
                    ui.small(format(
                        "Audio guide ready · {0} attacks",
                        &[self.markers.len().to_string()],
                    ));
                }
                if self.cached {
                    ui.small(tr("Analysis loaded from local cache."));
                }
            }
            Status::Canceled => {
                ui.small(tr("Audio analysis canceled"));
            }
            Status::Failed(error) => {
                ui.colored_label(ui.visuals().error_fg_color, tr(error));
            }
        }
        if self.source.is_some()
            && !self.working
            && self.manual_cancel
            && ui.button(tr("Analyze audio again")).clicked()
        {
            self.start();
        }
        ui.separator();
        let generate = ui.button(tr("Generate notes from markers…")).clicked();
        if ui.button(tr("Reset audio guide settings")).clicked() {
            *options = AudioGuideOptions::default();
            self.manual_cancel = false;
        }
        generate
    }
    pub(crate) fn attack_controls(ui: &mut egui::Ui, options: &mut AudioGuideOptions) {
        use crate::i18n::tr;
        ui.label(tr("Attack sensitivity"));
        ui.add(egui::Slider::new(&mut options.sensitivity, 0. ..=1.))
            .on_hover_text(tr(
                "Higher sensitivity shows weaker attacks and may include more false detections.",
            ));
        ui.label(tr("Minimum attack gap (ms)"));
        ui.add(egui::Slider::new(&mut options.min_gap_ms, 20..=500));
        ui.label(tr("Frequency focus"));
        egui::ComboBox::from_id_source("audio-guide-band")
            .width(ui.available_width())
            .selected_text(tr(band_name(options.band)))
            .touch_show_ui(ui, |ui| {
                for (i, name) in BANDS.iter().enumerate() {
                    ui.selectable_value(&mut options.band, i, tr(name));
                }
            });
        ui.horizontal_wrapped(|ui| {
            for (color, name) in [
                (egui::Color32::from_rgb(245, 190, 100), BANDS[1]),
                (egui::Color32::from_rgb(100, 207, 230), BANDS[2]),
                (egui::Color32::from_rgb(216, 145, 235), BANDS[3]),
            ] {
                ui.colored_label(color, tr(name));
            }
        });
        ui.small(tr(
            "Marker strength is relative salience, not a probability or a required note position.",
        ));
    }
    pub(crate) fn generation_status(&mut self, ui: &mut egui::Ui) {
        use crate::i18n::tr;
        match &self.status {
            Status::Failed(error) => {
                ui.colored_label(ui.visuals().error_fg_color, tr(error));
            }
            Status::Working => {
                ui.add(
                    egui::ProgressBar::new(
                        self.control.progress.load(Ordering::Relaxed) as f32 / 1000.,
                    )
                    .text(tr("Analyzing audio…")),
                );
                if ui.button(tr("Cancel audio analysis")).clicked() {
                    self.cancel();
                }
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(50));
            }
            _ if self.source.is_none() => {
                ui.small(tr("Load music to see audio guides."));
            }
            _ => {}
        }
        if self.source.is_some()
            && !self.working
            && self.manual_cancel
            && ui.button(tr("Analyze audio again")).clicked()
        {
            self.want_attacks = true;
            self.start();
        }
    }
    pub(crate) fn generation_markers(
        &self,
        options: &AudioGuideOptions,
    ) -> Option<(&[phistudio_audio::Onset], (u64, u64))> {
        let selection = phistudio_audio::Selection {
            sensitivity: options.sensitivity,
            min_gap: f64::from(options.min_gap_ms) / 1000.,
            band: options.band,
        };
        (self.full_data
            && !self.working
            && !self.manual_cancel
            && matches!(self.status, Status::Ready)
            && self.selection == Some(selection)
            && self.requested_selection == Some(selection))
        .then_some((&self.markers, (self.epoch, self.tuning)))
    }
    pub(crate) fn source_duration(&self) -> Option<f64> {
        self.source.as_ref().map(|s| s.0.length())
    }
}
const BANDS: [&str; 4] = [
    "All frequencies",
    "Low (<250 Hz)",
    "Mid (250–2500 Hz)",
    "High (>2500 Hz)",
];
fn band_name(band: usize) -> &'static str {
    BANDS[band.min(3)]
}
impl crate::workbench::Workbench {
    pub(crate) fn audio_guide_button(&mut self, ui: &mut egui::Ui) {
        if ui.button(crate::i18n::tr("Audio guide")).clicked() {
            self.audio_guide.show = true;
        }
    }

    pub(crate) fn audio_guide_window(&mut self, ctx: &egui::Context) {
        if self.show_projects || !self.audio_guide.show {
            return;
        }
        let mut open = true;
        let mut generate = false;
        let max_width = (ctx.screen_rect().width() - 32.).max(160.);
        let max_height = (ctx.screen_rect().height() - 90.).max(100.);
        // Ordinary windows use Order::Middle; the frequency combo uses
        // Order::Foreground and cannot end up behind its parent as in a menu.
        crate::i18n::window("Audio guide")
            .open(&mut open)
            .default_width(420_f32.min(max_width))
            .min_width(280_f32.min(max_width))
            .default_height(500_f32.min(max_height))
            .default_pos([48., 56.])
            .resize(|r| r.max_size([max_width, max_height]))
            .show(ctx, |ui| {
                ui.set_enabled(!self.modal_open());
                egui::ScrollArea::vertical()
                    .id_source("audio-guide-controls-scroll")
                    .max_height(ui.available_height().max(80.))
                    .touch_show_y(ui, |ui| {
                        generate = self.audio_guide.controls(ui, &mut self.options.audio_guide);
                    });
            });
        self.audio_guide.show = open;
        if generate {
            self.open_audio_notes();
        }
    }
}
