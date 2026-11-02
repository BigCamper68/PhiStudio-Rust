//! The shared touch router owns cancellation, pinch and physical input ordering.
use super::layout::{self, View};
use super::*;
mod native;
mod paint;
use crate::grid_input::{Gesture, Grid};
use egui::{Pos2, Rect, pos2, vec2};
pub(crate) use paint::paint;
#[derive(Clone)]
pub(crate) struct Edit {
    pub original: Value,
    pub current: Value,
    index: usize,
    creating: bool,
    key: bool,
    origin: Pos2,
    grid: Grid,
    range: [Beat; 2],
    handle: Option<usize>,
    end_key: Option<native::EndKey>,
    hits: Vec<usize>,
    companions: Vec<(usize, Value, Value)>,
    pub moved: bool,
}
pub(crate) enum NoiseGesture {
    Edit(Box<Edit>),
    Select {
        origin: Pos2,
        grid: Grid,
        ids: Vec<usize>,
        moved: bool,
        additive: bool,
        offset: f64,
        horizontal: Option<bool>,
    },
    Erase {
        ids: BTreeSet<usize>,
        grid: Grid,
        last: Pos2,
    },
    Box {
        origin: Pos2,
        current: Pos2,
        grid: Grid,
        additive: bool,
    },
}
impl NoiseGesture {
    pub(crate) fn edits_existing(&self) -> bool {
        matches!(self,Self::Edit(d)if !d.creating&&d.moved)
    }
}
fn rows_in(app: &Workbench, view: View) -> &[Value] {
    if view.keys {
        app.noise
            .selected
            .and_then(|i| app.noise.rows.get(i))
            .and_then(|v| noise::keys(v, view.track).ok())
            .unwrap_or(&[])
    } else {
        &app.noise.rows
    }
}
fn rows(app: &Workbench) -> &[Value] {
    rows_in(app, View::focus(app))
}
fn endpoints(app: &Workbench, raw: &Value) -> Option<[Beat; 2]> {
    let tempo = app.editor.document().tempo();
    if app.noise.animation {
        if app.noise.track.native(app.noise_format()) {
            let b = noise::beat(raw, "time", Format::Native, tempo).ok()?;
            Some([b, b])
        } else {
            Some([
                serde_json::from_value(raw["startTime"].clone()).ok()?,
                serde_json::from_value(raw["endTime"].clone()).ok()?,
            ])
        }
    } else {
        let f = noise::format(raw).ok()?;
        Some([
            noise::beat(raw, "appearTime", f, tempo).ok()?,
            noise::beat(raw, "disappearTime", f, tempo).ok()?,
        ])
    }
}
fn rect_span(
    app: &Workbench,
    grid: Grid,
    raw: &Value,
    index: usize,
    view: View,
    range: [Beat; 2],
) -> Rect {
    let lane = if view.keys {
        Track::all(app.noise_format())
            .iter()
            .position(|t| *t == view.track)
            .unwrap_or(0) as f64
    } else {
        layout::lane(raw, index)
    };
    let col = layout::column(app, grid, view, lane);
    let col = col.shrink2(vec2((col.width() * 0.25).min(4.), 0.));
    let bottom = grid.y(range[0].as_f64());
    let top = grid
        .y(range[1].as_f64())
        .min(bottom - if view.keys { 1. } else { 8. });
    Rect::from_min_max(pos2(col.left(), top), pos2(col.right(), bottom))
}
struct Span {
    range: [Beat; 2],
    next: Option<usize>,
}
fn spans(app: &Workbench, values: &[&Value], view: View) -> Vec<Option<Span>> {
    let tempo = app.editor.document().tempo();
    if view.keys && view.track.native(app.noise_format()) {
        let mut times: Vec<_> = values
            .iter()
            .enumerate()
            .filter_map(|(i, v)| {
                noise::beat(v, "time", Format::Native, tempo)
                    .ok()
                    .map(|b| (b, i))
            })
            .collect();
        times.sort_by_key(|p| p.0);
        let mut spans: Vec<Option<Span>> = (0..values.len()).map(|_| None).collect();
        for (n, (time, index)) in times.iter().enumerate() {
            let next = times.get(n + 1);
            spans[*index] = Some(Span {
                range: [*time, next.map_or(*time, |p| p.0)],
                next: next.map(|p| p.1),
            });
        }
        spans
    } else {
        values
            .iter()
            .map(|raw| {
                let f = if view.keys {
                    Format::Next
                } else {
                    noise::format(raw).ok()?
                };
                let names = if view.keys {
                    ["startTime", "endTime"]
                } else {
                    ["appearTime", "disappearTime"]
                };
                Some(Span {
                    range: [
                        noise::beat(raw, names[0], f, tempo).ok()?,
                        noise::beat(raw, names[1], f, tempo).ok()?,
                    ],
                    next: None,
                })
            })
            .collect()
    }
}
pub(crate) fn hits(app: &Workbench, grid: Grid, pos: Pos2) -> Vec<usize> {
    if !grid.rect.contains(pos) {
        return vec![];
    }
    let view = View::focus(app);
    let values: Vec<_> = rows(app).iter().collect();
    let spans = spans(app, &values, view);
    values
        .iter()
        .enumerate()
        .filter_map(|(i, raw)| {
            let r = rect_span(app, grid, raw, i, view, spans[i].as_ref()?.range);
            r.expand2(vec2(0., 7.)).contains(pos).then_some(i)
        })
        .collect()
}
pub(crate) fn begin_dual(
    app: &mut Workbench,
    grid: Grid,
    pos: Pos2,
    additive: bool,
) -> Option<Gesture> {
    let keys = grid.event_rect.contains(pos);
    let surface = if keys { grid.events() } else { grid };
    let track = if keys {
        let tracks = Track::all(app.noise_format());
        *tracks.get(layout::at(app, surface, true, pos).max(0.) as usize)?
    } else {
        app.noise.track
    };
    if !app.focus_noise(keys, track) {
        return None;
    }
    begin(app, surface, pos, additive)
}
pub(crate) fn begin(app: &mut Workbench, grid: Grid, pos: Pos2, additive: bool) -> Option<Gesture> {
    app.refresh_noise();
    let ids = hits(app, grid, pos);
    let found = ids
        .iter()
        .rev()
        .copied()
        .find(|i| app.noise.selection.contains(i))
        .or_else(|| ids.last().copied());
    if app.tool == Tool::Erase {
        if !app.flush_noise_draft() {
            return None;
        }
        return Some(Gesture::Noise(Box::new(NoiseGesture::Erase {
            ids: ids.into_iter().collect(),
            grid,
            last: pos,
        })));
    }
    if app.tool == Tool::BoxSelect {
        if !app.flush_noise_draft() {
            return None;
        }
        return Some(Gesture::Noise(Box::new(NoiseGesture::Box {
            origin: pos,
            current: pos,
            grid,
            additive,
        })));
    }
    let creating = app.tool == Tool::Event;
    let selected = if app.noise.animation {
        app.noise.key_selected
    } else {
        app.noise.selected
    };
    if !creating
        && (found.is_none()
            || (found != selected && !found.is_some_and(|i| app.noise.selection.contains(&i)))
            || additive)
    {
        return Some(Gesture::Noise(Box::new(NoiseGesture::Select {
            origin: pos,
            grid,
            ids,
            moved: false,
            additive,
            offset: app.noise.offset,
            horizontal: None,
        })));
    }
    if !app.flush_noise_draft() {
        return None;
    }
    app.refresh_noise();
    let start = grid.beat(pos.y);
    let end = start
        .checked_add(Beat::new(0, 1, grid.division).ok()?)
        .ok()?;
    let mut initial_tail = None;
    let mut raw = if creating {
        if app.noise.animation {
            let mut keys = noise::new_key_interval(
                app.noise.rows.get(app.noise.selected?)?,
                app.noise.track,
                start,
                end,
                app.editor.document().tempo(),
            )
            .ok()?;
            if keys.len() > 1 {
                initial_tail = keys.pop();
            }
            keys.remove(0)
        } else {
            noise::create(
                start,
                end,
                app.noise.format,
                app.noise.subtract,
                app.editor.document().tempo(),
            )
        }
    } else {
        rows(app).get(found?)?.clone()
    };
    if creating && !app.noise.animation {
        layout::place(&mut raw, layout::at(app, grid, false, pos));
    }
    let index = if creating { rows(app).len() } else { found? };
    let is_native = app.noise.animation && app.noise.track.native(app.noise_format());
    let native_span = if is_native && !creating {
        let values: Vec<_> = rows(app).iter().collect();
        spans(app, &values, View::focus(app))
            .get(index)?
            .as_ref()
            .map(|s| (s.range, s.next))
    } else {
        None
    };
    let range = if is_native {
        native_span.map_or([start, end], |s| s.0)
    } else {
        endpoints(app, &raw)?
    };
    let handle = if creating {
        Some(3)
    } else if app.noise.animation {
        let a = (pos.y - grid.y(range[0].as_f64())).abs();
        let b = (pos.y - grid.y(range[1].as_f64())).abs();
        if range[0] == range[1] {
            Some(if pos.y < grid.y(range[0].as_f64()) - 0.5 {
                3
            } else {
                0
            })
        } else {
            // Reserve the center of short blocks for moving the whole interval.
            let grip =
                ((grid.y(range[0].as_f64()) - grid.y(range[1].as_f64())).abs() * 0.25).min(12.);
            (a.min(b) <= grip).then_some(if a <= b { 0 } else { 3 })
        }
    } else {
        let format = noise::format(&raw).ok()?;
        noise::PHASES
            .iter()
            .enumerate()
            .filter(|(i, _)| format == Format::Native || *i == 0 || *i == 3)
            .filter_map(|(i, k)| {
                noise::beat(&raw, k, format, app.editor.document().tempo())
                    .ok()
                    .map(|b| (i, (grid.y(b.as_f64()) - pos.y).abs()))
            })
            .filter(|(_, distance)| *distance < 10.)
            .min_by(|a, b| {
                a.1.total_cmp(&b.1).then_with(|| {
                    usize::from(a.0 == 1 || a.0 == 2).cmp(&usize::from(b.0 == 1 || b.0 == 2))
                })
            })
            .map(|v| v.0)
    };
    let end_key = if is_native && (creating || handle == Some(3)) {
        Some(native::EndKey::new(
            app,
            native_span.and_then(|s| s.1),
            initial_tail.unwrap_or_else(|| raw.clone()),
            range[1],
        ))
    } else {
        None
    };
    let companions = if !creating && handle.is_none() {
        let mut ids = if app.noise.selection.contains(&index) {
            app.noise.selection.clone()
        } else {
            BTreeSet::from([index])
        };
        if is_native {
            // A native column is the interval between two real point keys.
            // Include every moved interval's endpoint once, including shared ones.
            let values: Vec<_> = rows(app).iter().collect();
            let spans = spans(app, &values, View::focus(app));
            let ends: Vec<_> = ids
                .iter()
                .filter_map(|i| spans.get(*i)?.as_ref()?.next)
                .collect();
            ids.extend(ends);
        }
        ids.remove(&index);
        ids.into_iter()
            .filter_map(|i| rows(app).get(i).map(|v| (i, v.clone(), v.clone())))
            .collect()
    } else {
        vec![]
    };
    Some(Gesture::Noise(Box::new(NoiseGesture::Edit(Box::new(
        Edit {
            original: raw.clone(),
            current: raw,
            index,
            creating,
            key: app.noise.animation,
            origin: pos,
            grid,
            range,
            handle,
            end_key,
            hits: ids,
            companions,
            moved: false,
        },
    )))))
}
pub(crate) fn update(app: &mut Workbench, gesture: &mut NoiseGesture, pos: Pos2) {
    match gesture {
        NoiseGesture::Edit(d) => {
            d.moved |= pos.distance(d.origin) > 8.;
            if !d.creating && !d.moved {
                return;
            }
            let at = d.grid.beat(pos.y);
            let tempo = app.editor.document().tempo();
            let mut value = d.original.clone();
            let result = (|| -> Result<(), phistudio_core::ChartError> {
                let tick = Beat::new(0, 1, d.grid.division)?;
                if d.key {
                    if app.noise.track.native(app.noise_format()) {
                        value = native::update(app, d, at)?;
                    } else if let Some(handle) = d.handle {
                        let at = if handle == 0 {
                            at.min(d.range[1].checked_sub(tick)?)
                        } else {
                            at.max(d.range[0].checked_add(tick)?)
                        };
                        noise::put_beat(
                            &mut value,
                            if handle == 0 { "startTime" } else { "endTime" },
                            at,
                            Format::Next,
                            tempo,
                        )?;
                    } else {
                        value = noise::shift_key(
                            &d.original,
                            app.noise.track,
                            app.noise_format(),
                            d.grid.beat(d.origin.y),
                            at,
                            tempo,
                        )?;
                    }
                } else if let Some(handle) = d.handle {
                    if d.creating {
                        value = noise::create(
                            d.range[0],
                            at.max(d.range[0].checked_add(tick)?),
                            app.noise.format,
                            app.noise.subtract,
                            tempo,
                        );
                    } else {
                        value = noise::resize_field(&d.original, handle, at, tick, tempo)?;
                    }
                } else {
                    let delta = at.checked_sub(d.grid.beat(d.origin.y))?;
                    let next = d.range[0].checked_add(delta)?;
                    value = noise::shifted(&d.original, d.range[0], next, tempo)?;
                }
                if !d.key && (d.creating || d.handle.is_none()) {
                    let lane = layout::lane(&d.original, d.index);
                    let next = layout::shifted(app, d.grid, lane, pos.x - d.origin.x);
                    if d.creating || next != lane {
                        layout::place(&mut value, next);
                    }
                }
                Ok(())
            })();
            if result.is_ok() {
                d.current = value;
                let from = d.grid.beat(d.origin.y);
                for (index, original, current) in &mut d.companions {
                    let shifted = if d.key {
                        noise::shift_key(
                            original,
                            app.noise.track,
                            app.noise_format(),
                            from,
                            at,
                            tempo,
                        )
                    } else {
                        noise::shifted(original, from, at, tempo)
                    };
                    if let Ok(mut value) = shifted {
                        if !d.key {
                            let lane = layout::lane(&d.original, d.index);
                            let delta =
                                layout::shifted(app, d.grid, lane, pos.x - d.origin.x) - lane;
                            if delta != 0. {
                                layout::place(&mut value, layout::lane(original, *index) + delta);
                            }
                        }
                        *current = value;
                    }
                }
            }
        }
        NoiseGesture::Select {
            origin,
            grid,
            moved,
            offset,
            horizontal,
            ..
        } => {
            let delta = pos - *origin;
            *moved |= delta.length() > 8.;
            if *moved {
                if horizontal.is_none() {
                    if delta.x.abs() > 18. && delta.x.abs() > delta.y.abs() * 1.6 {
                        *horizontal = Some(true);
                    } else if delta.y.abs() > 8. && delta.y.abs() >= delta.x.abs() * 0.6 {
                        *horizontal = Some(false);
                    }
                }
                match horizontal {
                    Some(true) if !app.noise.animation => {
                        app.noise.offset = *offset;
                        layout::pan(app, *grid, -f64::from(delta.x));
                    }
                    Some(true) => {}
                    Some(false) => app.set_grid_beat(
                        (grid.beat + f64::from(delta.y / grid.zoom * grid.scroll_speed)).max(0.),
                    ),
                    None => {}
                }
            }
        }
        NoiseGesture::Erase { ids, grid, last } => {
            let count = ((pos.distance(*last) / 8.).ceil() as usize).clamp(1, 256);
            for i in 1..=count {
                ids.extend(hits(
                    app,
                    *grid,
                    *last + (pos - *last) * (i as f32 / count as f32),
                ));
            }
            *last = pos;
        }
        NoiseGesture::Box { current, .. } => *current = pos,
    }
}
pub(crate) fn finish(app: &mut Workbench, gesture: NoiseGesture) {
    match gesture {
        NoiseGesture::Select {
            origin,
            ids,
            moved: false,
            additive,
            ..
        } => {
            let revision = app.editor.revision();
            let previous = app.noise.cycle.take();
            let selected = if app.noise.animation {
                app.noise.key_selected
            } else {
                app.noise.selected
            };
            let continuing = previous.as_ref().is_some_and(|(p, h, i, r)| {
                p.distance(origin) < 12. && *h == ids && Some(*i) == selected && *r == revision
            });
            let index = if continuing {
                selected
                    .and_then(|i| ids.iter().position(|v| *v == i))
                    .map(|i| ids[(i + ids.len() - 1) % ids.len()])
            } else {
                ids.last().copied()
            };
            app.select_noise(index, additive);
            if let Some(i) = index {
                app.noise.cycle = Some((origin, ids, i, app.editor.revision()));
            }
        }
        NoiseGesture::Edit(d) => {
            if !d.creating && !d.moved {
                if d.hits.len() > 1 {
                    finish(
                        app,
                        NoiseGesture::Select {
                            origin: d.origin,
                            grid: d.grid,
                            ids: d.hits,
                            moved: false,
                            additive: false,
                            offset: app.noise.offset,
                            horizontal: None,
                        },
                    );
                }
                return;
            }
            if !d.creating
                && d.current == d.original
                && d.end_key.as_ref().is_none_or(|e| !e.changed())
            {
                return;
            }
            let mut actions = vec![];
            let mut selected =
                if !d.creating && d.handle.is_none() && app.noise.selection.contains(&d.index) {
                    app.noise.selection.clone()
                } else {
                    BTreeSet::new()
                };
            selected.insert(d.index);
            let action = if d.key {
                let Some(field) = app.noise.selected else {
                    return;
                };
                if !d.companions.is_empty() {
                    let mut value = app.noise.rows[field].clone();
                    value[app.noise.track.key()][d.index] = d.current;
                    for (i, _, current) in d.companions {
                        value[app.noise.track.key()][i] = current;
                    }
                    Action::Update {
                        index: field,
                        value,
                    }
                } else {
                    Action::Key {
                        field,
                        track: app.noise.track,
                        index: (!d.creating).then_some(d.index),
                        value: Some(d.current),
                    }
                }
            } else if d.creating {
                Action::Insert {
                    index: d.index,
                    value: d.current,
                }
            } else {
                actions.extend(
                    d.companions
                        .into_iter()
                        .map(|(index, _, value)| Action::Update { index, value }),
                );
                Action::Update {
                    index: d.index,
                    value: d.current,
                }
            };
            actions.push(action);
            if let Some(end) = d.end_key
                && let Some(field) = app.noise.selected
            {
                actions.push(Action::Key {
                    field,
                    track: app.noise.track,
                    index: end.index,
                    value: Some(end.current),
                });
            }
            if app.noise_actions("Edit Noise Field interval", actions) {
                if d.key {
                    app.noise.key_selected = Some(d.index);
                } else {
                    app.noise.selected = Some(d.index);
                }
                app.noise.selection = selected;
                app.refresh_noise();
                app.show_context = true;
            }
        }
        NoiseGesture::Erase { ids, .. } => {
            app.noise.selection = ids;
            app.delete_noise();
        }
        NoiseGesture::Box {
            origin,
            current,
            grid,
            additive,
        } => {
            let box_rect = Rect::from_two_pos(origin, current);
            let values: Vec<_> = rows(app).iter().collect();
            let view = View::focus(app);
            let spans = spans(app, &values, view);
            let ids = values
                .iter()
                .enumerate()
                .filter_map(|(i, v)| {
                    let range = spans[i].as_ref()?.range;
                    rect_span(app, grid, v, i, view, range)
                        .intersects(box_rect)
                        .then_some(i)
                })
                .collect::<BTreeSet<_>>();
            if !additive {
                app.noise.selection.clear();
            }
            app.noise.selection.extend(ids);
            let first = app.noise.selection.first().copied();
            if app.noise.animation {
                app.noise.key_selected = first;
                app.noise.key_draft = None;
                app.load_noise_key();
            } else {
                app.noise.selected = first;
                app.noise.cache = None;
                app.refresh_noise();
            }
        }
        _ => {}
    }
}
