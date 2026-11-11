//! Sparse structural line edits preserve raw objects and reindex their references.
use crate::{ChartDocument, ChartError, Command, line_properties::FieldChange};
use serde_json::{Value, json};

pub const MAX_NEW_LINES: usize = 10_000;

pub fn moved_line_index(index: usize, from: usize, to: usize) -> usize {
    if index == from {
        to
    } else if from < to && index > from && index <= to {
        index - 1
    } else if from > to && index >= to && index < from {
        index + 1
    } else {
        index
    }
}

#[derive(Debug)]
enum Edit {
    Append { start: usize, values: Vec<Value> },
    Move { from: usize, to: usize },
    Delete { index: usize, value: Value },
}
#[derive(Debug)]
struct RootField {
    meta: bool,
    field: FieldChange,
}
#[derive(Debug)]
pub(crate) struct LineDelta {
    edit: Edit,
    parents: Vec<(usize, usize, FieldChange)>,
    roots: Vec<RootField>,
}
impl LineDelta {
    pub fn apply(&self, doc: &mut ChartDocument, forward: bool) {
        let lines = doc.root["judgeLineList"].as_array_mut().unwrap();
        match &self.edit {
            Edit::Append { start, values } => {
                if forward {
                    lines.extend(values.iter().cloned());
                } else {
                    lines.truncate(*start);
                }
            }
            Edit::Move { from, to } => {
                let (from, to) = if forward { (*from, *to) } else { (*to, *from) };
                let value = lines.remove(from);
                lines.insert(to, value);
            }
            Edit::Delete { index, value } => {
                if forward {
                    lines.remove(*index);
                } else {
                    lines.insert(*index, value.clone());
                }
            }
        }
        for (old, new, field) in &self.parents {
            field.apply(
                lines[if forward { *new } else { *old }]
                    .as_object_mut()
                    .unwrap(),
                forward,
            );
        }
        for change in &self.roots {
            let object = if change.meta {
                &mut doc.root["META"]
            } else {
                &mut doc.root
            };
            change.field.apply(object.as_object_mut().unwrap(), forward);
        }
    }
}

pub(crate) fn prepare(
    doc: &ChartDocument,
    command: Command,
) -> Result<Option<(LineDelta, &'static str)>, ChartError> {
    prepare_with_defaults(doc, command, &crate::LineCreationDefaults::default())
}
pub(crate) fn prepare_with_defaults(
    doc: &ChartDocument,
    command: Command,
    defaults: &crate::LineCreationDefaults,
) -> Result<Option<(LineDelta, &'static str)>, ChartError> {
    defaults.validate()?;
    let lines = doc.root["judgeLineList"].as_array().unwrap();
    let require = |index: usize| {
        lines
            .get(index)
            .ok_or_else(|| ChartError::Invalid("Judge line no longer exists".into()))
    };
    let duplicate = if let Command::DuplicateLine(index) = &command {
        Some(*index)
    } else {
        None
    };
    let (edit, label) = match command {
        Command::AddLine { name } => (
            Edit::Append {
                start: lines.len(),
                values: vec![defaults.line(&name)?],
            },
            "Add line",
        ),
        Command::AddLines { count } => {
            if count == 0 || count > MAX_NEW_LINES || lines.len().checked_add(count).is_none() {
                return Err(ChartError::Invalid(
                    "Create between 1 and 10,000 lines".into(),
                ));
            }
            (
                Edit::Append {
                    start: lines.len(),
                    values: (0..count)
                        .map(|i| defaults.line(&format!("Line {}", lines.len() + i)))
                        .collect::<Result<_, _>>()?,
                },
                "Create lines",
            )
        }
        Command::DuplicateLine(index) => {
            let mut value = require(index)?.clone();
            value["Name"] = json!(format!(
                "{} (copy)",
                value["Name"].as_str().unwrap_or("Line")
            ));
            (
                Edit::Append {
                    start: lines.len(),
                    values: vec![value],
                },
                "Duplicate line",
            )
        }
        Command::MoveLine { from, to } => {
            require(from)?;
            require(to)?;
            if from == to {
                return Ok(None);
            }
            (Edit::Move { from, to }, "Move line")
        }
        Command::DeleteLine(index) => {
            if index == 0 {
                return Err(ChartError::Invalid("Line 0 cannot be deleted".into()));
            }
            (
                Edit::Delete {
                    index,
                    value: require(index)?.clone(),
                },
                "Delete line",
            )
        }
        _ => unreachable!(),
    };
    let destination = |old: usize| match edit {
        Edit::Move { from, to } => Some(moved_line_index(old, from, to)),
        Edit::Delete { index, .. } if old == index => None,
        Edit::Delete { index, .. } => Some(if old > index { old - 1 } else { old }),
        _ => Some(old),
    };
    let parents = if matches!(edit, Edit::Append { .. }) {
        vec![]
    } else {
        lines
            .iter()
            .enumerate()
            .filter_map(|(old, line)| {
                let new = destination(old)?;
                let parent = line["father"].as_i64()?;
                if parent < 0 {
                    return None;
                }
                let after = destination(parent as usize).map_or(-1, |i| i as i64);
                (parent != after).then(|| {
                    (
                        old,
                        new,
                        FieldChange {
                            key: "father".into(),
                            before: line.get("father").cloned(),
                            after: Some(json!(after)),
                        },
                    )
                })
            })
            .collect()
    };
    let mut roots = vec![];
    // RPE Next's optional line-owned effect collections. Native extra.json
    // effects are independent of judge lines and intentionally remain intact.
    for (meta, key) in [
        (false, "effects"),
        (false, "shaderEvents"),
        (true, "effects"),
    ] {
        let object = if meta { &doc.root["META"] } else { &doc.root };
        let Some(before) = object.get(key).filter(|v| v.is_array()) else {
            continue;
        };
        let mut after = vec![];
        for effect in before.as_array().unwrap() {
            let Some(owner) = effect_owner(effect, lines.len()) else {
                after.push(effect.clone());
                continue;
            };
            if let Some(new) = destination(owner) {
                let mut value = effect.clone();
                let next_count = match &edit {
                    Edit::Append { values, .. } => lines.len() + values.len(),
                    Edit::Delete { .. } => lines.len() - 1,
                    Edit::Move { .. } => lines.len(),
                };
                if effect_owner(effect, next_count) != Some(new) {
                    value["line"] = json!(new);
                }
                after.push(value);
            }
            if duplicate == Some(owner) {
                let mut value = effect.clone();
                value["line"] = json!(lines.len());
                after.push(value);
            }
        }
        let after = Value::Array(after);
        if *before != after {
            roots.push(RootField {
                meta,
                field: FieldChange {
                    key: key.into(),
                    before: Some(before.clone()),
                    after: Some(after),
                },
            });
        }
    }
    Ok(Some((
        LineDelta {
            edit,
            parents,
            roots,
        },
        label,
    )))
}

// Legacy RPE effects infer their owner from the shader ordinal when `line` is
// absent. Materialize it only when a structural edit would change that meaning.
pub(crate) fn effect_owner(value: &Value, count: usize) -> Option<usize> {
    if !value.is_object() || count == 0 {
        return None;
    }
    if let Some(line) = value["line"].as_i64() {
        return Some(line.rem_euclid(count as i64) as usize);
    }
    if let Some(line) = value["line"]
        .as_f64()
        .filter(|n| n.is_finite() && n.fract() == 0.)
    {
        return Some(line.rem_euclid(count as f64) as usize);
    }
    const NAMES: [&str; 50] = [
        "chromatic",
        "circle_blur",
        "fisheye",
        "glitch",
        "grayscale",
        "noise",
        "pixel",
        "radial_blur",
        "shockwave",
        "vignette",
        "chromatic_2",
        "circle_blur_2",
        "fisheye_2",
        "glitch_2",
        "grayscale_2",
        "noise_2",
        "pixel_2",
        "radial_blur_2",
        "shockwave_2",
        "vignette_2",
        "liquid",
        "flowing",
        "image_noise",
        "snow",
        "glow_effect",
        "flip",
        "night_vision",
        "scanline",
        "rain",
        "rainbow",
        "flare",
        "underwater",
        "fog",
        "kaleidoscope",
        "emboss",
        "sobel",
        "oil_painting",
        "distortion",
        "hologram",
        "burn",
        "camera",
        "lightning",
        "old_tv",
        "bokeh",
        "neon",
        "heat_distortion",
        "rays",
        "color_shift",
        "wave",
        "two_tone",
    ];
    let shader = value
        .get("shader")
        .filter(|v| !v.is_null())
        .or_else(|| value.get("shaderType").filter(|v| !v.is_null()))
        .or_else(|| value.get("name"));
    let mut id = shader
        .and_then(Value::as_u64)
        .filter(|i| *i < 50)
        .map(|i| i as usize)
        .unwrap_or(0);
    if let Some(name) = shader.and_then(Value::as_str) {
        let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
        let mut normalized = String::new();
        let mut previous_lower = false;
        for c in name.chars() {
            if previous_lower && c.is_ascii_uppercase() {
                normalized.push('_');
            }
            normalized.extend(c.to_lowercase());
            previous_lower = c.is_ascii_lowercase();
        }
        normalized = normalized.replace('-', "_");
        for suffix in [".glsl", ".frag", "_pr"] {
            if let Some(base) = normalized.strip_suffix(suffix) {
                normalized = base.to_owned();
            }
        }
        let compact = normalized.replace('_', "");
        let canonical = match compact.as_str() {
            "radialblur" => "radial_blur",
            "circleblur" => "circle_blur",
            "oldtv" => "old_tv",
            "oilpainting" => "oil_painting",
            "heateffect" => "heat_distortion",
            _ => &normalized,
        };
        id = NAMES.iter().position(|s| *s == canonical).unwrap_or(0);
    }
    let cloned = match &value["clone"] {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
        _ => false,
    };
    if cloned && id < 10 {
        id += 10;
    }
    Some(id % count)
}
