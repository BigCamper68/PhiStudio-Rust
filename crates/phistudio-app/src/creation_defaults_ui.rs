use crate::{i18n::tr, workbench::Workbench};

impl Workbench {
    pub(crate) fn new_line_defaults_ui(&mut self, ui: &mut egui::Ui) {
        ui.push_id("new-line-defaults", |ui| {
            let content = egui::CollapsingHeader::new(tr("New line defaults"))
                .id_source("new-line-defaults-body")
                .show(ui, |ui| {
                    line_defaults_controls(ui, &mut self.options.new_line_defaults);
                });
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::new("new-line-defaults-header"),
                    content.header_response.rect,
                );
                data.insert_temp(
                    egui::Id::new("new-line-defaults-open"),
                    content.body_returned.is_some(),
                );
            });
        });
    }
}

fn line_defaults_controls(ui: &mut egui::Ui, defaults: &mut phistudio_core::LineCreationDefaults) {
    ui.small(tr(
        "Initial events for new lines only. Existing lines and copies keep their values.",
    ));
    let form = crate::form::Form::new(ui, &["Move X", "Move Y", "Rotate", "Alpha", "Speed"]);
    let mut fields = Vec::new();
    for (label, value, speed) in [
        ("Move X", &mut defaults.x, 1.),
        ("Move Y", &mut defaults.y, 1.),
        ("Rotate", &mut defaults.rotation, 1.),
        ("Alpha", &mut defaults.alpha, 1.),
        ("Speed", &mut defaults.speed, 0.1),
    ] {
        let bounds = if label == "Alpha" {
            -255. ..=255.
        } else {
            -1e9..=1e9
        };
        let response = form.field(ui, label, |ui| {
            ui.add(
                crate::numeric::Value::new(value)
                    .speed(speed)
                    .clamp_range(bounds),
            )
        });
        fields.push(response.rect);
    }
    let reset = ui.button(tr("Reset defaults"));
    ui.ctx().data_mut(|data| {
        data.insert_temp(egui::Id::new("new-line-default-fields"), fields);
        data.insert_temp(egui::Id::new("reset-new-line-defaults"), reset.rect);
    });
    if reset.clicked() {
        *defaults = Default::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phistudio_core::{ChartDocument, Command, EventType, LineCreationDefaults};
    fn defaults() -> LineCreationDefaults {
        LineCreationDefaults {
            x: 125.,
            y: -75.,
            rotation: 34.,
            alpha: 128.,
            speed: 12.,
        }
    }
    #[test]
    fn stored_preferences_drive_single_and_batch_creation_across_editor_sessions() {
        let root = tempfile::tempdir().unwrap();
        let doc = ChartDocument::empty("Defaults", 120.).unwrap();
        let original = doc.as_json().clone();
        let mut first = Workbench::new(doc.clone(), None);
        let mut store = crate::settings::Preferences::new(Some(root.path().to_owned()));
        store.update(&mut first);
        first.options.new_line_defaults = defaults();
        store.flush(&first).unwrap();
        let mut second = Workbench::new(doc, None);
        crate::settings::Preferences::new(Some(root.path().to_owned())).update(&mut second);
        assert_eq!(second.options.new_line_defaults, defaults());
        second.execute(Command::AddLines { count: 3 });
        assert!(second.error.is_none());
        assert_eq!(second.editor.document().line_count(), 4);
        for line in 1..4 {
            assert_eq!(
                second
                    .editor
                    .document()
                    .event_values(line, 0, EventType::MoveX)
                    .unwrap()[0]["start"],
                125.
            );
        }
        second.history_step_now(false);
        assert_eq!(second.editor.document().as_json(), &original);
        second.execute(Command::AddLine {
            name: "Named".into(),
        });
        assert!(second.error.is_none());
        assert_eq!(
            second.editor.document().as_json()["judgeLineList"][1]["Name"],
            "Named"
        );
        assert_eq!(
            second
                .editor
                .document()
                .event_values(1, 0, EventType::Speed)
                .unwrap()[0]["start"],
            12.
        );
    }
}
