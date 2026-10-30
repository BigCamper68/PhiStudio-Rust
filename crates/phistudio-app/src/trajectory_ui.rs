use crate::{scroll::TouchCombo as _, theme::*, workbench::Workbench};
use phistudio_core::{
    EventType, LineEvent,
    trajectories::{self, Shape, Spec},
};

struct Preview {
    key: (u64, usize, usize, Option<String>, String, Spec),
    points: Vec<[f64; 2]>,
    result: Result<(trajectories::Edit, usize), String>,
}
pub(crate) struct TrajectoryState {
    pub spec: Spec,
    pub name: String,
    pub selected: Option<String>,
    pub start: String,
    pub end: String,
    pub step: String,
    line: Option<usize>,
    preview: Option<Preview>,
}
impl Default for TrajectoryState {
    fn default() -> Self {
        let spec = Spec::default();
        Self {
            start: spec.start.to_string(),
            end: spec.end.to_string(),
            step: spec.step.to_string(),
            spec,
            name: "Trajectory".into(),
            selected: None,
            line: None,
            preview: None,
        }
    }
}
fn number(ui: &mut egui::Ui, key: &str, value: &mut f64) {
    ui.horizontal(|ui| {
        ui.label(crate::i18n::tr(key));
        ui.add(crate::numeric::Value::new(value).speed(0.5).max_decimals(4));
    });
}
fn easing(ui: &mut egui::Ui, key: &str, value: &mut i32) {
    egui::ComboBox::from_id_source(key)
        .selected_text(format!(
            "{}: {}",
            crate::i18n::tr(key),
            crate::authoring_ui::EASINGS[(*value - 1).clamp(0, 28) as usize]
        ))
        .touch_show_ui(ui, |ui| {
            for (i, name) in crate::authoring_ui::EASINGS.iter().enumerate() {
                ui.selectable_value(value, i as i32 + 1, *name);
            }
        });
}
fn curves() -> Vec<phistudio_prpr::EventCurve> {
    (1..=29)
        .map(|easing_type| {
            phistudio_prpr::EventCurve::new(
                EventType::MoveX,
                &LineEvent {
                    easing_type,
                    end: 1.,
                    ..Default::default()
                },
            )
        })
        .collect()
}
impl Workbench {
    pub(crate) fn trajectory_ui(&mut self, ui: &mut egui::Ui) {
        let mut state = std::mem::take(&mut self.trajectory);
        if state.line != Some(self.line) {
            state.selected = None;
            state.line = Some(self.line);
            state.preview = None;
            // Start a new line's recipe on a free layer, preserving existing motion.
            let free = |layer| {
                [EventType::MoveX, EventType::MoveY, EventType::Rotate]
                    .into_iter()
                    .all(|kind| {
                        self.editor
                            .document()
                            .event_values(self.line, layer, kind)
                            .is_ok_and(|values| {
                                values.iter().all(|raw| {
                                    LineEvent::read(kind, raw).is_ok_and(|event| {
                                        event.end_time <= state.spec.start
                                            || event.start_time >= state.spec.end
                                    })
                                })
                            })
                    })
            };
            if !free(self.events.layer)
                && let Some(layer) = (0..=3).find(|layer| free(*layer))
            {
                self.events.layer = layer;
            }
        }
        ui.strong(crate::i18n::tr("Motion generator"));
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Layer"));
            ui.add(crate::numeric::Value::new(&mut self.events.layer).clamp_range(0..=3));
        });
        ui.small(crate::i18n::format("Creates paired Move X / Move Y intervals on line {0}, layer {1}. Existing events must leave this range free.",&[self.line.to_string(),self.events.layer.to_string()]));
        let entries = self.editor.document().as_json()["judgeLineList"][self.line]
            .get(trajectories::RECIPE_FIELD)
            .and_then(serde_json::Value::as_array);
        if state.selected.as_deref().is_some_and(|id| {
            !entries
                .into_iter()
                .flatten()
                .any(|v| v["id"].as_str() == Some(id))
        }) {
            state.selected = None;
            state.preview = None;
        }
        let mut load = None;
        egui::ComboBox::from_id_source("trajectory-recipes")
            .selected_text(if state.selected.is_some() {
                state.name.clone()
            } else {
                crate::i18n::tr("New trajectory")
            })
            .touch_show_ui(ui, |ui| {
                if ui
                    .selectable_label(state.selected.is_none(), crate::i18n::tr("New trajectory"))
                    .clicked()
                {
                    state.selected = None;
                    state.preview = None;
                }
                for entry in entries.into_iter().flatten().take(128) {
                    if let (Some(id), Some(name)) = (entry["id"].as_str(), entry["name"].as_str())
                        && ui
                            .selectable_label(state.selected.as_deref() == Some(id), name)
                            .clicked()
                    {
                        load = Some((
                            id.to_owned(),
                            name.to_owned(),
                            entry["spec"].clone(),
                            entry["layer"].as_u64(),
                        ));
                    }
                }
            });
        if let Some((id, name, spec, layer)) = load {
            match serde_json::from_value::<Spec>(spec) {
                Ok(spec) => {
                    state.start = spec.start.to_string();
                    state.end = spec.end.to_string();
                    state.step = spec.step.to_string();
                    state.spec = spec;
                    state.name = name;
                    state.selected = Some(id);
                    state.preview = None;
                    if let Some(layer) = layer {
                        self.events.layer = (layer as usize).min(3);
                    }
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Name"));
            ui.text_edit_singleline(&mut state.name);
        });
        egui::ComboBox::from_id_source("trajectory-shape")
            .selected_text(crate::i18n::tr(state.spec.shape.label()))
            .touch_show_ui(ui, |ui| {
                for shape in Shape::ALL {
                    if ui
                        .selectable_value(
                            &mut state.spec.shape,
                            shape,
                            crate::i18n::tr(shape.label()),
                        )
                        .changed()
                        && shape == Shape::Polar
                    {
                        state
                            .spec
                            .polar_expressions
                            .get_or_insert_with(|| ["300".into(), "2*pi*t".into()]);
                    }
                }
            });
        for (key, value) in [
            ("Start beat", &mut state.start),
            ("End beat", &mut state.end),
            ("Step in beats", &mut state.step),
        ] {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr(key));
                ui.add(egui::TextEdit::singleline(value).desired_width(110.));
            });
        }
        if matches!(state.spec.shape, Shape::Formula | Shape::Polar) {
            for i in 0..2 {
                ui.label(crate::i18n::tr(if state.spec.shape == Shape::Polar {
                    if i == 0 {
                        "Radius expression"
                    } else {
                        "Angle expression (radians)"
                    }
                } else if i == 0 {
                    "X(t)"
                } else {
                    "Y(t)"
                }));
                let expressions = if state.spec.shape == Shape::Polar {
                    state
                        .spec
                        .polar_expressions
                        .get_or_insert_with(|| state.spec.expressions.clone())
                } else {
                    &mut state.spec.expressions
                };
                ui.text_edit_singleline(&mut expressions[i]);
            }
            number(ui, "Parameter start", &mut state.spec.parameter[0]);
            number(ui, "Parameter end", &mut state.spec.parameter[1]);
        } else {
            number(ui, "Radius X", &mut state.spec.radius[0]);
            if state.spec.shape != Shape::Circle {
                number(ui, "Radius Y", &mut state.spec.radius[1]);
            }
            if !matches!(
                state.spec.shape,
                Shape::Line
                    | Shape::Arc
                    | Shape::Triangle
                    | Shape::Square
                    | Shape::Pentagon
                    | Shape::Hexagon
                    | Shape::Octagon
            ) {
                number(ui, "Cycles", &mut state.spec.turns);
            }
            if matches!(
                state.spec.shape,
                Shape::Rose
                    | Shape::Lissajous
                    | Shape::Epicycloid
                    | Shape::Hypocycloid
                    | Shape::Epitrochoid
                    | Shape::Hypotrochoid
                    | Shape::Superellipse
                    | Shape::Star
            ) {
                number(ui, "Frequency X", &mut state.spec.frequency[0]);
            }
            if matches!(
                state.spec.shape,
                Shape::Lissajous | Shape::Epitrochoid | Shape::Hypotrochoid
            ) {
                number(ui, "Frequency Y", &mut state.spec.frequency[1]);
            }
            if state.spec.shape == Shape::Lissajous {
                number(ui, "Phase (radians)", &mut state.spec.phase);
            }
            if state.spec.shape == Shape::RandomLoop {
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::tr("Random seed"));
                    ui.add(crate::numeric::Value::new(&mut state.spec.seed));
                });
            }
        }
        number(ui, "Center X", &mut state.spec.center[0]);
        number(ui, "Center Y", &mut state.spec.center[1]);
        number(ui, "Scale X", &mut state.spec.scale[0]);
        number(ui, "Scale Y", &mut state.spec.scale[1]);
        number(ui, "Rotate", &mut state.spec.rotation);
        number(ui, "Trim start", &mut state.spec.trim[0]);
        number(ui, "Trim end", &mut state.spec.trim[1]);
        easing(ui, "X time", &mut state.spec.easing[0]);
        easing(ui, "Y time", &mut state.spec.easing[1]);
        ui.checkbox(
            &mut state.spec.tangent,
            crate::i18n::tr("Follow trajectory tangent"),
        );
        ui.checkbox(
            &mut state.spec.simplify,
            crate::i18n::tr("Simplify generated events"),
        );
        ui.small(crate::i18n::tr("The preview follows the sampled events. Use a smaller step for finer curves. Tolerances are measured against the unsimplified samples."));
        if state.spec.simplify {
            number(ui, "X tolerance", &mut state.spec.tolerance[0]);
            number(ui, "Y tolerance", &mut state.spec.tolerance[1]);
            if state.spec.tangent {
                number(ui, "Rotation tolerance", &mut state.spec.tolerance[2]);
            }
        }
        let parsed = (|| {
            state.spec.start = state.start.parse()?;
            state.spec.end = state.end.parse()?;
            state.spec.step = state.step.parse()?;
            state.spec.compile()
        })();
        match parsed {
            Ok(_) => {
                let key = (
                    self.editor.revision(),
                    self.line,
                    self.events.layer,
                    state.selected.clone(),
                    state.name.clone(),
                    state.spec.clone(),
                );
                if state.preview.as_ref().is_none_or(|p| p.key != key)
                    && !ui.input(|i| i.pointer.primary_down())
                {
                    let curves = curves();
                    let ease =
                        |id: i32, t: f64| f64::from(curves[(id - 1) as usize].sample(t as f32));
                    let result = trajectories::prepare(
                        self.editor.document(),
                        self.line,
                        self.events.layer,
                        state.selected.as_deref(),
                        &state.name,
                        state.spec.clone(),
                        ease,
                    )
                    .map_err(|e| e.to_string());
                    let points = result
                        .as_ref()
                        .map(|(edit, _)| edit.points().to_vec())
                        .unwrap_or_default();
                    state.preview = Some(Preview {
                        key: key.clone(),
                        points,
                        result,
                    });
                }
                if let Some(preview) = &state.preview {
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(
                            ui.available_width(),
                            (ui.available_width() * 2. / 3.)
                                .clamp(80., 320.)
                                .min(ui.clip_rect().height().max(32.)),
                        ),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(rect, 2., base());
                    if !preview.points.is_empty() {
                        let lo = [
                            preview.points.iter().map(|p| p[0]).fold(-675., f64::min),
                            preview.points.iter().map(|p| p[1]).fold(-450., f64::min),
                        ];
                        let hi = [
                            preview.points.iter().map(|p| p[0]).fold(675., f64::max),
                            preview.points.iter().map(|p| p[1]).fold(450., f64::max),
                        ];
                        let map = crate::plot::Cartesian::fit(rect.shrink(8.), lo, hi);
                        let points = preview.points.iter().map(|p| map.point(*p)).collect();
                        ui.painter().add(egui::Shape::line(
                            points,
                            egui::Stroke::new(1.5_f32, accent()),
                        ));
                    }
                    match &preview.result {
                        Ok((_, count)) => {
                            ui.small(crate::i18n::format(
                                "Generated events: {0}",
                                &[count.to_string()],
                            ));
                        }
                        Err(error) => {
                            ui.colored_label(flick(), crate::i18n::tr(error));
                        }
                    }
                }
                let ready = state.preview.as_ref().is_some_and(|p| {
                    p.key == key && p.result.as_ref().is_ok_and(|(edit, _)| !edit.is_noop())
                });
                if ui
                    .add_enabled(
                        ready,
                        egui::Button::new(crate::i18n::tr(if state.selected.is_some() {
                            "Rebuild trajectory"
                        } else {
                            "Generate motion"
                        })),
                    )
                    .clicked()
                    && let Some(preview) = state.preview.take()
                    && let Ok((edit, _)) = preview.result
                {
                    self.remember_trajectory_parameters(&state.spec, &state.name);
                    match edit.apply(&mut self.editor) {
                        Ok(selection) => {
                            self.playing = false;
                            self.remember_anchor(self.anchor());
                            if !selection.is_empty() {
                                self.set_selection(selection);
                            }
                            self.error = None;
                            if state.selected.is_none() {
                                state.selected = self.editor.document().as_json()["judgeLineList"]
                                    [self.line][trajectories::RECIPE_FIELD]
                                    .as_array()
                                    .and_then(|v| v.last())
                                    .and_then(|v| v["id"].as_str())
                                    .map(str::to_owned);
                            }
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
            }
            Err(error) => {
                state.preview = None;
                ui.colored_label(flick(), crate::i18n::tr(error.to_string()));
            }
        }
        if let Some(id) = state.selected.clone() {
            ui.small(crate::i18n::tr(
                "Detach keeps the events and removes their editable recipe.",
            ));
            ui.horizontal_wrapped(|ui| {
                for (label, delete) in [("Detach trajectory", false), ("Delete trajectory", true)] {
                    if ui.button(crate::i18n::tr(label)).clicked() {
                        match trajectories::detach(self.editor.document(), self.line, &id, delete)
                            .and_then(|e| e.apply(&mut self.editor))
                        {
                            Ok(_) => {
                                state.selected = None;
                                state.preview = None;
                                self.playing = false;
                                if delete {
                                    self.set_selection(Default::default());
                                }
                                self.error = None;
                            }
                            Err(error) => self.error = Some(error.to_string()),
                        }
                    }
                }
            });
        }
        self.trajectory = state;
    }
}

impl TrajectoryState {
    pub(crate) fn load_configuration(&mut self, spec: Spec, name: String) {
        self.start = spec.start.to_string();
        self.end = spec.end.to_string();
        self.step = spec.step.to_string();
        self.spec = spec;
        self.name = name;
        self.selected = None;
        self.preview = None;
    }
}
