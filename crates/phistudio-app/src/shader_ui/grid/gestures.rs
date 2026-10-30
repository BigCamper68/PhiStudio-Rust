use super::*;

pub(crate) fn tap(gesture: &ShaderGesture) -> Option<(Grid, Pos2)> {
    match gesture {
        ShaderGesture::Edit(d) if !d.creating && !d.moved => Some((d.grid, d.origin)),
        _ => None,
    }
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
            ids: hits(app, grid, pos).into_iter().collect(),
            grid,
            last: pos,
        })));
    }
    let selected = if app.shaders.animation_track().is_some() {
        app.shaders.animation.as_ref().map(|a| a.index)
    } else {
        app.shaders.selected
    };
    if app.tool != Tool::Event && (found.is_none() || found != selected) {
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
            let lanes = lanes(app);
            let lane = (((pos.x - grid.rect.left()) / grid.rect.width() * lanes.len() as f32)
                .floor() as usize)
                .min(lanes.len() - 1);
            raw["shader"] = json!(lanes[lane].0);
            raw["global"] = json!(lanes[lane].1);
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
            interval: IntervalDrag {
                original: [a, b],
                current: [a, b],
                origin: at(app, grid, pos.y),
                earliest: a,
                handle: if creating {
                    Some(false)
                } else {
                    (da.min(db) <= 14.).then_some(da <= db)
                },
            },
        },
    )))))
}

pub(crate) fn select(app: &mut Workbench, id: Option<usize>) {
    app.playing = false;
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
    }
    app.show_context = true;
    app.properties_scroll = true;
}

pub(crate) fn update(app: &Workbench, gesture: &mut ShaderGesture, pos: Pos2) {
    match gesture {
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
            }
        }
        ShaderGesture::Erase { ids, grid, last } => {
            let steps = ((pos.distance(*last) / 8.).ceil() as usize).clamp(1, 256);
            for i in 1..=steps {
                ids.extend(hits(
                    app,
                    *grid,
                    *last + (pos - *last) * (i as f32 / steps as f32),
                ));
            }
            *last = pos;
        }
    }
}

pub(crate) fn finish(app: &mut Workbench, gesture: ShaderGesture) {
    match gesture {
        ShaderGesture::Edit(d) => {
            if !d.creating && (!d.moved || d.current == d.original) {
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
            if ids.is_empty() {
                return;
            }
            if let Some(name) = app.shaders.animation_track().map(str::to_owned) {
                let Some(index) = app.shaders.selected else {
                    return;
                };
                let mut effect = app.shaders.source["effects"][index].clone();
                if let Some(rows) = effect["vars"][&name].as_array_mut() {
                    let value = rows
                        .first()
                        .map(|r| r["start"].clone())
                        .unwrap_or(json!(0.));
                    for id in ids.iter().rev() {
                        rows.remove(*id);
                    }
                    if rows.is_empty() {
                        effect["vars"][&name] = value;
                    }
                    app.shader_command(ShaderAction::Update { index, effect });
                }
            } else {
                let commands = ids
                    .iter()
                    .rev()
                    .map(|&index| Command::EditShader {
                        baseline: app.shaders.source.clone(),
                        action: ShaderAction::Delete(index),
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
    }
}
