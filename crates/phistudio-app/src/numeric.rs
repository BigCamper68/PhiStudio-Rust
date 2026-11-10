//! Expressions reuse the native DragValue focus, touch, bounds and commit path.
use egui::{DragValue, Response, Ui, Widget, emath::Numeric};
use std::{cell::Cell, ops::RangeInclusive, rc::Rc};

const HELP: &str = "Enter a number or formula: 675/2, 360/7, sin(pi/4)*300. Angles are radians. Enter applies; Escape cancels.";
const ERROR: &str = "Invalid formula or result. Use a finite number; integer fields require a whole number. The previous value was kept.";

pub(crate) struct Value<'a> {
    get_set: Box<dyn FnMut(Option<f64>) -> f64 + 'a>,
    speed: f64,
    range: RangeInclusive<f64>,
    prefix: String,
    max_decimals: Option<usize>,
    integral: bool,
    preferences: bool,
    parse: fn(&str) -> Option<f64>,
}

fn parse<Num: Numeric>(source: &str) -> Option<f64> {
    let value = phistudio_core::Formula::constant(source).ok()?;
    if Num::INTEGRAL && value.fract() != 0. {
        return None;
    }
    // DragValue applies its configured range after parsing. Also reject f32 overflow.
    Num::from_f64(value).to_f64().is_finite().then_some(value)
}

impl<'a> Value<'a> {
    pub fn new<Num: Numeric>(value: &'a mut Num) -> Self {
        Self {
            get_set: Box::new(move |new| {
                if let Some(new) = new {
                    *value = Num::from_f64(new);
                }
                value.to_f64()
            }),
            speed: if Num::INTEGRAL { 0.25 } else { 1. },
            range: Num::MIN.to_f64()..=Num::MAX.to_f64(),
            prefix: String::new(),
            max_decimals: Num::INTEGRAL.then_some(0),
            integral: Num::INTEGRAL,
            preferences: true,
            parse: parse::<Num>,
        }
    }
    pub fn speed(mut self, speed: impl Into<f64>) -> Self {
        self.speed = speed.into();
        self
    }
    pub fn clamp_range<Num: Numeric>(mut self, range: RangeInclusive<Num>) -> Self {
        self.range = range.start().to_f64()..=range.end().to_f64();
        self
    }
    pub fn prefix(mut self, prefix: impl ToString) -> Self {
        self.prefix = prefix.to_string();
        self
    }
    pub fn max_decimals(mut self, decimals: usize) -> Self {
        self.max_decimals = Some(decimals);
        self
    }

    /// Avoid applying a step/sensitivity setting to its own control.
    /// The shared enable/disable switch still applies to these fields.
    pub fn unconfigured(mut self) -> Self {
        self.preferences = false;
        self
    }
}

pub(crate) fn configure(ctx: &egui::Context, options: phistudio_core::NumericDragOptions) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("numeric-drag-options"), options.normalized()));
}

#[derive(Clone, Copy)]
struct DragState {
    raw: f64,
}

impl Widget for Value<'_> {
    fn ui(mut self, ui: &mut Ui) -> Response {
        let mut options = ui
            .data_mut(|d| {
                d.get_temp::<phistudio_core::NumericDragOptions>(egui::Id::new(
                    "numeric-drag-options",
                ))
            })
            .unwrap_or_default();
        if !self.preferences {
            options.sensitivity = 1.;
            options.snap = false;
        }
        let id = ui.next_auto_id();
        let state_id = id.with("horizontal-numeric-drag");
        let editing = ui.memory(|m| m.has_focus(id));
        let speed = self.speed * options.sensitivity;
        let before = (self.get_set)(None);
        let auto_decimals = ((ui.input(|i| i.aim_radius()) as f64 / self.speed.abs().max(1e-15))
            .log10()
            .ceil()
            .clamp(0., 15.) as usize)
            + usize::from(
                ui.input(|i| i.modifiers.shift_only()) && ui.memory(|m| m.is_being_dragged(id)),
            );
        let decimals = self.max_decimals.unwrap_or((auto_decimals + 2).min(15));
        let shown_decimals = auto_decimals.min(decimals);
        let result = Rc::new(Cell::new(None));
        let parsed = Rc::clone(&result);
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
        let mut response = DragValue::from_get_set(&mut self.get_set)
            // The native widget still owns focus, text editing and IME. Its
            // two-axis drag is replaced below with a horizontal-only gesture.
            .speed(if editing { self.speed } else { 0. })
            .clamp_range(self.range.clone())
            .prefix(&self.prefix)
            .max_decimals(decimals)
            .custom_formatter(move |v, _| {
                if v == 0. {
                    "0".into()
                } else {
                    egui::emath::format_with_decimals_in_range(v, shown_decimals..=decimals)
                }
            })
            .custom_parser(move |source| {
                if escape {
                    return None;
                }
                let value = (self.parse)(source);
                parsed.set(Some(value.is_some()));
                value
            })
            .ui(ui);
        if response.dragged()
            && response.drag_delta().x != 0.
            && options.enabled
            && ui.is_enabled()
            && !editing
        {
            let mut state = ui
                .data_mut(|d| d.get_temp::<DragState>(state_id))
                .unwrap_or(DragState { raw: before });
            if response.drag_started() {
                state.raw = before;
            }
            let slow = ui.input(|i| if i.modifiers.shift_only() { 0.1 } else { 1. });
            state.raw = (state.raw + response.drag_delta().x as f64 * speed * slow)
                .clamp(*self.range.start(), *self.range.end());
            let mut value = state.raw;
            if options.snap {
                let step = if self.integral {
                    options.step.round().max(1.)
                } else {
                    options.step
                };
                value = (value / step).round() * step;
            } else {
                value = egui::emath::round_to_decimals(value, decimals);
            }
            if self.integral {
                value = value.round();
            }
            value = value.clamp(*self.range.start(), *self.range.end());
            if value.is_finite() && value != before {
                (self.get_set)(Some(value));
                response.mark_changed();
                response.output_event(egui::output::OutputEvent::ValueChanged(
                    egui::WidgetInfo::drag_value(value),
                ));
                ui.ctx().request_repaint();
            }
            ui.data_mut(|d| d.insert_temp(state_id, state));
        } else if !response.is_pointer_button_down_on() {
            ui.data_mut(|d| d.remove::<DragState>(state_id));
        }
        if !options.enabled {
            response = response.on_hover_cursor(egui::CursorIcon::Text);
        }
        let error_id = response.id.with("formula-error");
        let invalid = ui.data_mut(|data| {
            let mut invalid = data.get_temp::<bool>(error_id).unwrap_or(false);
            if let Some(valid) = result.get() {
                invalid = !valid;
            } else if escape || response.clicked() || response.changed() {
                invalid = false;
            }
            data.insert_temp(error_id, invalid);
            invalid
        });
        if invalid {
            ui.painter().rect_stroke(
                response.rect,
                2.,
                egui::Stroke::new(1.5_f32, ui.visuals().error_fg_color),
            );
            // Keep errors beside this control, outside global chart diagnostics.
            let error = ui.colored_label(ui.visuals().error_fg_color, crate::i18n::tr(ERROR));
            response = response.union(error);
        } else {
            // A conditional Label consumes an auto ID. Reserve it even without
            // an error so later controls keep their focus/IME identity.
            ui.skip_ahead_auto_ids(1);
        }
        response.on_hover_text(crate::i18n::tr(HELP))
    }
}

#[cfg(test)]
mod tests;
