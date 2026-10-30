use super::*;

pub fn default_shader_extra(doc: &ChartDocument) -> Value {
    let bpm = doc.as_json()["BPMList"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| json!({"time":b["startTime"],"bpm":b["bpm"]}))
        .collect::<Vec<_>>();
    json!({"bpm":bpm,"effects":[]})
}

pub fn validate_shader_extra(source: &Value) -> Result<(), ChartError> {
    if !source.is_object() || serde_json::to_vec(source)?.len() > 4 * 1024 * 1024 {
        return Err(invalid(
            "Shader data must be an extra.json object up to 4 MiB",
        ));
    }
    if let Some(bpm) = source["bpm"].as_f64() {
        if !bpm.is_finite() || bpm <= 0. {
            return Err(invalid("Shader BPM must be positive"));
        }
    } else if let Some(rows) = source["bpm"].as_array().filter(|a| !a.is_empty()) {
        for r in rows {
            let _: Beat = serde_json::from_value(r["time"].clone())?;
            if !r["bpm"].as_f64().is_some_and(|n| n.is_finite() && n > 0.) {
                return Err(invalid("Shader BPM must be positive"));
            }
        }
    } else {
        return Err(invalid("extra.json needs a BPM or a nonempty BPM list"));
    }
    if let Some(v) = source.get("effects") {
        let rows = v
            .as_array()
            .ok_or_else(|| invalid("Effects must be a list"))?;
        if rows.len() > 4096 {
            return Err(invalid("Use at most 4,096 shader effects"));
        }
        for r in rows {
            serde_json::from_value::<ShaderEffect>(r.clone())?.validate()?;
        }
    }
    Ok(())
}

pub fn load_shader_extra(doc: &ChartDocument, root: Option<&Path>) -> Result<Value, ChartError> {
    if let Some(value) = doc.as_json().get(EXTRA_KEY) {
        validate_shader_extra(value)?;
        return Ok(value.clone());
    }
    if let Some(root) = root {
        let path = root.join("extra.json");
        if path.exists() {
            let root = root.canonicalize()?;
            let path = path.canonicalize()?;
            if !path.starts_with(&root) {
                return Err(invalid("Shader data leaves the project directory"));
            }
            let mut bytes = Vec::new();
            fs::File::open(path)?
                .take(4 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 4 * 1024 * 1024 {
                return Err(invalid("Shader data exceeds 4 MiB"));
            }
            let value = serde_json::from_slice(&bytes)?;
            validate_shader_extra(&value)?;
            return Ok(value);
        }
    }
    Ok(default_shader_extra(doc))
}

pub fn shader_effects(source: &Value) -> Result<Vec<ShaderEffect>, ChartError> {
    Ok(serde_json::from_value(
        source.get("effects").cloned().unwrap_or(json!([])),
    )?)
}

/// Convert the independent extra.json clock to chart seconds for shared playback.
pub fn shader_tempo(source: &Value) -> Result<crate::TempoMap, ChartError> {
    let changes = if let Some(bpm) = source["bpm"].as_f64() {
        vec![crate::BpmChange {
            start: Beat::ZERO,
            bpm,
        }]
    } else {
        source["bpm"]
            .as_array()
            .ok_or_else(|| invalid("Missing shader tempo"))?
            .iter()
            .map(|r| {
                Ok(crate::BpmChange {
                    start: serde_json::from_value(r["time"].clone())?,
                    bpm: r["bpm"]
                        .as_f64()
                        .ok_or_else(|| invalid("Invalid shader tempo"))?,
                })
            })
            .collect::<Result<Vec<_>, ChartError>>()?
    };
    crate::TempoMap::new(changes)
}

pub(super) fn validate_authored_effect(value: &Value) -> Result<(), ChartError> {
    let effect: ShaderEffect = serde_json::from_value(value.clone())?;
    effect.validate()?;
    for (name, value) in &effect.vars {
        if !uniform_name(name)
            || ["time", "screenSize", "UVScale", "screenTexture"].contains(&name.as_str())
        {
            return Err(invalid(
                "Use a custom GLSL uniform name; built-in inputs are automatic",
            ));
        }
        if let Some(rows) = value
            .as_array()
            .filter(|r| r.first().is_some_and(Value::is_object))
        {
            let kind = uniform_type(&rows[0]["start"]);
            for row in rows {
                if uniform_type(&row["start"]) != kind || uniform_type(&row["end"]) != kind {
                    return Err(invalid(
                        "All keyframes of a uniform must use the same value type",
                    ));
                }
                let easing = row["easingType"].as_i64();
                let left = row.get("easingLeft").map_or(Some(0.), Value::as_f64);
                let right = row.get("easingRight").map_or(Some(1.), Value::as_f64);
                if !easing.is_some_and(|v| (1..=29).contains(&v))
                    || !matches!((left,right), (Some(a),Some(b)) if a>=0. && b<=1. && b>a)
                {
                    return Err(invalid(
                        "Choose easing 1–29 and a positive easing window within 0–1",
                    ));
                }
            }
        }
    }
    Ok(())
}
