use super::*;

pub(crate) fn prepare(
    doc: &ChartDocument,
    baseline: Value,
    action: ShaderAction,
) -> Result<Option<(ShaderDelta, &'static str)>, ChartError> {
    use crate::array_edit::{ArrayEdit, Segment::Field};
    let source = doc.root.get(EXTRA_KEY).unwrap_or(&baseline);
    validate_shader_extra(source)?;
    let empty = Vec::new();
    let rows = source["effects"].as_array().unwrap_or(&empty);
    let index_check = |i| {
        if i < rows.len() {
            Ok(())
        } else {
            Err(invalid("Shader effect no longer exists"))
        }
    };
    let (delta, label) = match action {
        ShaderAction::Move { from, to } => {
            index_check(from)?;
            index_check(to)?;
            if from == to {
                return Ok(None);
            }
            (ShaderDelta::Move { from, to }, "Reorder shader effects")
        }
        ShaderAction::Tempo(after) => {
            let mut next = source.clone();
            next["bpm"] = after.clone();
            validate_shader_extra(&next)?;
            if source["bpm"] == after {
                return Ok(None);
            }
            (
                ShaderDelta::Tempo {
                    before: source["bpm"].clone(),
                    after,
                },
                "Edit shader tempo",
            )
        }
        action => {
            let (index, replacing, after, label) = match action {
                ShaderAction::Insert { index, effect } => {
                    (index, false, Some(effect), "Add shader effect")
                }
                ShaderAction::Update { index, effect } => {
                    (index, true, Some(effect), "Edit shader effect")
                }
                ShaderAction::Delete(index) => (index, true, None, "Delete shader effect"),
                _ => unreachable!(),
            };
            if replacing {
                index_check(index)?;
            } else if index > rows.len() || rows.len() >= 4096 {
                return Err(invalid("Invalid shader insertion index or effect limit"));
            }
            if let Some(value) = &after {
                validate_authored_effect(value)?;
            }
            if replacing && after.as_ref() == rows.get(index) {
                return Ok(None);
            }
            // Only the first edit imports the sidecar. Later edits retain one effect.
            let mut root = json!({EXTRA_KEY: source});
            let edit = ArrayEdit::prepare(
                &root,
                vec![Field(EXTRA_KEY), Field("effects")],
                index,
                replacing,
                after,
                false,
                false,
            )?;
            edit.apply(&mut root, true);
            validate_shader_extra(&root[EXTRA_KEY])?;
            (ShaderDelta::Effect(edit), label)
        }
    };
    if doc.root.get(EXTRA_KEY).is_none() {
        let mut scratch = json!({EXTRA_KEY: source});
        delta.apply_root(&mut scratch, true);
        Ok(Some((
            ShaderDelta::Initialize(scratch[EXTRA_KEY].take()),
            label,
        )))
    } else {
        Ok(Some((delta, label)))
    }
}

pub(crate) fn prepare_source(
    doc: &ChartDocument,
    path: String,
    source: String,
) -> Result<Option<ShaderDelta>, ChartError> {
    validate_shader_source(&path, &source)?;
    let mut map = shader_sources(doc)?;
    let after = Value::String(source);
    let before = map.insert(path.clone(), after.clone());
    if before.as_ref() == Some(&after) {
        return Ok(None);
    }
    if map.len() > 128 || serde_json::to_vec(&map)?.len() > 8 * MAX_SOURCE_BYTES {
        return Err(invalid("Shader sources exceed the project limit"));
    }
    Ok(Some(ShaderDelta::Source {
        path,
        before,
        after,
        container: doc.root.get(SOURCES_KEY).map(|_| json!({})),
    }))
}
