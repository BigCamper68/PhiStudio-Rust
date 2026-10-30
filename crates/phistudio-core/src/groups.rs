//! Reversible RPE line-group organization; line indices/parents stay stable.
use crate::{ChartDocument, ChartError, line_properties::FieldChange};
use serde_json::{Value, json};
use std::collections::BTreeSet;
#[derive(Clone, Debug)]
pub enum GroupAction {
    Add(String),
    Rename { index: usize, name: String },
    Delete(usize),
    Move { from: usize, to: usize },
    Assign { lines: Vec<usize>, group: usize },
}
impl ChartDocument {
    pub fn group_names(&self) -> Result<Vec<String>, ChartError> {
        let names = match self.root.get("judgeLineGroup") {
            None | Some(Value::Null) => vec!["Default".into()],
            Some(Value::Array(values)) => values
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| ChartError::Invalid("Group names must be strings".into()))
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => {
                return Err(ChartError::Invalid(
                    "judgeLineGroup must be an array".into(),
                ));
            }
        };
        Ok(if names.is_empty() {
            vec!["Default".into()]
        } else {
            names
        })
    }
    pub fn line_group(&self, line: usize) -> usize {
        self.root["judgeLineList"][line]["Group"]
            .as_u64()
            .unwrap_or(0) as usize
    }
}
#[derive(Debug)]
pub(crate) struct GroupDelta {
    before: Option<Value>,
    after: Option<Value>,
    members: Vec<(usize, FieldChange)>,
}
impl GroupDelta {
    pub fn apply(&self, doc: &mut ChartDocument, forward: bool) {
        let value = if forward { &self.after } else { &self.before };
        if let Some(value) = value {
            doc.root["judgeLineGroup"] = value.clone();
        } else {
            doc.root
                .as_object_mut()
                .unwrap()
                .shift_remove("judgeLineGroup");
        }
        for (line, field) in &self.members {
            field.apply(
                doc.root["judgeLineList"][*line].as_object_mut().unwrap(),
                forward,
            );
        }
    }
}
pub(crate) fn prepare(
    doc: &ChartDocument,
    action: GroupAction,
) -> Result<Option<(GroupDelta, &'static str)>, ChartError> {
    let mut names = doc.group_names()?;
    let len = names.len();
    let mut mapping: Vec<usize> = (0..len).collect();
    let mut members = Vec::new();
    let mut rewrite = true;
    let before = doc.root.get("judgeLineGroup").cloned();
    let valid_name = |name: String| -> Result<String, ChartError> {
        let name = name.trim().to_owned();
        if name.is_empty() || name.len() > 256 {
            return Err(ChartError::Invalid(
                "Group name must contain 1–256 bytes".into(),
            ));
        }
        Ok(name)
    };
    let (label, deleted) = match action {
        GroupAction::Add(name) => {
            if len >= 1024 {
                return Err(ChartError::Invalid("Use at most 1,024 groups".into()));
            }
            names.push(valid_name(name)?);
            ("Add line group", None)
        }
        GroupAction::Rename { index, name } => {
            let target = names
                .get_mut(index)
                .ok_or_else(|| ChartError::Invalid("Group no longer exists".into()))?;
            *target = valid_name(name)?;
            ("Rename line group", None)
        }
        GroupAction::Delete(index) => {
            if index == 0 || index >= len {
                return Err(ChartError::Invalid(
                    "Group 0 is protected; choose an existing group".into(),
                ));
            }
            names.remove(index);
            mapping[index] = 0;
            for (i, target) in mapping.iter_mut().enumerate() {
                if i > index {
                    *target -= 1;
                }
            }
            ("Delete line group", Some(index))
        }
        GroupAction::Move { from, to } => {
            if from == 0 || to == 0 || from >= len || to >= len {
                return Err(ChartError::Invalid(
                    "Keep group 0 first and choose existing groups".into(),
                ));
            }
            let mut order: Vec<usize> = (0..len).collect();
            let old = order.remove(from);
            order.insert(to, old);
            let old = names.clone();
            names = order.iter().map(|i| old[*i].clone()).collect();
            for (new, old) in order.into_iter().enumerate() {
                mapping[old] = new;
            }
            ("Reorder line groups", None)
        }
        GroupAction::Assign { lines, group } => {
            if group >= len {
                return Err(ChartError::Invalid(
                    "Destination group no longer exists".into(),
                ));
            }
            for line in lines.into_iter().collect::<BTreeSet<_>>() {
                let object = doc.root["judgeLineList"]
                    .get(line)
                    .ok_or_else(|| ChartError::Invalid("Judge line no longer exists".into()))?;
                if doc.line_group(line) != group {
                    members.push((
                        line,
                        FieldChange {
                            key: "Group".into(),
                            before: object.get("Group").cloned(),
                            after: Some(json!(group)),
                        },
                    ));
                }
            }
            rewrite = false;
            ("Move lines to group", None)
        }
    };
    if rewrite {
        for line in 0..doc.line_count() {
            let old = doc.line_group(line);
            let new = if let Some(index) = deleted {
                if old == index {
                    0
                } else if old > index {
                    old - 1
                } else {
                    old
                }
            } else {
                mapping.get(old).copied().unwrap_or(old)
            };
            if old != new {
                members.push((
                    line,
                    FieldChange {
                        key: "Group".into(),
                        before: doc.root["judgeLineList"][line].get("Group").cloned(),
                        after: Some(json!(new)),
                    },
                ));
            }
        }
    }
    let after = if rewrite {
        Some(json!(names))
    } else {
        before.clone()
    };
    Ok((before != after || !members.is_empty()).then_some((
        GroupDelta {
            before,
            after,
            members,
        },
        label,
    )))
}
