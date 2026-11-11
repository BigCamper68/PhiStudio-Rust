use super::*;
use crate::{
    i18n::tr,
    scroll::{TouchCombo as _, TouchScroll as _},
    theme,
};

impl Workbench {
    pub(crate) fn custom_easing_window(&mut self, ctx: &egui::Context) {
        let blocked = self.modal_open_except_custom_easing()
            || self.show_projects
            || self.preview.fullscreen
            || self.playtest.active();
        let mut state = std::mem::take(&mut self.custom_easing);
        if !state.open {
            self.custom_easing = state;
            return;
        }
        if state.request.as_ref().is_none_or(|r| {
            r.key.session != self.session || r.key.revision != self.editor.revision()
        }) {
            state.close();
            self.error = Some("The chart changed. Reopen custom easing before applying".into());
            self.custom_easing = state;
            return;
        }
        if blocked || state.input_canceled || ctx.input(|i| !i.raw.has_focus) {
            state.cancel_gesture();
        }
        if blocked || !ctx.input(|i| i.raw.has_focus) {
            state.navigation = Default::default();
        }
        state.poll();
        if !state.open {
            self.custom_easing = state;
            return;
        }
        let mut open = true;
        let mut cancel = false;
        if !blocked
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && !ctx.wants_keyboard_input()
        {
            if state.pointer.is_some() {
                state.cancel_gesture();
            } else {
                open = false;
            }
        }
        if !blocked && !ctx.wants_keyboard_input() && state.pending.is_none() {
            let undo = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
            let redo = ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
                    egui::Key::Z,
                ) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
            });
            if redo {
                state.history(true);
            } else if undo {
                state.history(false);
            }
            if ctx
                .input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
            {
                state.delete();
            }
        }
        let screen = ctx.screen_rect();
        let compact = screen.height() < 480. || screen.width() < 320.;
        let title = if screen.width() < 320. {
            "Curve"
        } else if compact {
            "Custom easing"
        } else {
            "Custom easing editor"
        };
        crate::window::Window::new(tr(title), egui::Id::new("Custom easing editor"))
            .open(&mut open)
            .default_pos([8., if compact { 0. } else { 32. }])
            .default_width((screen.width() - 32.).clamp(180., 840.))
            .default_height((screen.height() - 64.).clamp(140., 680.))
            .min_width(180.)
            .collapsible(false)
            .resize(|r| r.max_size([(screen.width() - 24.).max(180.), (screen.height() - 52.).max(120.)]))
            .show(ctx, |ui| {
                ui.style_mut().wrap = Some(true);
                if compact { ui.spacing_mut().item_spacing.y = 3.; }
                ui.set_enabled(!blocked);
                ui.add_enabled_ui(state.pending.is_none(), |ui| {
                    toolbar(ui, &mut state, compact);
                    if compact {
                        ui.horizontal_wrapped(|ui| {
                            ui.selectable_value(&mut state.details, false, tr("Curve"));
                            ui.selectable_value(&mut state.details, true, tr("Point details"));
                        });
                        let reserve = if ui.available_width() < 360. { 72. } else { 40. };
                        if state.details {
                            point_actions(ui, &mut state);
                            details(ui, &mut state, (ui.available_height() - reserve).max(32.));
                        } else {
                            let height = (ui.available_height() - reserve).clamp(100., 510.);
                            graph::canvas(ui, &mut state, height);
                        }
                    } else {
                        let reserve = if ui.available_width() < 520. { 244. } else { 158. };
                        let height = (ui.available_height() - reserve).clamp(110., 510.);
                        graph::canvas(ui, &mut state, height);
                        details(ui, &mut state, 112.);
                        ui.horizontal_wrapped(|ui| {
                            ui.toggle_value(&mut state.animate, tr("Preview"));
                            ui.add(egui::Slider::new(&mut state.preview, 0. ..=1.).show_value(false));
                        });
                    }
                });
                if state.animate && state.pending.is_none() && !blocked {
                    state.preview = (state.preview + f64::from(ctx.input(|i| i.stable_dt).min(0.1)) * 0.4) % 1.;
                    ctx.request_repaint();
                }
                if let Some(error) = &state.error {
                    ui.colored_label(theme::flick(), tr(error));
                }
                ui.horizontal_wrapped(|ui| {
                    let button = ui.add_enabled(!blocked && state.pending.is_none() && state.pointer.is_none(), egui::Button::new(tr("Use curve")))
                        .on_hover_text(tr("Use curve updates the inspector draft. Apply the event to save it in the chart."));
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("custom-easing-use"), button.rect));
                    if button.clicked() { state.prepare(); }
                    if ui.button(tr("Cancel")).clicked() { cancel = true; }
                    if compact {
                        ui.toggle_value(&mut state.animate, tr("Preview"));
                    }
                    if state.pending.is_some() {
                        ui.spinner();
                        ui.label(tr("Preparing curve…"));
                        ctx.request_repaint_after(std::time::Duration::from_millis(16));
                    }
                });
            });
        if !open || cancel {
            state.close();
        }
        self.custom_easing = state;
    }
}
fn toolbar(ui: &mut egui::Ui, state: &mut State, compact: bool) {
    ui.horizontal_wrapped(|ui| {
        for (tool, label) in [
            (Tool::Edit, "Select"),
            (Tool::Add, "Add point"),
            (Tool::Pan, "Pan"),
        ] {
            let response = ui.selectable_value(&mut state.tool, tool, tr(label));
            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::new(("custom-easing-tool", label)), response.rect)
            });
        }
        if ui.button(tr("Fit")).clicked() {
            state.view = graph::Bounds::fit(&state.draft.curve);
        }
        if theme::history_button(ui, false, !state.undo.is_empty()).clicked() {
            state.history(false);
        }
        if theme::history_button(ui, true, !state.redo.is_empty()).clicked() {
            state.history(true);
        }
        if !compact {
            point_actions(ui, state);
        }
    });
}
fn point_actions(ui: &mut egui::Ui, state: &mut State) {
    ui.horizontal_wrapped(|ui| {
        let removable = state
            .draft
            .selected
            .iter()
            .any(|i| *i > 0 && *i + 1 < state.draft.curve.nodes.len());
        if ui
            .add_enabled(removable, egui::Button::new(tr("Delete point")))
            .clicked()
        {
            state.delete();
        }
        ui.checkbox(&mut state.multiple, tr("Multiple selection"));
    });
}
fn details(ui: &mut egui::Ui, state: &mut State, height: f32) {
    let before = state.draft.clone();
    let mut next = state.draft.curve.clone();
    let Some(&index) = state
        .draft
        .selected
        .iter()
        .find(|i| **i > 0 && **i + 1 < state.draft.curve.nodes.len())
        .or_else(|| state.draft.selected.first())
    else {
        ui.small(tr("Select a point to edit its position and handles."));
        return;
    };
    let Some(node) = next.nodes.get(index).cloned() else {
        return;
    };
    let mut point = node.point();
    let mut mode = node.mode;
    let fixed = index == 0 || index + 1 == next.nodes.len();
    let mut changed = false;
    let mut failure = None;
    egui::ScrollArea::vertical()
        .id_source("custom-easing-point-details")
        .max_height(height)
        .touch_show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_source("custom-easing-mode")
                    .selected_text(tr(mode.label()))
                    .touch_show_ui(ui, |ui| {
                        for value in Mode::ALL {
                            ui.selectable_value(&mut mode, value, tr(value.label()));
                        }
                    });
                for (axis, name) in [(0, "X"), (1, "Y")] {
                    ui.label(name);
                    changed |= ui
                        .add_enabled(
                            !fixed,
                            crate::numeric::Value::new(&mut point[axis])
                                .speed(0.002)
                                .max_decimals(6),
                        )
                        .changed();
                }
            });
            if changed {
                match next.move_nodes(&state.indices(), point[0] - node.x, point[1] - node.y) {
                    Ok(curve) => next = curve,
                    Err(error) => failure = Some(error.to_string()),
                }
            }
            if state.draft.selected.len() == 1
                && let Ok(material) = next.materialized()
            {
                for (outgoing, handle, label) in [
                    (false, material.nodes[index].incoming, "Incoming handle"),
                    (true, material.nodes[index].outgoing, "Outgoing handle"),
                ] {
                    let Some(mut handle) = handle else { continue };
                    let mut edited = false;
                    ui.push_id(label, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(tr(label));
                            for (axis, name) in [(0, "X"), (1, "Y")] {
                                ui.label(name);
                                edited |= ui
                                    .add(
                                        crate::numeric::Value::new(&mut handle[axis])
                                            .speed(0.002)
                                            .max_decimals(6),
                                    )
                                    .changed();
                            }
                        });
                    });
                    if edited {
                        match next.move_handle(index, outgoing, handle, false) {
                            Ok(curve) => {
                                next = curve;
                                changed = true;
                            }
                            Err(error) => failure = Some(error.to_string()),
                        }
                    }
                }
            }
        });
    if let Some(error) = failure {
        state.error = Some(error);
    }
    if changed && next != state.draft.curve {
        state.numeric_before.get_or_insert(before);
        state.draft.curve = next;
    }
    if !ui.input(|i| i.pointer.any_down()) {
        state.finish_numeric();
    }
    if mode != state.draft.curve.nodes[index].mode {
        state.finish_numeric();
        state.set_mode(mode);
    }
}
