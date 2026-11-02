use super::*;
use crate::array_edit::{ArrayEdit, Segment};
#[derive(Debug)]
pub(crate) enum Delta {
    Array {
        edit: Box<ArrayEdit>,
        version: Option<Option<Value>>,
    },
    Move {
        from: usize,
        to: usize,
    },
    Options {
        before: Option<Value>,
        after: Value,
    },
}
impl Delta {
    pub fn apply(&self, doc: &mut ChartDocument, forward: bool) {
        match self {
            Self::Array { edit, version } => {
                edit.apply(&mut doc.root, forward);
                if let Some(before) = version {
                    if forward {
                        doc.root["META"]["RPEVersion"] = json!(
                            before
                                .as_ref()
                                .and_then(Value::as_u64)
                                .unwrap_or(0)
                                .max(220)
                        );
                    } else if before.is_none() {
                        doc.root["META"]
                            .as_object_mut()
                            .unwrap()
                            .shift_remove("RPEVersion");
                    } else {
                        doc.root["META"]["RPEVersion"] = before.clone().unwrap();
                    }
                }
            }
            Self::Move { from, to } => {
                let (a, b) = if forward { (*from, *to) } else { (*to, *from) };
                let rows = doc.root[LIST].as_array_mut().unwrap();
                let value = rows.remove(a);
                rows.insert(b, value);
            }
            Self::Options { before, after } => {
                if forward {
                    doc.root["noiseAreaOptions"] = after.clone();
                } else if let Some(v) = before {
                    doc.root["noiseAreaOptions"] = v.clone();
                } else {
                    doc.root
                        .as_object_mut()
                        .unwrap()
                        .shift_remove("noiseAreaOptions");
                }
            }
        }
    }
}
pub(crate) fn prepare(
    doc: &ChartDocument,
    action: Action,
) -> Result<Option<(Delta, &'static str)>, ChartError> {
    let rows = fields(doc)?;
    let check = |i: usize| {
        rows.get(i)
            .ok_or_else(|| invalid("Noise Field no longer exists"))
    };
    let (path, index, replacing, after, label, version) = match action {
        Action::Move { from, to } => {
            check(from)?;
            check(to)?;
            return Ok((from != to).then_some((Delta::Move { from, to }, "Reorder Noise Fields")));
        }
        Action::Options(after) => {
            if !after.is_object()
                || after
                    .get("ignoreTripleInversion")
                    .is_some_and(|v| !v.is_boolean())
            {
                return Err(invalid("Invalid Noise Field options"));
            }
            let before = doc.root.get("noiseAreaOptions").cloned();
            return Ok((before.as_ref() != Some(&after))
                .then_some((Delta::Options { before, after }, "Edit Noise Field options")));
        }
        Action::Key {
            field,
            track,
            index,
            value,
        } => {
            let original = check(field)?;
            let f = format(original)?;
            if !Track::all(f).contains(&track) {
                return Err(invalid(
                    "This track does not belong to the selected Noise Field format",
                ));
            }
            let keys = keys(original, track)?;
            let at = index.unwrap_or(keys.len());
            if index.is_some() && at >= keys.len() {
                return Err(invalid("Noise Field key no longer exists"));
            }
            if index.is_none() && value.is_none() {
                return Ok(None);
            }
            if let Some(value) = &value {
                validate_key(value, track, f)?;
            }
            if index.is_some() && value.as_ref() == keys.get(at) {
                return Ok(None);
            }
            let mut scratch = original.clone();
            let mut next = keys.to_vec();
            match (index, value.as_ref()) {
                (None, Some(v)) => next.push(v.clone()),
                (Some(i), Some(v)) => next[i] = v.clone(),
                (Some(i), None) => {
                    next.remove(i);
                }
                _ => {}
            }
            scratch[track.key()] = json!(next);
            validate(&scratch)?;
            (
                vec![
                    Segment::Field(LIST),
                    Segment::Index(field),
                    Segment::Field(track.key()),
                ],
                at,
                index.is_some(),
                value,
                "Edit Noise Field key",
                f == Format::Next && doc.root["META"]["RPEVersion"].as_u64().unwrap_or(0) < 220,
            )
        }
        action => {
            let (index, replacing, after, label) = match action {
                Action::Insert { index, value } => (index, false, Some(value), "Add Noise Field"),
                Action::Update { index, value } => (index, true, Some(value), "Edit Noise Field"),
                Action::Delete(index) => (index, true, None, "Delete Noise Field"),
                _ => unreachable!(),
            };
            if replacing {
                check(index)?;
            } else if index > rows.len() || rows.len() >= 20000 {
                return Err(invalid("Noise Field insertion index or count is invalid"));
            }
            if replacing && after.as_ref() == rows.get(index) {
                return Ok(None);
            }
            if let Some(value) = &after {
                validate(value)?;
            }
            let version = after
                .as_ref()
                .is_some_and(|v| format(v).ok() == Some(Format::Next))
                && doc.root["META"]["RPEVersion"].as_u64().unwrap_or(0) < 220;
            (
                vec![Segment::Field(LIST)],
                index,
                replacing,
                after,
                label,
                version,
            )
        }
    };
    let edit = ArrayEdit::prepare(&doc.root, path, index, replacing, after, false, false)?;
    Ok(Some((
        Delta::Array {
            edit: Box::new(edit),
            version: version.then(|| doc.root["META"].get("RPEVersion").cloned()),
        },
        label,
    )))
}
