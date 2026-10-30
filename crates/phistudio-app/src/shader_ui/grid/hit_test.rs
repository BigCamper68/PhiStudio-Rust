use super::*;

pub(super) fn at(_app: &Workbench, grid: Grid, y: f32) -> Beat {
    grid.beat(y)
}

pub(super) fn lanes(app: &Workbench) -> Vec<(String, bool)> {
    let mut lanes = Vec::new();
    for effect in &app.shaders.effects {
        let key = (effect.shader.clone(), effect.global);
        if !lanes.contains(&key) {
            lanes.push(key);
        }
    }
    let preset = (PRESETS[app.shaders.preset].to_owned(), false);
    if lanes.is_empty() || (app.tool == Tool::Event && !lanes.contains(&preset)) {
        lanes.push(preset);
    }
    lanes
}

pub(super) fn column(grid: Grid, index: usize, count: usize) -> Rect {
    let width = grid.rect.width() / count.max(1) as f32;
    Rect::from_min_max(
        pos2(grid.rect.left() + width * index as f32, grid.rect.top()),
        pos2(
            grid.rect.left() + width * (index + 1) as f32,
            grid.rect.bottom(),
        ),
    )
}

pub(super) fn interval(
    _app: &Workbench,
    grid: Grid,
    col: Rect,
    raw: &Value,
    keyframe: bool,
) -> Option<Rect> {
    let start: Beat =
        serde_json::from_value(raw[if keyframe { "startTime" } else { "start" }].clone()).ok()?;
    let end: Beat =
        serde_json::from_value(raw[if keyframe { "endTime" } else { "end" }].clone()).ok()?;
    Some(Rect::from_min_max(
        pos2(col.left() + 3., grid.y(end.as_f64())),
        pos2(col.right() - 3., grid.y(start.as_f64())),
    ))
}

pub(super) fn rows(app: &Workbench) -> Vec<Value> {
    if let Some(name) = app.shaders.animation_track() {
        app.shaders
            .draft
            .as_ref()
            .and_then(|d| d.raw["vars"][name].as_array())
            .filter(|rows| rows.first().is_some_and(Value::is_object))
            .cloned()
            .unwrap_or_default()
    } else {
        app.shaders.source["effects"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }
}

pub(super) fn hits(app: &Workbench, grid: Grid, pos: Pos2) -> Vec<usize> {
    if !grid.rect.contains(pos) {
        return vec![];
    }
    let lanes = lanes(app);
    rows(app)
        .iter()
        .enumerate()
        .filter_map(|(i, raw)| {
            let col = if app.shaders.animation_track().is_some() {
                grid.rect
            } else {
                let lane = lanes.iter().position(|(name, global)| {
                    raw["shader"].as_str() == Some(name)
                        && raw["global"].as_bool().unwrap_or(false) == *global
                })?;
                column(grid, lane, lanes.len())
            };
            interval(app, grid, col, raw, app.shaders.animation_track().is_some())
                .filter(|r| r.expand2(vec2(0., 8.)).contains(pos))
                .map(|_| i)
        })
        .collect()
}

pub(super) fn hit(app: &Workbench, grid: Grid, pos: Pos2) -> Option<usize> {
    hits(app, grid, pos).last().copied()
}

pub(crate) fn selected_hit(app: &Workbench) -> Option<crate::picking::Hit> {
    if let Some(name) = app.shaders.animation_track() {
        Some(crate::picking::Hit::Keyframe {
            effect: app.shaders.selected?,
            parameter: name.to_owned(),
            index: app.shaders.animation.as_ref()?.index,
        })
    } else {
        app.shaders.selected.map(crate::picking::Hit::Shader)
    }
}

pub(crate) fn cycle_hits(app: &Workbench, grid: Grid, pos: Pos2) -> Vec<crate::picking::Hit> {
    hits(app, grid, pos)
        .into_iter()
        .rev()
        .filter_map(|index| {
            if let Some(name) = app.shaders.animation_track() {
                Some(crate::picking::Hit::Keyframe {
                    effect: app.shaders.selected?,
                    parameter: name.to_owned(),
                    index,
                })
            } else {
                Some(crate::picking::Hit::Shader(index))
            }
        })
        .collect()
}
