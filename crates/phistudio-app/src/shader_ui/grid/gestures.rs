use super::*;

pub(crate) fn tap(gesture: &ShaderGesture) -> Option<(Grid, Pos2)> {
    match gesture {
        ShaderGesture::Edit(d) if !d.creating && !d.moved => Some((d.grid, d.origin)),
        _ => None,
    }
}

pub(crate) fn begin_dual(app: &mut Workbench, grid: Grid, pos: Pos2) -> Option<Gesture> {
    app.refresh_shaders();
    let keys = grid.event_rect.contains(pos);
    let mut surface = if keys {
        grid.events()
    } else {
        Grid {
            event_rect: grid.rect,
            ..grid
        }
    };
    let parameter = if keys {
        if let Some(parameter) = layout::parameter_at(app, surface, pos) {
            Some(parameter)
        } else {
            return Some(Gesture::Shader(Box::new(ShaderGesture::Navigate {
                grid: surface,
                origin: pos,
                offset: app.shaders.offsets[1],
                horizontal: None,
            })));
        }
    } else {
        None
    };
    if !app.focus_shader_grid(parameter) {
        return None;
    }
    surface.camera_x = app.shaders.offsets[usize::from(keys)];
    begin(app, surface, pos)
}

pub(crate) fn begin(app: &mut Workbench, grid: Grid, pos: Pos2) -> Option<Gesture> {
    app.refresh_shaders();
    if app.shaders.error.is_some() {
        return None;
    }
    let selected = selected_hit(app).map(|hit| match hit {
        crate::picking::Hit::Shader(i) | crate::picking::Hit::Keyframe { index: i, .. } => i,
        _ => unreachable!(),
    });
    let found = if app.tool == Tool::Select {
        selected.filter(|i| hits(app, grid, pos).contains(i))
    } else {
        None
    }
    .or_else(|| hit(app, grid, pos));
    if app.tool == Tool::Erase {
        if !app.flush_shader_draft() {
            return None;
        }
        app.refresh_shaders();
        return Some(Gesture::Shader(Box::new(ShaderGesture::Erase {
            ids: selection::box_hits(
                app,
                grid,
                app.shaders.animation_track().is_some(),
                Rect::from_center_size(pos, vec2(1., 16.)),
            ),
            keys: app.shaders.animation_track().is_some(),
            grid,
            last: pos,
        })));
    }
    let item = found.map(|i| Item::new(app.shaders.animation_track(), i));
    let in_group = item
        .as_ref()
        .is_some_and(|id| app.shaders.selection.contains(id));
    if app.tool == Tool::BoxSelect && !in_group {
        return Some(Gesture::Shader(Box::new(ShaderGesture::Box {
            grid,
            keys: app.shaders.animation_track().is_some(),
            origin: pos,
            current: pos,
            additive: app.tools.additive,
        })));
    }
    let selected = if app.shaders.animation_track().is_some() {
        app.shaders.animation.as_ref().map(|a| a.index)
    } else {
        app.shaders.selected
    };
    if app.tool != Tool::Event && !in_group && (found.is_none() || found != selected) {
        return Some(Gesture::Tap {
            origin: pos,
            grid,
            intent: Intent::SelectShader(found),
            moved: false,
        });
    }
    if !app.flush_shader_draft() {
        return None;
    }
    app.refresh_shaders();
    let creating = app.tool == Tool::Event;
    let keyframe = app
        .shaders
        .animation_track()
        .map(str::to_owned)
        .map(|name| {
            (
                name,
                if creating {
                    rows(app).len()
                } else {
                    found.unwrap()
                },
            )
        });
    let effect = if keyframe.is_some() {
        app.shaders.selected?
    } else if creating {
        app.shaders.effects.len()
    } else {
        found?
    };
    let start = at(app, grid, pos.y);
    let tick = Beat::new(0, 1, grid.division).unwrap();
    let end = start.checked_add(tick).unwrap_or(start);
    let raw = if creating {
        if let Some((name, _)) = &keyframe {
            let parameter = app.shaders.parameters.iter().find(|p| p.name == *name)?;
            let value = app
                .shaders
                .draft
                .as_ref()
                .and_then(|d| d.raw["vars"].get(name))
                .filter(|v| uniform_type(v).is_some())
                .cloned()
                .unwrap_or(parameter.default.clone());
            json!({"startTime":start,"endTime":end,"start":value,"end":value,"easingType":1})
        } else {
            let mut raw = json!(ShaderEffect::new(start, end));
            raw["shader"] = json!(PRESETS[app.shaders.preset]);
            layout::place(&mut raw, layout::at(app, grid, false, pos));
            raw
        }
    } else if let Some((name, index)) = &keyframe {
        app.shaders.draft.as_ref()?.raw["vars"][name][*index].clone()
    } else {
        app.shaders.source["effects"][effect].clone()
    };
    let keys = if keyframe.is_some() {
        ["startTime", "endTime"]
    } else {
        ["start", "end"]
    };
    let a: Beat = serde_json::from_value(raw[keys[0]].clone()).ok()?;
    let b: Beat = serde_json::from_value(raw[keys[1]].clone()).ok()?;
    let da = (pos.y - grid.y(a.as_f64())).abs();
    let db = (pos.y - grid.y(b.as_f64())).abs();
    let group = if !creating && in_group {
        selection::items(app, keyframe.is_some())
            .into_iter()
            .filter(|(id, _)| app.shaders.selection.contains(id))
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    let earliest = group
        .iter()
        .filter_map(|(_, raw)| serde_json::from_value::<Beat>(raw[keys[0]].clone()).ok())
        .min()
        .unwrap_or(a);
    let lane = layout::lane(&raw, effect);
    Some(Gesture::Shader(Box::new(ShaderGesture::Edit(Box::new(
        Edit {
            effect,
            keyframe,
            creating,
            original: raw.clone(),
            current: raw,
            origin: pos,
            grid,
            moved: false,
            lane,
            group,
            interval: IntervalDrag {
                original: [a, b],
                current: [a, b],
                origin: at(app, grid, pos.y),
                earliest,
                handle: if creating {
                    Some(false)
                } else {
                    (app.shaders.selection.len() < 2
                        && da.min(db) <= 14.
                        && da.min(db) < (grid.y(a.as_f64()) - grid.y(b.as_f64())).abs() * 0.35)
                        .then_some(da <= db)
                },
            },
        },
    )))))
}

pub(crate) fn select(app: &mut Workbench, id: Option<usize>) {
    app.playing = false;
    app.shaders.selection.clear();
    if let Some(name) = app.shaders.animation_track().map(str::to_owned) {
        if !app.apply_shader_animation() {
            return;
        }
        app.shaders.animation = id.and_then(|i| {
            app.shaders
                .draft
                .as_ref()
                .and_then(|d| animation_draft(&d.raw, &name, i))
        });
    } else if let Some(index) = id {
        app.select_shader(index);
    } else if app.flush_shader_draft() {
        app.shaders.selected = None;
        app.shaders.draft = None;
        app.shaders.parameters.clear();
        app.shaders.animation = None;
        app.shaders.track = None;
    }
    app.show_context = true;
    app.properties_scroll = true;
}

pub(crate) fn update(app: &mut Workbench, gesture: &mut ShaderGesture, pos: Pos2) {
    match gesture {
        ShaderGesture::Navigate {
            grid,
            origin,
            offset,
            horizontal,
        } => {
            let delta = pos - *origin;
            if horizontal.is_none() {
                if delta.y.abs() > 8. && delta.y.abs() >= delta.x.abs() * 0.6 {
                    *horizontal = Some(false);
                } else if delta.x.abs() > 18. && delta.x.abs() > delta.y.abs() * 2. {
                    *horizontal = Some(true);
                }
            }
            if *horizontal == Some(true) {
                app.shaders.offsets[1] = *offset;
                layout::pan(app, *grid, true, -f64::from(delta.x));
            } else if *horizontal == Some(false) {
                app.set_grid_beat(
                    (grid.beat + f64::from(delta.y * grid.scroll_speed / grid.zoom)).max(0.),
                );
            }
        }
        ShaderGesture::Box { current, .. } => *current = pos,
        ShaderGesture::Edit(d) => {
            d.moved |= pos.distance(d.origin) > 8.;
            if d.creating || d.moved {
                d.interval.update(at(app, d.grid, pos.y), d.grid.division);
                let keys = if d.keyframe.is_some() {
                    ["startTime", "endTime"]
                } else {
                    ["start", "end"]
                };
                d.current[keys[0]] = json!(d.interval.current[0]);
                d.current[keys[1]] = json!(d.interval.current[1]);
                if d.keyframe.is_none()
                    && d.interval.handle.is_none()
                    && (pos.x - d.origin.x).abs() > 8.
                {
                    let lane = layout::snap(
                        d.grid,
                        d.lane + f64::from((pos.x - d.origin.x) / layout::EFFECT_WIDTH),
                    );
                    if (lane - d.lane).abs() > f64::EPSILON {
                        layout::place(&mut d.current, lane);
                    }
                }
            }
        }
        ShaderGesture::Erase {
            ids,
            keys,
            grid,
            last,
        } => {
            let steps = ((pos.distance(*last) / 8.).ceil() as usize).clamp(1, 256);
            for i in 1..=steps {
                let point = *last + (pos - *last) * (i as f32 / steps as f32);
                ids.extend(selection::box_hits(
                    app,
                    *grid,
                    *keys,
                    Rect::from_center_size(point, vec2(1., 16.)),
                ));
            }
            *last = pos;
        }
    }
}

pub(crate) fn finish(app: &mut Workbench, gesture: ShaderGesture) {
    match gesture {
        ShaderGesture::Navigate { .. } => {}
        ShaderGesture::Box {
            grid,
            keys,
            origin,
            current,
            additive,
        } => {
            let mut ids = selection::box_hits(app, grid, keys, Rect::from_two_pos(origin, current));
            if additive {
                ids.extend(app.shaders.selection.iter().cloned());
            }
            if let Some(id) = ids.first() {
                if keys {
                    if app.focus_shader_grid(id.parameter.clone()) {
                        select(app, Some(id.index));
                    }
                } else {
                    app.select_shader(id.index);
                }
            }
            if ids.is_empty() {
                select(app, None);
            }
            app.shaders.selection = ids;
        }
        ShaderGesture::Edit(d) => {
            if !d.creating && (!d.moved || d.current == d.original) {
                return;
            }
            if d.group.len() > 1 {
                let values = d
                    .group
                    .iter()
                    .map(|(id, raw)| (id.clone(), selection::shifted(&d, id, raw)))
                    .collect::<Vec<_>>();
                let ids = values.iter().map(|(id, _)| id.clone()).collect();
                if selection::commit_group(app, values) {
                    app.refresh_shaders();
                    app.shaders.selection = ids;
                }
                return;
            }
            if let Some((name, index)) = d.keyframe {
                let mut effect = app.shaders.source["effects"][d.effect].clone();
                if let Some(rows) = effect["vars"][&name]
                    .as_array_mut()
                    .filter(|rows| rows.first().is_some_and(Value::is_object))
                {
                    if d.creating {
                        rows.push(d.current);
                    } else {
                        rows[index] = d.current;
                    }
                } else if d.creating {
                    effect["vars"][&name] = json!([d.current]);
                }
                if app.shader_command(ShaderAction::Update {
                    index: d.effect,
                    effect,
                }) {
                    app.refresh_shaders();
                    app.shaders.animation = app
                        .shaders
                        .draft
                        .as_ref()
                        .and_then(|draft| animation_draft(&draft.raw, &name, index));
                }
            } else {
                let action = if d.creating {
                    ShaderAction::Insert {
                        index: d.effect,
                        effect: d.current,
                    }
                } else {
                    ShaderAction::Update {
                        index: d.effect,
                        effect: d.current,
                    }
                };
                if app.shader_command(action) {
                    app.shaders.selected = Some(d.effect);
                    app.refresh_shaders();
                }
            }
            app.show_context = true;
            app.properties_scroll = true;
        }
        ShaderGesture::Erase { ids, .. } => {
            if !ids.is_empty() {
                selection::delete_group(app, ids);
            }
        }
    }
}
