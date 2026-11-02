//! Snapshot-only Phira Pro Line2Area export for colored fields. Source hashes
//! allow exact reconstruction on reimport without overwriting externally edited lines.
use super::*;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};
pub const SOURCE: &str = "phiStudioNoiseSource";
fn hash(value: &Value) -> Result<String, ChartError> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
fn event(a: f64, b: f64, x: Value, y: Value, tempo: &TempoMap) -> Result<Value, ChartError> {
    Ok(
        json!({"startTime":Beat::from_grid(tempo.beat_at(a),1_000_000_000)?,"endTime":Beat::from_grid(tempo.beat_at(b),1_000_000_000)?,"start":x,"end":y,"easingType":1}),
    )
}
fn values(field: &Field, t: f64) -> Option<[f64; 8]> {
    let s = field.sample(t, 1.5)?;
    let color = field.color_values(t);
    Some([
        s.pose.center.x as f64 * 90.,
        s.pose.center.y as f64 * 90.,
        -(s.pose.angle as f64).to_degrees(),
        s.pose.size.x as f64 * 90.,
        s.pose.size.y as f64 * 90.,
        color[0],
        color[1],
        color[2],
    ])
}
fn refine(
    field: &Field,
    a: f64,
    b: f64,
    x: [f64; 8],
    y: [f64; 8],
    out: &mut Vec<(f64, [f64; 8])>,
    depth: usize,
) -> Result<(), ChartError> {
    if out.len() > 200_000 {
        return Err(invalid(
            "Noise Field export exceeds the geometry sample limit",
        ));
    }
    let m = (a + b) * 0.5;
    let Some(v) = values(field, m) else {
        out.push((b, y));
        return Ok(());
    };
    let error = (0..8)
        .map(|i| {
            let e = (v[i] - (x[i] + y[i]) * 0.5).abs();
            if i == 2 {
                e.to_radians() * v[3].max(v[4])
            } else if i >= 5 {
                e * 0.02
            } else {
                e
            }
        })
        .fold(0., f64::max);
    if depth < 20 && b - a > 1e-6 && (error > 0.005 || (b - a > 1. / 60. && x != y)) {
        refine(field, a, m, x, v, out, depth + 1)?;
        refine(field, m, b, v, y, out, depth + 1)?;
    } else {
        out.push((b, y));
    }
    Ok(())
}
/// The caller runs package preparation on its existing file worker.
pub fn prepare(
    document: &ChartDocument,
    existing: &[PathBuf],
) -> Result<(Value, BTreeMap<PathBuf, Vec<u8>>), ChartError> {
    let rows = fields(document)?;
    let mut chart = document.serialization_value().into_owned();
    let mut resources = BTreeMap::new();
    if !rows
        .iter()
        .any(|v| color(v) != DEFAULT_COLOR || !keys(v, Track::Color).unwrap_or(&[]).is_empty())
    {
        return Ok((chart, resources));
    }
    let mut suffix = 0;
    let folder = loop {
        let name = if suffix == 0 {
            "phistudio-noise-fields".into()
        } else {
            format!("phistudio-noise-fields-{suffix}")
        };
        if !existing.iter().any(|p| p.starts_with(&name)) {
            break name;
        }
        suffix += 1;
    };
    let base = chart["judgeLineList"]
        .as_array()
        .ok_or_else(|| invalid("Chart judge lines must be an array"))?
        .len();
    let template = ChartDocument::empty("Noise export", 120.)?.root["judgeLineList"][0].clone();
    let tempo = document.tempo();
    let mut generated = Vec::new();
    for (i, raw) in rows.iter().enumerate() {
        let f = format(raw)?;
        let start = time(raw, "appearTime", f, tempo)?;
        let end = time(raw, "disappearTime", f, tempo)?;
        if f == Format::Native
            && !(start <= number(&raw["enableTime"])? && number(&raw["disableTime"])? <= end)
        {
            return Err(invalid(
                "Phira Pro colored export requires phases within the field lifetime",
            ));
        }
        if start >= end {
            return Err(invalid(format!(
                "Noise Field {} needs a positive lifetime for Phira Pro export",
                i + 1
            )));
        }
        let field = Field::new(raw, tempo)?;
        let mut cuts = vec![start, end];
        if f == Format::Native {
            cuts.extend([number(&raw["enableTime"])?, number(&raw["disableTime"])?]);
        }
        for track in Track::all(f) {
            for key in keys(raw, *track)? {
                if track.native(f) {
                    let t = number(&key["time"])?;
                    cuts.push(t);
                    let before = (t as f32).next_down() as f64;
                    cuts.push(before);
                } else {
                    cuts.extend([
                        time(key, "startTime", Format::Next, tempo)?,
                        time(key, "endTime", Format::Next, tempo)?,
                        time(key, "startTime", Format::Next, tempo)? - 1e-7,
                    ]);
                }
            }
        }
        cuts.retain(|t| *t >= start && *t <= end);
        cuts.sort_by(f64::total_cmp);
        cuts.dedup();
        let final_time = (end as f32).next_down() as f64;
        let at = |t: f64| {
            values(&field, t.min(final_time))
                .ok_or_else(|| invalid("Noise Field has no visible geometry"))
        };
        let first = at(start)?;
        let mut points = vec![(start, first)];
        for cut in cuts.windows(2) {
            refine(
                &field,
                cut[0],
                cut[1],
                at(cut[0])?,
                at(cut[1])?,
                &mut points,
                0,
            )?;
        }
        let mut tracks = Vec::new();
        for channel in 0..5 {
            let mut events = Vec::new();
            for pair in points.windows(2) {
                events.push(event(
                    pair[0].0,
                    pair[1].0,
                    json!(pair[0].1[channel]),
                    json!(pair[1].1[channel]),
                    tempo,
                )?);
            }
            tracks.push(json!(events));
        }
        let mut speed = Vec::new();
        if f == Format::Native {
            for (code, key) in PHASES.iter().enumerate() {
                speed.push(event(
                    number(&raw[*key])?,
                    number(&raw[*key])?,
                    json!(code + 1),
                    json!(code + 1),
                    tempo,
                )?);
            }
        } else {
            speed.push(event(start, start, json!(1), json!(1), tempo)?);
            let mut intervals = keys(raw, Track::Active)?
                .iter()
                .map(|key| {
                    Ok((
                        time(key, "startTime", Format::Next, tempo)?.max(start),
                        time(key, "endTime", Format::Next, tempo)?.min(end),
                        key,
                    ))
                })
                .collect::<Result<Vec<_>, ChartError>>()?;
            intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut previous_end = None;
            for (a, b, key) in intervals {
                if a >= b {
                    continue;
                }
                let ready = match key.get("readyFlash") {
                    Some(Value::Bool(false)) => 0.,
                    Some(Value::Number(n)) => {
                        (n.as_f64().unwrap_or(0.).max(0.) * 1000.).round() / 1000.
                    }
                    _ => 0.5,
                };
                if ready != 0.5 && a - start > ready.min(0.5) {
                    return Err(invalid(
                        "Phira Pro colored export requires a 0.5-second Ready duration",
                    ));
                }
                if let Some(previous) = previous_end {
                    speed.push(event(previous, previous, json!(1), json!(1), tempo)?);
                }
                previous_end = Some(b);
                speed.push(event(a, a, json!(2), json!(2), tempo)?);
                speed.push(event(b, b, json!(3), json!(3), tempo)?);
            }
            speed.push(event(end, end, json!(4), json!(4), tempo)?);
        }
        let color_events: Vec<_> = points
            .windows(2)
            .map(|pair| {
                event(
                    pair[0].0,
                    pair[1].0,
                    json!(std::array::from_fn::<_, 3, _>(|i| pair[0].1[5 + i]
                        .round()
                        .clamp(0., 255.)
                        as u8)),
                    json!(std::array::from_fn::<_, 3, _>(|i| pair[1].1[5 + i]
                        .round()
                        .clamp(0., 255.)
                        as u8)),
                    tempo,
                )
            })
            .collect::<Result<_, _>>()?;
        let mut line = template.clone();
        line["Name"] = json!(format!("Noise Field {}", i + 1));
        line["Texture"] = json!(format!(
            "{folder}/isSubtract{}.png",
            u8::from(
                raw[if f == Format::Native {
                    "isSubtract"
                } else {
                    "isInvert"
                }]
                .as_bool()
                .unwrap_or(false)
            )
        ));
        line["father"] = json!(-1);
        line["bpmfactor"] = json!(1);
        line["zOrder"] = json!(i);
        line["anchor"] = json!([0.5, 0.5]);
        line["notes"] = json!([]);
        line["numOfNotes"] = json!(0);
        line["eventLayers"] = json!([{"moveXEvents":tracks[0],"moveYEvents":tracks[1],"rotateEvents":tracks[2],"alphaEvents":[{"startTime":[0,0,1],"endTime":[0,0,1],"start":255,"end":255}],"speedEvents":speed}]);
        line["extended"] =
            json!({"scaleXEvents":tracks[3],"scaleYEvents":tracks[4],"colorEvents":color_events});
        generated.push(line);
    }
    chart[SOURCE] = json!({"version":1,"baseLineCount":base,"fields":rows,"lineHash":hash(&json!(generated))?,"bpmHash":hash(&chart["BPMList"])?});
    chart["judgeLineList"]
        .as_array_mut()
        .unwrap()
        .extend(generated);
    chart[LIST] = json!([]);
    const GUIDE: &[u8] = include_bytes!("../../../../assets/noise-fields/guide.png");
    for i in [0, 1] {
        resources.insert(
            PathBuf::from(format!("{folder}/isSubtract{i}.png")),
            GUIDE.to_vec(),
        );
    }
    Ok((chart, resources))
}
pub(crate) fn restore(root: &mut Value) -> Result<(), ChartError> {
    let Some(source) = root.get(SOURCE) else {
        return Ok(());
    };
    let (Some(base), Some(rows)) = (
        source["baseLineCount"].as_u64(),
        source["fields"].as_array(),
    ) else {
        return Ok(());
    };
    if source["version"] != 1 || root[LIST].as_array().is_some_and(|v| !v.is_empty()) {
        return Ok(());
    }
    let Some(lines) = root["judgeLineList"].as_array() else {
        return Ok(());
    };
    let base = base as usize;
    if base > lines.len()
        || source["lineHash"] != hash(&json!(&lines[base..]))?
        || source["bpmHash"] != hash(&root["BPMList"])?
    {
        return Ok(());
    }
    let rows = rows.clone();
    for row in &rows {
        validate(row)?;
    }
    root["judgeLineList"].as_array_mut().unwrap().truncate(base);
    root[LIST] = json!(rows);
    root.as_object_mut().unwrap().shift_remove(SOURCE);
    Ok(())
}
