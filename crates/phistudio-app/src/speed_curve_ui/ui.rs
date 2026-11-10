use super::*;
use crate::{
    i18n::{format, tr},
    scroll::{TouchCombo as _, TouchScroll as _},
};
use phistudio_core::Command;
impl Workbench {
    pub(crate) fn speed_curve_window(&mut self, ctx: &egui::Context) {
        let blocked = self.modal_open_except_speed_curve()
            || self.show_projects
            || self.preview.fullscreen
            || self.playtest.active();
        let id = self.speed_curve_note();
        let mut state = std::mem::take(&mut self.speed_curve);
        if !state.open {
            let _ = state.poll();
            self.speed_curve = state;
            return;
        }
        state.input_canceled |= ctx.input(|i| {
            !i.raw.has_focus
                || i.events.iter().any(|e| {
                    matches!(
                        e,
                        egui::Event::Touch {
                            phase: egui::TouchPhase::Cancel,
                            ..
                        }
                    )
                })
        });
        if state.input_canceled || blocked {
            state.cancel();
        }
        if let Some(id) = id {
            let target = Target {
                session: self.session,
                revision: self.editor.revision(),
                id,
                modern: self.project_options.use_rpe_170_speed,
            };
            let horizon = state
                .native_clock
                .filter(|c| c.0 == target.session && c.1 == target.revision)
                .map(|c| c.2);
            if state.clock_required && horizon.is_none() {
                if !state.clock_waiting {
                    state.cancel();
                    state.serial = state.serial.wrapping_add(1);
                    state.queued = None;
                    state.source = None;
                    state.profile = None;
                    state.error = None;
                }
                state.clock_waiting = true;
            } else if state.target != Some(target) || state.clock_waiting {
                state.clock_waiting = false;
                let different = state
                    .target
                    .is_none_or(|old| old.id != id || old.session != target.session);
                state.cancel();
                state.serial = state.serial.wrapping_add(1);
                state.queued = None;
                state.target = Some(target);
                state.profile = None;
                match SpeedCurveSource::capture(self.editor.document(), id) {
                    Ok(mut source) => {
                        source.native_horizon = horizon;
                        if different
                            || state
                                .source
                                .as_ref()
                                .is_some_and(|old| old.note.start_time != source.note.start_time)
                        {
                            state.selected = None;
                            state.range = source
                                .recipe(state.layer)
                                .map(|r| r.range)
                                .unwrap_or_else(|| {
                                    let hit = source.note.start_time.as_f64();
                                    [
                                        Beat::from_grid((hit - 8.).max(0.), 1920)
                                            .unwrap_or(Beat::ZERO),
                                        Beat::from_grid((hit + 2.).max(1.), 1920)
                                            .unwrap_or(Beat::ONE),
                                    ]
                                });
                            if let Some(horizon) = source.native_horizon {
                                let end = self.editor.document().tempo().beat_at(horizon);
                                if let Ok(end) = Beat::from_grid(end, 192_000)
                                    .and_then(|b| b.checked_sub(Beat::new(0, 1, 1920)?))
                                    && end > state.range[0]
                                {
                                    state.range[1] = state.range[1].min(end);
                                }
                            }
                            expand_nonlinear_range(&source, state.layer, &mut state.range);
                            state.range_text = state.range.map(|b| b.to_string());
                            state.fit_view = true;
                        }
                        state.source = Some(Arc::new(source));
                        state.load();
                    }
                    Err(error) => {
                        state.source = None;
                        state.error = Some(error.to_string());
                    }
                }
            }
        } else {
            state.cancel();
            if state.target.is_some() {
                state.serial = state.serial.wrapping_add(1);
                state.queued = None;
            }
            state.target = None;
            state.source = None;
            state.profile = None;
        }
        let apply = state.poll();
        let mut open = true;
        let mut history = None;
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if escape && !blocked {
            if state.drag.is_some() || ctx.wants_keyboard_input() {
                state.cancel();
            } else {
                open = false;
            }
        }
        crate::i18n::window("Note speed curve")
            .open(&mut open)
            .default_width(820.)
            .default_height((ctx.screen_rect().height() - 110.).clamp(200., 680.))
            .default_pos([16., 32.])
            .resize(|r| {
                r.max_size([
                    (ctx.screen_rect().width() - 32.).max(180.),
                    (ctx.screen_rect().height() - 80.).max(160.),
                ])
            })
            .show(ctx, |ui| {
                ui.style_mut().wrap = Some(true);
                ui.set_enabled(!blocked && apply.is_none());
                if id.is_none() {
                    ui.label(tr("Select exactly one note to edit its speed curve."));
                    return;
                }
                if let Some(id) = id {
                    ui.strong(format(
                        "Line {0} · Note {1}",
                        &[(id.line + 1).to_string(), (id.index + 1).to_string()],
                    ));
                }
                ui.small(tr(
                    "Speed events belong to the judge line and affect all its notes.",
                ));
                ui.horizontal_wrapped(|ui| {
                    for (mode, label) in [
                        (Mode::Edit, "Edit point"),
                        (Mode::Add, "Add point"),
                        (Mode::Delete, "Delete point"),
                    ] {
                        if ui
                            .selectable_value(&mut state.mode, mode, tr(label))
                            .clicked()
                        {
                            state.cancel();
                        }
                    }
                    ui.label(tr("Layer"));
                    let old = state.layer;
                    egui::ComboBox::from_id_source("speed-curve-layer")
                        .width(56.)
                        .selected_text(state.layer.to_string())
                        .touch_show_ui(ui, |ui| {
                            for layer in 0..4 {
                                ui.selectable_value(&mut state.layer, layer, layer.to_string());
                            }
                        });
                    if old != state.layer {
                        state.selected = None;
                        if let Some(source) = &state.source {
                            expand_nonlinear_range(source, state.layer, &mut state.range);
                            state.range_text = state.range.map(|b| b.to_string());
                        }
                        state.load();
                    }
                    if ui.button(tr("Fit graph")).clicked() {
                        state.fit_view = true;
                        if let Some(profile) = &state.profile {
                            state.start = profile.range[0].as_f64();
                            state.span = profile.range[1].as_f64() - state.start;
                            state.limit = profile
                                .samples
                                .iter()
                                .map(|p| p[1].abs())
                                .fold(100., f64::max)
                                * 1.15;
                            state.fit_view = false;
                        }
                    }
                    if ui.button(tr("Reload")).clicked() {
                        if state.source.is_some() {
                            state.load();
                        } else {
                            state.target = None;
                        }
                    }
                    if ui
                        .button(tr(if self.transport.playing {
                            "Pause"
                        } else {
                            "Play"
                        }))
                        .clicked()
                    {
                        self.transport.playing = !self.transport.playing;
                    }
                });
                let height = (ui.available_height() - 145.).clamp(100., 360.);
                if state.profile.is_some() {
                    let changed = graph::canvas(
                        ui,
                        &mut state,
                        self.timeline.division,
                        self.transport.beat,
                        height,
                    );
                    if changed {
                        self.transport.playing = false;
                    }
                } else {
                    ui.allocate_ui(egui::vec2(ui.available_width(), height), |ui| {
                        if state.worker.is_some() || state.clock_waiting {
                            ui.spinner();
                            ui.label(tr("Loading speed curve…"));
                        }
                    });
                }
                egui::ScrollArea::vertical()
                    .id_source("speed-curve-controls")
                    .max_height(ui.available_height().max(64.))
                    .touch_show_y(ui, |ui| {
                        if let Some(error) = &state.error {
                            ui.colored_label(crate::theme::flick(), tr(error));
                        }
                        curve_controls(ui, &mut state);
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(
                                    self.editor.can_undo() && !state.commit_pending,
                                    egui::Button::new(tr("Undo")),
                                )
                                .clicked()
                            {
                                state.cancel();
                                history = Some(false);
                            }
                            if ui
                                .add_enabled(
                                    self.editor.can_redo() && !state.commit_pending,
                                    egui::Button::new(tr("Redo")),
                                )
                                .clicked()
                            {
                                state.cancel();
                                history = Some(true);
                            }
                            if state.worker.is_some() || state.queued.is_some() {
                                ui.spinner();
                                ui.small(tr("Updating speed curve…"));
                            }
                        });
                    });
            });
        if !ctx.wants_keyboard_input() && !blocked && !state.commit_pending {
            if ctx
                .input(|i| (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::Z))
            {
                state.cancel();
                history = Some(ctx.input(|i| i.modifiers.shift));
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
                self.transport.playing = !self.transport.playing;
            }
            if ctx
                .input(|i| (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::S))
            {
                self.save_action();
            }
            if ctx
                .input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
            {
                state.delete_selected();
            }
        }
        if !open {
            state.close();
        }
        self.speed_curve = state;
        if let Some(edit) = apply
            && self.speed_curve.open
            && self.session == self.speed_curve.target.map_or(u64::MAX, |t| t.session)
        {
            self.execute(Command::EditSpeedCurve(edit));
        }
        if let Some(redo) = history {
            self.history_step(redo);
        }
    }
}
impl State {
    pub(super) fn delete_selected(&mut self) {
        let Some(index) = self
            .selected
            .filter(|i| self.points.get(*i).is_some_and(|p| !p.fixed))
        else {
            return;
        };
        self.points.remove(index);
        if index > 0 {
            self.points[index - 1].easing = Some(1);
        }
        self.selected = None;
        self.drag = None;
        self.request_fit(true);
    }
}
fn curve_controls(ui: &mut egui::Ui, state: &mut State) {
    if let Some(index) = state.selected.filter(|i| *i < state.points.len()) {
        let original = state.points.clone();
        let mut preview = false;
        let mut commit = false;
        ui.add_enabled_ui(!state.busy(), |ui| {
            ui.horizontal_wrapped(|ui| {
                let fixed = state.points[index].fixed;
                ui.label(tr("Beat"));
                let mut beat = state.points[index].beat.as_f64();
                let r = ui.add_enabled(
                    !fixed,
                    crate::numeric::Value::new(&mut beat)
                        .speed(0.25)
                        .max_decimals(6),
                );
                if r.changed()
                    && let Ok(beat) = Beat::from_grid(beat, 1920)
                {
                    let left = index.checked_sub(1).map(|i| state.points[i].beat);
                    let right = state.points.get(index + 1).map(|p| p.beat);
                    if left.is_none_or(|l| beat > l) && right.is_none_or(|r| beat < r) {
                        state.points[index].beat = beat;
                        preview = true;
                    }
                }
                commit |= r.drag_released() || r.lost_focus();
                ui.label(tr("Distance"));
                let r = ui.add_enabled(
                    !fixed,
                    crate::numeric::Value::new(&mut state.points[index].distance)
                        .speed(5.)
                        .clamp_range(-1e9..=1e9)
                        .max_decimals(3),
                );
                preview |= r.changed();
                commit |= r.drag_released() || r.lost_focus();
                if ui
                    .add_enabled(!fixed, egui::Button::new(tr("Delete point")))
                    .clicked()
                {
                    state.delete_selected();
                }
            });
            if index < state.points.len() {
                let mut easing = state.points[index].easing;
                let enabled = index + 1 < state.points.len()
                    && state.source.as_ref().is_none_or(|s| {
                        s.note.kind != phistudio_core::NoteKind::Hold
                            || state.points[index].beat < s.note.start_time
                    });
                ui.add_enabled_ui(enabled, |ui| {
                    ui.label(tr("Outgoing distance easing"));
                    egui::ComboBox::from_id_source("speed-distance-easing")
                        .selected_text(
                            easing
                                .and_then(|e| crate::authoring_ui::EASINGS.get((e - 1) as usize))
                                .map_or_else(|| tr("Original curve"), tr),
                        )
                        .touch_show_ui(ui, |ui| {
                            ui.selectable_value(&mut easing, None, tr("Original curve"));
                            for (i, label) in crate::authoring_ui::EASINGS.iter().enumerate() {
                                ui.selectable_value(
                                    &mut easing,
                                    Some(i as i32 + 1),
                                    format!("{} · {}", i + 1, tr(label)),
                                );
                            }
                        });
                });
                if easing != state.points[index].easing {
                    state.points[index].easing = easing;
                    preview = true;
                    commit = true;
                }
            }
        });
        if preview {
            if state.drag.is_none() {
                state.drag = Some(original);
            }
            state.request_fit(false);
        }
        if commit && state.drag.take().is_some() {
            state.request_fit(true);
        }
    } else {
        ui.small(tr(
            "Select a point. Add or delete points with the graph tools.",
        ));
    }
    if let Some(point) = state.selected.and_then(|i| state.points.get(i))
        && let Some(profile) = state.preview.as_ref().or(state.profile.as_deref())
        && let Some((_, total, layer)) = profile.speeds.iter().find(|p| p.0 == point.beat)
    {
        ui.small(format(
            "Line speed: {0} · Layer speed: {1}",
            &[format!("{total:.3}"), format!("{layer:.3}")],
        ));
    }
    ui.collapsing(tr("Graph view and snapping"), |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut state.snap, tr("Snap distance"));
            ui.label(tr("Distance step"));
            ui.add(
                crate::numeric::Value::new(&mut state.snap_step)
                    .speed(1.)
                    .clamp_range(0.001..=1e6),
            );
            ui.label(tr("Distance range"));
            ui.add(
                crate::numeric::Value::new(&mut state.limit)
                    .speed(50.)
                    .clamp_range(1. ..=1e9),
            );
            if ui.button("−").clicked() {
                state.span =
                    (state.span * 1.5).min(state.range[1].as_f64() - state.range[0].as_f64());
            }
            if ui.button("+").clicked() {
                state.span = (state.span / 1.5)
                    .max((state.range[1].as_f64() - state.range[0].as_f64()).min(0.05));
            }
        });
        let low = state.range[0].as_f64();
        let high = (state.range[1].as_f64() - state.span).max(low);
        state.start = state.start.clamp(low, high);
        ui.add(egui::Slider::new(&mut state.start, low..=high).text(tr("Pan graph")));
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("Work range"));
            for text in &mut state.range_text {
                ui.add(egui::TextEdit::singleline(text).desired_width(80.));
            }
            if ui
                .add_enabled(
                    !state.commit_pending && state.source.is_some(),
                    egui::Button::new(tr("Load range")),
                )
                .clicked()
            {
                match (
                    state.range_text[0].parse::<Beat>(),
                    state.range_text[1].parse::<Beat>(),
                ) {
                    (Ok(a), Ok(b)) if a >= Beat::ZERO && b > a => {
                        state.range = [a, b];
                        state.fit_view = true;
                        state.selected = None;
                        state.load();
                    }
                    _ => state.error = Some("Choose an ordered, nonnegative work range".into()),
                }
            }
        });
    });
}

// Do not truncate an imported nonlinear event merely to establish the initial view.
fn expand_nonlinear_range(source: &SpeedCurveSource, layer: usize, range: &mut [Beat; 2]) {
    let Some(events) = source.layers.get(layer) else {
        return;
    };
    for raw in events {
        if raw["bezier"].as_u64().unwrap_or(0) == 0 && raw["easingType"].as_i64().unwrap_or(1) <= 1
        {
            continue;
        }
        let Ok(event) = phistudio_core::LineEvent::read(phistudio_core::EventType::Speed, raw)
        else {
            continue;
        };
        if event.start == event.end {
            continue;
        }
        if event.start_time < range[0] && event.end_time > range[0] {
            range[0] = event.start_time.max(Beat::ZERO);
        }
        if event.start_time < range[1] && event.end_time > range[1] {
            range[1] = event.end_time;
        }
    }
}
