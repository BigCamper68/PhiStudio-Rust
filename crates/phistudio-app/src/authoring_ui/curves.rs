use super::*;

pub(crate) fn draw_curve(ui: &mut egui::Ui, kind: EventType, event: &LineEvent) {
    let curve = phistudio_prpr::EventCurve::new(kind, event);
    draw_sampled_curve(ui, |t| curve.sample(t as f32));
}
pub(crate) fn draw_sampled_curve(ui: &mut egui::Ui, sample: impl Fn(f64) -> f32) {
    let rect = crate::plot::square(ui);
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 2., base());
    let inner = rect.shrink(8.);
    let values: Vec<_> = (0..=96).map(|i| sample(i as f64 / 96.)).collect();
    let lo = values
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(0., f32::min);
    let hi = values
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(1., f32::max);
    let map = crate::plot::Cartesian::fit(inner, [0., f64::from(lo)], [1., f64::from(hi)]);
    let points: Vec<_> = values
        .iter()
        .enumerate()
        .filter(|(_, v)| v.is_finite())
        .map(|(i, v)| map.point([i as f64 / 96., f64::from(*v)]))
        .collect();
    p.line_segment(
        [inner.left_bottom(), inner.right_bottom()],
        egui::Stroke::new(1_f32, border()),
    );
    p.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.5_f32, accent()),
    ));
}

pub(crate) fn easing_fields(ui: &mut egui::Ui, event: &mut LineEvent) {
    easing_fields_impl(ui, event, true);
}

pub(crate) fn shader_easing_fields(ui: &mut egui::Ui, event: &mut LineEvent) {
    easing_fields_impl(ui, event, false);
}

pub(super) fn easing_fields_impl(ui: &mut egui::Ui, event: &mut LineEvent, allow_bezier: bool) {
    if let Some(binding) = &event.custom_easing {
        ui.strong(crate::i18n::tr("Custom easing"));
        if !binding.curve.name.trim().is_empty() {
            ui.label(&binding.curve.name);
        }
        if ui
            .add(egui::Button::new(crate::i18n::tr("Use standard easing")).wrap(true))
            .clicked()
        {
            event.custom_easing = None;
            event.easing_type = 1;
            event.easing_left = 0.;
            event.easing_right = 1.;
            event.bezier = 0;
        }
        return;
    }
    let form = crate::form::Form::new(
        ui,
        &[
            "Easing",
            "Bezier",
            "Easing left",
            "Easing right",
            "Control 1 X",
            "Control 1 Y",
            "Control 2 X",
            "Control 2 Y",
        ],
    );
    form.field(ui, "Easing", |ui| {
        egui::ComboBox::from_id_source("easing")
            .width(ui.available_width())
            .selected_text(crate::i18n::tr(
                EASINGS
                    .get((event.easing_type - 1).max(0) as usize)
                    .copied()
                    .unwrap_or("Unknown"),
            ))
            .touch_show_ui(ui, |ui| {
                for (i, label) in EASINGS.iter().enumerate() {
                    ui.selectable_value(
                        &mut event.easing_type,
                        i as i32 + 1,
                        format!("{} · {}", i + 1, crate::i18n::tr(label)),
                    );
                }
            });
    });
    if allow_bezier {
        form.field(ui, "Bezier", |ui| {
            let mut bezier = event.bezier != 0;
            if ui.checkbox(&mut bezier, "").changed() {
                event.bezier = u8::from(bezier);
                if bezier && event.bezier_points == [0.; 4] {
                    event.bezier_points = [0.25, 0.25, 0.75, 0.75];
                }
            }
        });
    }
    if event.bezier != 0 && allow_bezier {
        for (i, label) in ["Control 1 X", "Control 1 Y", "Control 2 X", "Control 2 Y"]
            .into_iter()
            .enumerate()
        {
            form.field(ui, label, |ui| {
                ui.add(
                    crate::numeric::Value::new(&mut event.bezier_points[i])
                        .speed(0.01)
                        .max_decimals(4),
                )
            });
        }
    } else {
        for (label, value) in [
            ("Easing left", &mut event.easing_left),
            ("Easing right", &mut event.easing_right),
        ] {
            form.field(ui, label, |ui| {
                ui.add(
                    crate::numeric::Value::new(value)
                        .clamp_range(0. ..=1.)
                        .speed(0.01),
                )
            });
        }
    }
}
