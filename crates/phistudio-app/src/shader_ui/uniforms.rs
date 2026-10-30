use super::*;
pub(super) fn rgba_value(value: &Value) -> Option<[u8; 4]> {
    if uniform_type(value) != Some(UniformKind::Color) {
        return None;
    }
    Some(std::array::from_fn(|i| value[i].as_u64().unwrap() as u8))
}

pub(super) fn is_animated(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|v| v.first().is_some_and(Value::is_object))
}

pub(super) fn uniform_widget(ui: &mut egui::Ui, value: &mut Value) -> bool {
    let mut changed = false;
    match uniform_type(value) {
        Some(UniformKind::Float) => {
            let mut v = value.as_f64().unwrap();
            changed = ui
                .add(
                    crate::numeric::Value::new(&mut v)
                        .speed(0.01)
                        .max_decimals(5),
                )
                .changed();
            if changed {
                *value = json!(v);
            }
        }
        Some(UniformKind::Vec2) => {
            ui.horizontal_wrapped(|ui| {
                for i in 0..2 {
                    let mut v = value[i].as_f64().unwrap();
                    if ui
                        .add(
                            crate::numeric::Value::new(&mut v)
                                .prefix(crate::i18n::tr(if i == 0 { "X " } else { "Y " }))
                                .speed(0.01)
                                .max_decimals(4),
                        )
                        .changed()
                    {
                        value[i] = json!(v);
                        changed = true;
                    }
                }
            });
        }
        Some(UniformKind::Color) => {
            ui.horizontal_wrapped(|ui| {
                for (i, label) in ["R ", "G ", "B ", "A "].iter().enumerate() {
                    let mut v = value[i].as_u64().unwrap();
                    if ui
                        .add(
                            crate::numeric::Value::new(&mut v)
                                .prefix(*label)
                                .clamp_range(0..=255),
                        )
                        .changed()
                    {
                        value[i] = json!(v);
                        changed = true;
                    }
                }
            });
            let c = egui::Color32::from_rgba_unmultiplied(
                value[0].as_u64().unwrap() as u8,
                value[1].as_u64().unwrap() as u8,
                value[2].as_u64().unwrap() as u8,
                value[3].as_u64().unwrap() as u8,
            );
            let (r, _) = ui.allocate_exact_size(vec2(42., 12.), Sense::hover());
            ui.painter().rect_filled(r, 2., c);
        }
        None => {
            ui.small(crate::i18n::tr("Uses shader default"));
        }
    }
    changed
}
