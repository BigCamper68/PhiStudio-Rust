use crate::{scroll::TouchCombo as _, theme::*, workbench::Workbench};
use phistudio_core::text_composition::{self, Font, Mode, Plan, Spec};
use std::sync::mpsc;
type Key = (u64, Spec);
type Reply = (Key, Result<Plan, String>);
pub(crate) struct TextState {
    pub spec: Spec,
    start: String,
    end: String,
    worker: Option<mpsc::Receiver<Reply>>,
    preview: Option<Reply>,
}
impl Default for TextState {
    fn default() -> Self {
        let spec = Spec::default();
        Self {
            start: spec.start.to_string(),
            end: spec.end.to_string(),
            spec,
            worker: None,
            preview: None,
        }
    }
}
impl TextState {
    pub(crate) fn load(&mut self, spec: Spec) {
        self.start = spec.start.to_string();
        self.end = spec.end.to_string();
        self.spec = spec;
        self.preview = None;
    }
}
fn number(ui: &mut egui::Ui, label: &str, value: &mut f64) {
    ui.horizontal(|ui| {
        ui.label(crate::i18n::tr(label));
        ui.add(crate::numeric::Value::new(value).speed(0.1).max_decimals(3));
    });
}
impl Workbench {
    pub(crate) fn text_composition_ui(&mut self, ui: &mut egui::Ui) {
        let mut state = std::mem::take(&mut self.text_composition);
        if let Some(rx) = &state.worker {
            match rx.try_recv() {
                Ok(result) => {
                    state.preview = Some(result);
                    state.worker = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.error = Some("Text preparation interrupted".into());
                    state.worker = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(25));
                }
            }
        }
        ui.strong(crate::i18n::tr("Text from notes / lines"));
        ui.small(crate::i18n::tr("Builds font outlines from ordinary chart objects. Preview first; the whole insertion has one Undo."));
        ui.small(crate::i18n::tr(
            "The first generated line controls the position and rotation of the text.",
        ));
        ui.label(crate::i18n::tr("Text"));
        ui.add(egui::TextEdit::multiline(&mut state.spec.text).desired_rows(2));
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Name"));
            ui.text_edit_singleline(&mut state.spec.name);
        });
        let modes = [
            (Mode::Lines, "Judge lines"),
            (Mode::Tap, "Fake Tap"),
            (Mode::Drag, "Fake Drag"),
            (Mode::Flick, "Fake Flick"),
        ];
        egui::ComboBox::from_id_source("text-composition-mode")
            .selected_text(crate::i18n::tr(
                modes.iter().find(|(m, _)| *m == state.spec.mode).unwrap().1,
            ))
            .touch_show_ui(ui, |ui| {
                for (mode, label) in modes {
                    ui.selectable_value(&mut state.spec.mode, mode, crate::i18n::tr(label));
                }
            });
        egui::ComboBox::from_id_source("text-composition-font")
            .selected_text(crate::i18n::tr(if state.spec.font == Font::Phira {
                "Phira font"
            } else {
                "Phigros font"
            }))
            .touch_show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut state.spec.font,
                    Font::Phira,
                    crate::i18n::tr("Phira font"),
                );
                ui.selectable_value(
                    &mut state.spec.font,
                    Font::Phigros,
                    crate::i18n::tr("Phigros font"),
                );
            });
        for (label, value) in [
            ("Start beat", &mut state.start),
            ("End beat", &mut state.end),
        ] {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr(label));
                ui.add(egui::TextEdit::singleline(value).desired_width(100.));
            });
        }
        number(ui, "Text height", &mut state.spec.height);
        number(ui, "Center X", &mut state.spec.center[0]);
        number(ui, "Center Y", &mut state.spec.center[1]);
        number(ui, "Rotate", &mut state.spec.rotation);
        number(ui, "Letter spacing", &mut state.spec.tracking);
        number(ui, "Line spacing", &mut state.spec.line_spacing);
        number(ui, "Outline flatness", &mut state.spec.flatness);
        if state.spec.mode == Mode::Lines {
            number(ui, "Stroke width", &mut state.spec.thickness);
        } else {
            number(ui, "Point spacing", &mut state.spec.spacing);
            number(ui, "Note size", &mut state.spec.note_size);
        }
        let parsed = (|| {
            state.spec.start = state.start.parse::<phistudio_core::Beat>()?;
            state.spec.end = state.end.parse()?;
            Ok::<_, phistudio_core::ChartError>(())
        })();
        state.spec.aspect = f64::from(self.project_options.aspect_ratio.unwrap_or(16. / 9.));
        let key = (self.editor.revision(), state.spec.clone());
        if ui
            .add_enabled(
                state.worker.is_none() && parsed.is_ok(),
                egui::Button::new(crate::i18n::tr("Preview text")),
            )
            .clicked()
        {
            let (tx, rx) = mpsc::channel();
            let spec = state.spec.clone();
            let tempo = self.editor.document().tempo().clone();
            let key = key.clone();
            state.worker = Some(rx);
            std::thread::spawn(move || {
                let font = if spec.font == Font::Phira {
                    phistudio_prpr::TEXT_FONT_BYTES
                } else {
                    include_bytes!("../../../assets/fonts/phigros.ttf")
                };
                let result =
                    text_composition::prepare(spec, font, &tempo).map_err(|e| e.to_string());
                let _ = tx.send((key, result));
            });
        }
        if let Err(error) = parsed {
            ui.colored_label(flick(), crate::i18n::tr(error.to_string()));
        }
        let ready = state
            .preview
            .as_ref()
            .is_some_and(|(k, p)| *k == key && p.is_ok());
        if let Some((preview_key, result)) = &state.preview {
            match result {
                Ok(plan) => {
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 104.),
                        egui::Sense::hover(),
                    );
                    let p = ui.painter_at(rect);
                    p.rect_filled(rect, 3., base());
                    let points = plan.contours.iter().flatten();
                    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
                    for v in points {
                        for axis in 0..2 {
                            let n = if axis == 1 {
                                v[axis] / plan.spec.aspect * 1.5
                            } else {
                                v[axis]
                            };
                            lo[axis] = lo[axis].min(n);
                            hi[axis] = hi[axis].max(n);
                        }
                    }
                    let scale = ((rect.width() - 12.) as f64 / (hi[0] - lo[0]).max(1.))
                        .min((rect.height() - 12.) as f64 / (hi[1] - lo[1]).max(1.));
                    for contour in plan
                        .contours
                        .iter()
                        .filter(|_| plan.spec.mode == Mode::Lines)
                    {
                        let path = contour
                            .iter()
                            .map(|v| {
                                egui::pos2(
                                    rect.center().x
                                        + ((v[0] - (lo[0] + hi[0]) / 2.) * scale) as f32,
                                    rect.center().y
                                        - ((v[1] / plan.spec.aspect * 1.5 - (lo[1] + hi[1]) / 2.)
                                            * scale)
                                            as f32,
                                )
                            })
                            .collect();
                        p.add(egui::Shape::line(path, egui::Stroke::new(1_f32, accent())));
                    }
                    for mark in &plan.marks {
                        let at = egui::pos2(
                            rect.center().x + ((mark[0] - (lo[0] + hi[0]) / 2.) * scale) as f32,
                            rect.center().y
                                - ((mark[1] / plan.spec.aspect * 1.5 - (lo[1] + hi[1]) / 2.)
                                    * scale) as f32,
                        );
                        p.circle_filled(
                            at,
                            (plan.spec.note_size * 25.).clamp(0.8, 5.) as f32,
                            accent(),
                        );
                    }
                    ui.label(crate::i18n::format(
                        "{0} lines · {1} notes",
                        &[plan.lines.to_string(), plan.notes.to_string()],
                    ));
                    if *preview_key != key {
                        ui.small(crate::i18n::tr("Parameters changed; refresh the preview"));
                    }
                }
                Err(error) => {
                    ui.colored_label(flick(), crate::i18n::tr(error));
                }
            }
        }
        if ui
            .add_enabled(
                ready,
                egui::Button::new(crate::i18n::tr("Insert text objects")),
            )
            .clicked()
            && let Some((_, Ok(plan))) = state.preview.take()
        {
            let line = self.editor.document().line_count();
            let anchor = self.anchor();
            match plan.apply(&mut self.editor) {
                Ok(selection) => {
                    self.remember_anchor(anchor);
                    self.line = line;
                    self.playing = false;
                    self.set_selection(selection);
                    self.error = None;
                    if let Ok(mut v) = self.parameter_snapshot() {
                        v["text_composition"] = serde_json::to_value(&state.spec).unwrap();
                        self.presets.remember(v);
                    }
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
        self.text_composition = state;
    }
}
