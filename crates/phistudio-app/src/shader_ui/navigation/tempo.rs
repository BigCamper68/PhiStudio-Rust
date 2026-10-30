use super::*;

impl Workbench {
    pub(in super::super) fn shader_tempo_ui(&mut self, ui: &mut egui::Ui) {
        ui.strong(crate::i18n::tr("Shader BPM"));
        ui.small(crate::i18n::tr(
            "The grid and all shader intervals use this BPM map.",
        ));
        if self.shaders.tempo_rows.is_none() {
            let bpm = &self.shaders.source["bpm"];
            let rows = if let Some(rows) = bpm.as_array() {
                rows.clone()
            } else {
                vec![json!({"time":[0,0,1],"bpm":bpm})]
            };
            self.shaders.tempo_rows = Some(
                rows.into_iter()
                    .map(|raw| {
                        let beat = serde_json::from_value::<Beat>(raw["time"].clone())
                            .unwrap_or(Beat::ZERO)
                            .to_string();
                        let bpm = raw["bpm"].as_f64().unwrap_or(120.);
                        BpmDraft { raw, beat, bpm }
                    })
                    .collect(),
            );
        }
        let rows = self.shaders.tempo_rows.as_mut().unwrap();
        let mut remove = None;
        let count = rows.len();
        for (i, row) in rows.iter_mut().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.label(crate::i18n::tr("Beat"));
                ui.add(egui::TextEdit::singleline(&mut row.beat).desired_width(55.));
                ui.label(crate::i18n::tr("BPM"));
                ui.add(
                    crate::numeric::Value::new(&mut row.bpm)
                        .speed(0.1)
                        .clamp_range(0.001..=10000.),
                );
                if count > 1 && ui.small_button(crate::i18n::tr("Delete")).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            rows.remove(i);
        }
        let mut add = false;
        let mut apply = false;
        let mut chart = false;
        ui.horizontal_wrapped(|ui| {
            add = ui.button(crate::i18n::tr("Add BPM change")).clicked();
            apply = ui.button(crate::i18n::tr("Apply BPM")).clicked();
            chart = ui.button(crate::i18n::tr("Use chart BPM")).clicked();
        });
        if add {
            let beat = self
                .shaders
                .tempo_rows
                .as_ref()
                .unwrap()
                .last()
                .and_then(|r| r.beat.parse::<Beat>().ok())
                .unwrap_or(Beat::ZERO)
                .checked_add(Beat::ONE)
                .unwrap_or(Beat::ONE);
            self.shaders.tempo_rows.as_mut().unwrap().push(BpmDraft {
                raw: json!({"time":beat,"bpm":120.}),
                beat: beat.to_string(),
                bpm: 120.,
            });
        }
        if apply {
            let result = self
                .shaders
                .tempo_rows
                .as_ref()
                .unwrap()
                .iter()
                .map(|row| {
                    let beat = row.beat.parse::<Beat>()?;
                    let mut raw = row.raw.clone();
                    if serde_json::from_value::<Beat>(raw["time"].clone()).ok() != Some(beat) {
                        raw["time"] = json!(beat);
                    }
                    if raw["bpm"].as_f64() != Some(row.bpm) {
                        raw["bpm"] = json!(row.bpm);
                    }
                    Ok(raw)
                })
                .collect::<Result<Vec<_>, phistudio_core::ChartError>>();
            match result {
                Ok(rows) => {
                    let value = if self.shaders.source["bpm"].is_number()
                        && rows.len() == 1
                        && rows[0]["time"] == json!(Beat::ZERO)
                    {
                        rows[0]["bpm"].clone()
                    } else {
                        json!(rows)
                    };
                    if self.shader_command(ShaderAction::Tempo(value)) {
                        self.shaders.tempo_rows = None;
                        self.refresh_shaders();
                    }
                }
                Err(e) => self.error = Some(e.to_string()),
            }
        }
        if chart
            && self.shader_command(ShaderAction::Tempo(
                default_shader_extra(self.editor.document())["bpm"].clone(),
            ))
        {
            self.shaders.tempo_rows = None;
            self.refresh_shaders();
        }
    }
}
