use super::*;

pub(super) fn at(_app: &Workbench, grid: Grid, y: f32) -> Beat {
    grid.beat(y)
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

pub(super) fn rows_for<'a>(app: &'a Workbench, name: Option<&str>) -> &'a [Value] {
    if let Some(name) = name {
        app.shaders
            .draft
            .as_ref()
            .and_then(|d| d.raw["vars"][name].as_array())
            .filter(|rows| rows.first().is_some_and(Value::is_object))
            .map(Vec::as_slice)
            .unwrap_or_default()
    } else {
        app.shaders.source["effects"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}
pub(super) fn rows(app: &Workbench) -> &[Value] {
    rows_for(app, app.shaders.animation_track())
}
pub(super) fn row_column(
    app: &Workbench,
    grid: Grid,
    name: Option<&str>,
    index: usize,
    raw: &Value,
) -> Option<Rect> {
    if let Some(name) = name {
        layout::parameter_column(app, grid, name)
    } else {
        Some(layout::column(app, grid, false, layout::lane(raw, index)))
    }
}
pub(super) fn hits(app: &Workbench, grid: Grid, pos: Pos2) -> Vec<usize> {
    if !grid.rect.contains(pos) {
        return vec![];
    }
    let name = app.shaders.animation_track();
    rows(app)
        .iter()
        .enumerate()
        .filter_map(|(i, raw)| {
            let col = row_column(app, grid, name, i, raw)?;
            interval(app, grid, col, raw, name.is_some())
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
