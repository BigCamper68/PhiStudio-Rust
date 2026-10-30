use super::*;

pub(super) fn video_settings(ui: &mut egui::Ui, state: &mut VideoState, song_end: Option<f64>) {
    ui.horizontal(|ui| {
        ui.label(tr("Filename"));
        ui.add(egui::TextEdit::singleline(&mut state.name).desired_width(ui.available_width()));
    });
    let s = &mut state.settings;
    if ui.available_width() >= 620. {
        ui.columns(2, |cols| {
            video_picture(&mut cols[0], s);
            video_range(&mut cols[0], s);
            video_appearance(&mut cols[1], s);
            video_audio(&mut cols[1], s);
        });
    } else {
        video_picture(ui, s);
        video_range(ui, s);
        video_appearance(ui, s);
        video_audio(ui, s);
    }
    if let Ok(plan) = s.plan(song_end.unwrap_or(s.end)) {
        ui.separator();
        ui.label(fmt(
            "Duration: {0} s · Estimated size: {1} MB",
            &[
                format!("{:.2}", plan.duration()),
                format!(
                    "{:.1}",
                    plan.duration()
                        * (f64::from(s.bitrate_mbps) / 8.
                            + if s.audio {
                                f64::from(s.audio_bitrate_kbps) / 8000.
                            } else {
                                0.
                            })
                ),
            ],
        ));
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        ui.separator();
        ui.label(tr("Output path"));
        ui.add(
            egui::TextEdit::singleline(&mut state.destination).desired_width(ui.available_width()),
        );
    }
}

pub(super) fn video_picture(ui: &mut egui::Ui, s: &mut VideoSettings) {
    ui.label(tr("Video"));
    egui::ComboBox::from_id_source("mp4-resolution")
        .selected_text(format!("{} × {}", s.width, s.height))
        .touch_show_ui(ui, |ui| {
            for (w, h) in [
                (1280, 720),
                (1920, 1080),
                (2560, 1440),
                (3840, 2160),
                (1920, 1440),
                (1080, 1920),
            ] {
                if ui
                    .selectable_label(s.width == w && s.height == h, format!("{w} × {h}"))
                    .clicked()
                {
                    s.width = w;
                    s.height = h;
                }
            }
        });
    ui.horizontal(|ui| {
        ui.label(tr("Width"));
        ui.add(
            crate::numeric::Value::new(&mut s.width)
                .clamp_range(64..=4096)
                .speed(2.),
        );
        ui.label(tr("Height"));
        ui.add(
            crate::numeric::Value::new(&mut s.height)
                .clamp_range(64..=4096)
                .speed(2.),
        );
    });
    ui.horizontal(|ui| {
        ui.label(tr("FPS"));
        for fps in [24, 25, 30, 50, 60, 120] {
            ui.selectable_value(&mut s.fps, fps, fps.to_string());
        }
    });
    ui.add(egui::Slider::new(&mut s.bitrate_mbps, 1..=100).text(tr("Video bitrate (Mbps)")));
    ui.small(tr("H.264 video. Available resolution and FPS depend on the device encoder. Custom dimensions must be even."));
}

pub(super) fn video_range(ui: &mut egui::Ui, s: &mut VideoSettings) {
    ui.separator();
    ui.label(tr("Time range"));
    ui.checkbox(&mut s.whole_song, tr("Whole song"));
    if !s.whole_song {
        ui.horizontal(|ui| {
            ui.label(tr("Start (seconds)"));
            ui.add(
                crate::numeric::Value::new(&mut s.start)
                    .speed(0.1)
                    .clamp_range(0. ..=7200.),
            );
        });
        ui.horizontal(|ui| {
            ui.label(tr("End (seconds)"));
            ui.add(
                crate::numeric::Value::new(&mut s.end)
                    .speed(0.1)
                    .clamp_range(0. ..=7200.),
            );
        });
    }
    ui.add(egui::Slider::new(&mut s.tail, 0. ..=10.).text(tr("Extra tail (seconds)")));
}

pub(super) fn video_appearance(ui: &mut egui::Ui, s: &mut VideoSettings) {
    ui.label(tr("Appearance"));
    ui.checkbox(&mut s.hud, tr("HUD"));
    ui.checkbox(&mut s.particles, tr("Hit effects"));
    ui.checkbox(&mut s.shaders, tr("Chart shaders"));
    ui.small(tr("Uses the current skin, aspect ratio, note scale and background settings. Export runs at normal speed."));
}

pub(super) fn video_audio(ui: &mut egui::Ui, s: &mut VideoSettings) {
    ui.separator();
    ui.checkbox(&mut s.audio, tr("Include audio"));
    ui.add_enabled_ui(s.audio, |ui| {
        ui.add(egui::Slider::new(&mut s.music, 0. ..=1.).text(tr("Music volume")));
        ui.add(egui::Slider::new(&mut s.hitsounds, 0. ..=1.).text(tr("Hitsound volume")));
        ui.add(
            egui::Slider::new(&mut s.audio_bitrate_kbps, 64..=320).text(tr("Audio bitrate (kbps)")),
        );
        ui.small(tr("AAC · 48 kHz · Stereo"));
    });
}
