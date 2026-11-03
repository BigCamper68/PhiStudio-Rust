//! Native Noise Fields, shader metadata and foreign extension diagnostics.
use super::*;

impl Scan {
    pub(super) fn inspect_aux(
        &mut self,
        value: &Value,
        path: &str,
        report: &mut Report,
        target: &str,
    ) {
        self.resources.collect(value, path, &self.base());
        let mut root = json!({});
        root[path.trim_start_matches('/')] = value.clone();
        resources::compatibility(&root, "", target, report);
        if path == "/phiStudioExtra" {
            let bpms = if let Some(bpm) = number(&value["bpm"]) {
                vec![json!({"startTime":[0,0,1],"bpm":bpm})]
            } else {
                value["bpm"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|p| json!({"startTime":p["time"],"bpm":p["bpm"]}))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let mut validity = Report::default();
            let clock = Clock::new(&bpms, &mut validity);
            if validity.counts[0] == 0 {
                let original = std::mem::replace(&mut self.clock, clock);
                self.inspect_aux(&value["effects"], "/phiStudioExtra/effects", report, target);
                self.clock = original;
            }
            return;
        }
        if path == "/blockAreaList" {
            if let Some(fields) = value.as_array() {
                for (index, field) in fields.iter().enumerate() {
                    let source = format!("{path}/{index}");
                    let native = field.get("bottomLeftPercentage").is_some();
                    let seconds = |value: &Value| {
                        if native {
                            number(value)
                        } else {
                            beat(value).and_then(|b| self.clock.seconds(b))
                        }
                    };
                    if let (Some(start), Some(end)) = (
                        seconds(&field["appearTime"]),
                        seconds(&field["disappearTime"]),
                    ) && self.bounds.outside(start, end, true)
                    {
                        report.push(
                            Severity::Error,
                            "audio_range",
                            &source,
                            None,
                            "Noise Field lifetime is outside the available music",
                            false,
                        )
                    }
                    for key in [
                        "moveEvents",
                        "scaleEvents",
                        "rotateEvents",
                        "activeIntervals",
                        "moveXEvents",
                        "moveYEvents",
                        "scaleXEvents",
                        "scaleYEvents",
                        "phiStudioColorEvents",
                    ] {
                        if let Some(events) = field[key].as_array() {
                            for (index, event) in events.iter().enumerate() {
                                if let (Some(start), Some(end)) =
                                    (seconds(&event["startTime"]), seconds(&event["endTime"]))
                                    && self.bounds.outside(start, end, true)
                                {
                                    report.push(
                                        Severity::Error,
                                        "audio_range",
                                        &format!("{source}/{key}/{index}"),
                                        None,
                                        "Noise Field animation is outside the available music",
                                        false,
                                    )
                                }
                            }
                        }
                    }
                }
            }
            return;
        }
        if path == "/noiseAreaLayers"
            && let Some(layers) = value.as_array()
        {
            for (index, layer) in layers.iter().enumerate() {
                if let Some(skin) = layer["skin"].as_str().filter(|s| !s.is_empty()) {
                    let source = format!("{path}/{index}");
                    self.resources
                        .add(&format!("{skin}/config.json"), &source, &self.base(), true);
                    self.resources
                        .add(&format!("{skin}/main.glsl"), &source, &self.base(), false);
                }
            }
        }
        if path == "/globalEvents" && value["types"].as_object().is_some_and(|v| !v.is_empty()) {
            report.push(Severity::Warning,"foreign_global_schema",path,None,"rpe-next global-event providers and JavaScript validators are not installed as PhiStudio plugins; these rules require compatible providers",false);
            if let Some(types) = value["types"].as_object() {
                for (id, definition) in types {
                    if definition["source"]["kind"] == "mod" {
                        let provider = definition["source"]["id"].as_str().unwrap_or(id);
                        let installed = self
                            .project
                            .as_ref()
                            .and_then(|p| p.plugins.iter().find(|p| p.id == provider));
                        if installed.is_none() {
                            report.push(
                                Severity::Warning,
                                "mod_missing",
                                &format!("{path}/types/{}", escape(id)),
                                None,
                                format!("Global-event provider is missing: {provider}"),
                                false,
                            )
                        } else if let Some(installed) = installed
                            && definition["source"]["version"]
                                .as_str()
                                .is_some_and(|v| v != installed.version.to_string())
                        {
                            report.push(
                                Severity::Warning,
                                "mod_version",
                                &format!("{path}/types/{}", escape(id)),
                                None,
                                format!(
                                    "Global-event provider version differs: installed {}",
                                    installed.version
                                ),
                                false,
                            );
                        }
                    }
                }
            }
        }
        let mut stack = vec![(value, path.to_owned())];
        while let Some((value, path)) = stack.pop() {
            inspect_aux_interval(value, &path, &self.clock, &self.bounds, report);
            match value {
                Value::Array(a) => {
                    for (i, v) in a.iter().enumerate() {
                        stack.push((v, format!("{path}/{i}")))
                    }
                }
                Value::Object(o) => {
                    for (k, v) in o {
                        stack.push((v, format!("{path}/{}", escape(k))))
                    }
                }
                _ => {}
            }
        }
    }
}
