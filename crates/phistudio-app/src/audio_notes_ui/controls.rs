use super::*;
use phistudio_core::{
    NoteKind,
    tools::{AudioCollision, AudioPosition, AudioSide},
};
use std::ops::RangeInclusive;
fn number<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut N,
    speed: f64,
    range: RangeInclusive<N>,
) {
    crate::form::Form::new(ui, &[label]).field(ui, label, |ui| {
        ui.add(
            crate::numeric::Value::new(value)
                .speed(speed)
                .clamp_range(range),
        )
    });
}
fn choice<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[(T, &str)],
) {
    let selected = options
        .iter()
        .find(|(v, _)| v == value)
        .map_or("", |(_, s)| *s);
    ui.label(crate::i18n::tr(label));
    egui::ComboBox::from_id_source(label)
        .wrap(true)
        .width(ui.available_width().min(320.))
        .selected_text(crate::i18n::tr(selected))
        .touch_show_ui(ui, |ui| {
            for &(v, name) in options {
                ui.selectable_value(value, v, crate::i18n::tr(name));
            }
        });
}
fn kind(ui: &mut egui::Ui, label: &str, value: &mut NoteKind) {
    choice(
        ui,
        label,
        value,
        &[
            (NoteKind::Tap, "Tap"),
            (NoteKind::Hold, "Hold"),
            (NoteKind::Flick, "Flick"),
            (NoteKind::Drag, "Drag"),
        ],
    );
}
impl Workbench {
    pub(super) fn audio_note_controls(&mut self, ui: &mut egui::Ui) {
        use crate::i18n::{format, tr};
        let validation = self
            .audio_guide
            .generation_markers(&self.options.audio_guide)
            .and_then(|_| self.audio_note_key().err());
        ui.small(tr("Preview first. Notes are inserted only when you press Insert; one Undo removes the whole generation."));
        ui.collapsing(tr("Attack selection"), |ui| {
            crate::audio_guide::Guide::attack_controls(ui, &mut self.options.audio_guide);
            ui.small(tr(
                "Uses the same markers as Audio guide. Changing these filters reuses the analysis.",
            ));
        });
        let lines = self.line_summaries();
        let state = &mut self.audio_notes;
        let o = &mut self.options.audio_notes;
        ui.separator();
        ui.strong(tr("Generation range"));
        choice(
            ui,
            "Range",
            &mut state.range,
            &[
                (Range::Song, "Whole song"),
                (Range::Cursor, "From current beat"),
                (Range::Visible, "Visible note grid"),
                (Range::Custom, "Custom beat range"),
            ],
        );
        if state.range == Range::Custom {
            crate::form::Form::new(ui, &["Start beat", "End beat"]).field(ui, "Start beat", |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.start)
                        .id_source("audio-note-start-beat")
                        .desired_width(ui.available_width()),
                )
            });
            crate::form::Form::new(ui, &["Start beat", "End beat"]).field(ui, "End beat", |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.end)
                        .id_source("audio-note-end-beat")
                        .desired_width(ui.available_width()),
                )
            });
        }
        ui.small(tr(
            "The current/visible range is captured when you build the preview.",
        ));
        ui.checkbox(&mut state.multiple, tr("Distribute across Multiple lines"));
        if state.multiple {
            ui.small(format(
                "{0} target lines",
                &[self.multi_line.lines.len().to_string()],
            ));
        } else {
            ui.label(tr("Destination line"));
            egui::ComboBox::from_id_source("audio-note-line")
                .wrap(true)
                .width(ui.available_width().min(350.))
                .selected_text(lines.get(state.line).map_or_else(
                    || tr("Choose a line"),
                    |l| format!("L{} · {}", l.index, l.name),
                ))
                .touch_show_ui(ui, |ui| {
                    for l in lines.iter() {
                        ui.selectable_value(
                            &mut state.line,
                            l.index,
                            format!("L{} · {}", l.index, l.name),
                        );
                    }
                });
        }
        ui.collapsing(tr("Density and timing"),|ui|{
            number(ui,"Minimum marker strength",&mut o.minimum_strength,0.01,0. ..=1.);
            number(ui,"Keep every Nth marker",&mut o.every,0.25,1..=1000);
            number(ui,"Minimum gap (beats)",&mut o.minimum_gap_beats,0.0625,0. ..=256.);
            number(ui,"Maximum notes",&mut o.max_notes,10.,1..=20000);
            ui.small(tr("The maximum keeps the strongest markers. Equal snapped times keep the strongest marker."));
            ui.checkbox(&mut o.quantize,tr("Quantize generated notes"));
            ui.add_enabled_ui(o.quantize,|ui|{
                number(ui,"Grid subdivisions per beat",&mut o.division,0.25,1..=1920);
                if ui.button(tr("Use editor grid")).clicked(){o.division=self.division;}
                number(ui,"Maximum snap shift (ms)",&mut o.max_snap_ms,1.,0. ..=2000.);
                ui.small(tr("0 allows any snap distance. Other values skip markers farther from the grid."));
            });
            number(ui,"Timing shift (ms)",&mut o.shift_ms,1.,-2000. ..=2000.);
        });
        ui.collapsing(tr("Note types and properties"),|ui|{
            ui.checkbox(&mut o.per_band,tr("Choose note type by frequency"));
            if o.per_band {
                for (i,label) in ["Low (<250 Hz)","Mid (250–2500 Hz)","High (>2500 Hz)"].into_iter().enumerate(){kind(ui,label,&mut o.band_kinds[i]);}
            } else {kind(ui,"Note type",&mut o.kind);}
            if o.kind==NoteKind::Hold || (o.per_band && o.band_kinds.contains(&NoteKind::Hold)) {
                number(ui,"Hold length (beats)",&mut o.hold_beats,0.125,0.000001..=256.);
                ui.checkbox(&mut o.trim_holds,tr("Trim holds at the next generated note"));
                ui.small(tr("Attack markers do not estimate sustain length. Holds use this duration and stay inside the chosen range."));
            }
            choice(ui,"Note side",&mut o.side,&[(AudioSide::Above,"Above the line"),(AudioSide::Below,"Below the line"),(AudioSide::Alternating,"Alternate sides")]);
            number(ui,"Speed",&mut o.speed,0.01,0.000001..=10000.);
            number(ui,"Size",&mut o.size,0.01,0.000001..=10000.);
            number(ui,"Alpha",&mut o.alpha,1.,0..=255);
            number(ui,"Y offset",&mut o.y_offset,1.,-100000. ..=100000.);
            ui.checkbox(&mut o.fake,tr("Fake"));
        });
        ui.collapsing(tr("Horizontal placement"), |ui| {
            choice(
                ui,
                "Position pattern",
                &mut o.position,
                &[
                    (AudioPosition::Fixed, "Fixed X"),
                    (AudioPosition::Alternating, "Alternate left/right"),
                    (AudioPosition::Ascending, "Cycle lanes"),
                    (AudioPosition::PingPong, "Bounce across lanes"),
                    (AudioPosition::Random, "Seeded random lanes"),
                ],
            );
            if o.position == AudioPosition::Fixed {
                number(ui, "Position X", &mut o.x, 5., -100000. ..=100000.);
            } else {
                number(ui, "Left X", &mut o.min_x, 5., -100000. ..=100000.);
                number(ui, "Right X", &mut o.max_x, 5., -100000. ..=100000.);
                number(ui, "Placement lanes", &mut o.lanes, 0.25, 2..=128);
                if o.position == AudioPosition::Random {
                    number(ui, "Random seed", &mut o.seed, 1., 0..=u32::MAX as u64);
                    ui.small(tr(
                        "The same seed and settings reproduce the same placement.",
                    ));
                }
            }
        });
        ui.collapsing(tr("Existing notes and overlaps"),|ui|{
            choice(ui,"Collision policy",&mut o.collision,&[(AudioCollision::Position,"Skip at nearby X"),(AudioCollision::Line,"Skip anywhere on the line"),(AudioCollision::Allow,"Allow overlaps")]);
            if o.collision!=AudioCollision::Allow {
                number(ui,"Head tolerance (ms)",&mut o.collision_ms,1.,0. ..=2000.);
                if o.collision==AudioCollision::Position {number(ui,"Position tolerance",&mut o.collision_x,1.,0. ..=100000.);}
                ui.small(tr("Checks the target line and side, including occupied Hold intervals. Existing notes are preserved."));
            }
        });
        let valid = state
            .preview
            .as_ref()
            .is_some_and(|(old, _)| old.spec.options == *o);
        if valid && let Some((_, p)) = &state.preview {
            ui.collapsing(tr("Preview details"), |ui| {
                let s = &p.plan.stats;
                for (label, n) in [
                    ("Markers received", s.input),
                    ("Weak markers", s.weak),
                    ("Outside range", s.outside),
                    ("Too far from grid", s.off_grid),
                    ("Merged times", s.merged),
                    ("Density filtered", s.thinned),
                    ("Limited by maximum", s.limited),
                    ("Note collisions", s.collisions),
                    ("Shortened holds", s.trimmed),
                ] {
                    ui.label(format("{0}: {1}", &[tr(label), n.to_string()]));
                }
            });
        }
        ui.checkbox(&mut state.hide_ghosts, tr("Hide preview notes"));
        ui.small(tr(
            "Translucent notes on the grid are a preview; they cannot be selected until insertion.",
        ));
        ui.small(tr("Press Play to hear preview notes with the current skin and hitsound volume. Pause or close this window to stop."));
        if let Some(error) = &state.error {
            ui.colored_label(ui.visuals().error_fg_color, tr(error));
        }
        // Transient analysis/errors belong after the inputs: inserting them
        // above the form moves an active slider and shifts egui's auto IDs.
        if let Some(error) = validation {
            ui.colored_label(ui.visuals().error_fg_color, tr(&error));
        }
        self.audio_guide.generation_status(ui);
        if ui.button(tr("Reset generator settings")).clicked() {
            *o = AudioNoteOptions::default();
            state.range = Range::Song;
            state.multiple = false;
            state.hide_ghosts = false;
            state.preview = None;
            if let Some(p) = state.pending.take() {
                p.cancel.store(true, Ordering::Release);
            }
            state.error = None;
        }
    }
}
