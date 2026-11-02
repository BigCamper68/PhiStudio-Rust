use super::*;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Item {
    pub parameter: Option<String>,
    pub index: usize,
}
impl Item {
    pub fn new(parameter: Option<&str>, index: usize) -> Self {
        Self {
            parameter: parameter.map(str::to_owned),
            index,
        }
    }
}
pub(super) fn items(app: &Workbench, keys: bool) -> Vec<(Item, Value)> {
    let names = if keys {
        app.shaders
            .parameters
            .iter()
            .map(|p| Some(p.name.as_str()))
            .collect()
    } else {
        vec![None]
    };
    names
        .into_iter()
        .flat_map(|name| {
            rows_for(app, name)
                .into_iter()
                .enumerate()
                .map(move |(index, raw)| (Item::new(name, index), raw))
        })
        .collect()
}
pub(super) fn box_hits(app: &Workbench, grid: Grid, keys: bool, area: Rect) -> BTreeSet<Item> {
    items(app, keys)
        .into_iter()
        .filter_map(|(item, raw)| {
            let col = row_column(app, grid, item.parameter.as_deref(), item.index, &raw)?;
            let rect = interval(app, grid, col, &raw, keys)?;
            (rect.intersects(grid.rect) && area.intersects(rect.intersect(grid.rect)))
                .then_some(item)
        })
        .collect()
}
pub(super) fn shifted(edit: &Edit, item: &Item, raw: &Value) -> Value {
    let mut value = raw.clone();
    let keys = if item.parameter.is_some() {
        ["startTime", "endTime"]
    } else {
        ["start", "end"]
    };
    if let Ok(delta) = edit.interval.current[0].checked_sub(edit.interval.original[0]) {
        for key in keys {
            if let Ok(beat) = serde_json::from_value::<Beat>(raw[key].clone())
                && let Ok(beat) = beat.checked_add(delta)
            {
                value[key] = json!(beat);
            }
        }
    }
    if item.parameter.is_none() {
        let delta = layout::lane(&edit.current, edit.effect) - edit.lane;
        if delta.abs() > f64::EPSILON {
            layout::place(&mut value, layout::lane(raw, item.index) + delta);
        }
    }
    value
}
pub(super) fn commit_group(app: &mut Workbench, values: Vec<(Item, Value)>) -> bool {
    let keys = values.first().is_some_and(|(id, _)| id.parameter.is_some());
    if keys {
        let Some(index) = app.shaders.selected else {
            return false;
        };
        let mut effect = app.shaders.source["effects"][index].clone();
        for (id, raw) in values {
            let Some(name) = id.parameter else {
                continue;
            };
            effect["vars"][name][id.index] = raw;
        }
        app.shader_command(ShaderAction::Update { index, effect })
    } else {
        let commands = values
            .into_iter()
            .map(|(id, effect)| Command::EditShader {
                baseline: app.shaders.source.clone(),
                action: ShaderAction::Update {
                    index: id.index,
                    effect,
                },
            })
            .collect();
        let anchor = app.anchor();
        match app.editor.execute_batch("Edit shader effect", commands) {
            Ok(changed) => {
                if changed {
                    app.playing = false;
                    app.remember_anchor(anchor);
                }
                true
            }
            Err(e) => {
                app.error = Some(e.to_string());
                false
            }
        }
    }
}
pub(super) fn delete_group(app: &mut Workbench, ids: BTreeSet<Item>) {
    if ids.first().is_some_and(|id| id.parameter.is_some()) {
        let Some(index) = app.shaders.selected else {
            return;
        };
        let mut effect = app.shaders.source["effects"][index].clone();
        for id in ids.iter().rev() {
            let Some(name) = &id.parameter else {
                continue;
            };
            if let Some(rows) = effect["vars"][name].as_array_mut() {
                let value = rows.remove(id.index)["start"].clone();
                if rows.is_empty() {
                    effect["vars"][name] = value;
                }
            }
        }
        app.shader_command(ShaderAction::Update { index, effect });
        app.shaders.animation = None;
    } else {
        let commands = ids
            .iter()
            .rev()
            .map(|id| Command::EditShader {
                baseline: app.shaders.source.clone(),
                action: ShaderAction::Delete(id.index),
            })
            .collect();
        let anchor = app.anchor();
        match app.editor.execute_batch("Erase shader effects", commands) {
            Ok(true) => {
                app.playing = false;
                app.remember_anchor(anchor);
                app.shaders.selected = None;
            }
            Ok(false) => {}
            Err(e) => app.error = Some(e.to_string()),
        }
    }
    app.shaders.key = None;
    app.refresh_shaders();
}
pub(in crate::shader_ui) fn selection_key(app: &mut Workbench, key: egui::Key) -> bool {
    use egui::Key;
    if key == Key::V {
        if app.shaders.selection_clipboard.len() < 2 {
            return false;
        }
    } else if app.shaders.selection.len() < 2 {
        return false;
    }
    if !app.flush_shader_draft() {
        return true;
    }
    app.refresh_shaders();
    let keys = app.shaders.animation_track().is_some();
    let values = items(app, keys)
        .into_iter()
        .filter(|(id, _)| app.shaders.selection.contains(id))
        .collect::<Vec<_>>();
    if matches!(key, Key::C | Key::X | Key::D) {
        app.shaders.selection_clipboard = values
            .into_iter()
            .map(|(id, value)| (id.parameter, value))
            .collect();
    }
    if matches!(key, Key::Delete | Key::X) {
        delete_group(app, app.shaders.selection.clone());
        return true;
    }
    if key == Key::C {
        return true;
    }
    if !matches!(key, Key::V | Key::D) {
        return false;
    }
    let clipboard = app.shaders.selection_clipboard.clone();
    if clipboard.iter().any(|(name, _)| name.is_some() != keys) {
        return true;
    }
    if keys {
        let Some(index) = app.shaders.selected else {
            return true;
        };
        let mut effect = app.shaders.source["effects"][index].clone();
        for (name, raw) in clipboard {
            let Some(name) = name else {
                continue;
            };
            let expected = app
                .shaders
                .parameters
                .iter()
                .find(|p| p.name == name)
                .and_then(|p| uniform_type(&p.default));
            if expected.is_none()
                || uniform_type(&raw["start"]) != expected
                || uniform_type(&raw["end"]) != expected
            {
                app.error = Some("Paste a keyframe with the same parameter type".into());
                return true;
            }
            let value = &mut effect["vars"][&name];
            if !is_animated(value) {
                *value = json!([]);
            }
            if let Some(rows) = value.as_array_mut() {
                rows.push(raw);
            }
        }
        app.shader_command(ShaderAction::Update { index, effect });
    } else {
        let count = app.shaders.effects.len();
        let commands = clipboard
            .into_iter()
            .enumerate()
            .map(|(i, (_, effect))| Command::EditShader {
                baseline: app.shaders.source.clone(),
                action: ShaderAction::Insert {
                    index: count + i,
                    effect,
                },
            })
            .collect();
        let anchor = app.anchor();
        match app.editor.execute_batch("Add shader effect", commands) {
            Ok(true) => {
                app.playing = false;
                app.remember_anchor(anchor);
            }
            Ok(false) => {}
            Err(e) => app.error = Some(e.to_string()),
        }
    }
    app.refresh_shaders();
    true
}
