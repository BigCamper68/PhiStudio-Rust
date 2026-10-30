use super::*;

pub fn begin(app: &mut Workbench, grid: Grid, pos: Pos2) -> Gesture {
    let pending = |id| Gesture::Tap {
        origin: pos,
        grid,
        intent: Intent::SelectEvent(id, false),
        moved: false,
    };
    let kind = kind_at(grid, pos);
    if app.tool == Tool::Event && app.events.layer < 4 {
        let start = grid.beat(pos.y);
        let end = start
            .checked_add(Beat::new(0, 1, grid.division).unwrap())
            .unwrap_or(start);
        match app
            .editor
            .document()
            .event_candidate(app.line, app.events.layer, kind, start, end)
        {
            Ok(event) => {
                return Gesture::Event(EventGesture::Create {
                    line: app.line,
                    kind,
                    event,
                    grid,
                });
            }
            Err(e) => app.error = Some(e.to_string()),
        }
    }
    let hits = hits(app, grid, pos);
    let picked = if app.selection().len() == 1 {
        hits.iter()
            .find(|(id, _)| Some(*id) == app.events.selected)
            .cloned()
    } else {
        None
    }
    .or_else(|| hits.first().cloned());
    let Some((id, event)) = picked else {
        return pending(None);
    };
    if app.events.selected != Some(id) || id.layer > 3 {
        return pending(Some(id));
    }
    let start = (pos.y - grid.y(event.start_time.as_f64())).abs();
    let end = (pos.y - grid.y(event.end_time.as_f64())).abs();
    Gesture::Event(EventGesture::Edit {
        id,
        original: event.clone(),
        current: event,
        origin: pos,
        grid,
        handle: (start.min(end) <= 14.).then_some(start <= end),
        moved: false,
    })
}

pub fn update(gesture: &mut EventGesture, pos: Pos2) {
    match gesture {
        EventGesture::Create { event, grid, .. } => {
            event.end_time = grid.beat(pos.y).max(
                event
                    .start_time
                    .checked_add(Beat::new(0, 1, grid.division).unwrap())
                    .unwrap_or(event.start_time),
            )
        }
        EventGesture::Edit {
            original,
            current,
            origin,
            grid,
            handle,
            moved,
            ..
        } => {
            *moved |= pos.distance(*origin) > 8.;
            if !*moved {
                return;
            }
            let tick = Beat::new(0, 1, grid.division).unwrap();
            if let Some(start) = handle {
                if *start {
                    current.start_time = grid
                        .beat(pos.y)
                        .min(current.end_time.checked_sub(tick).unwrap_or(Beat::ZERO))
                        .max(Beat::ZERO);
                } else {
                    current.end_time = grid.beat(pos.y).max(
                        current
                            .start_time
                            .checked_add(tick)
                            .unwrap_or(current.start_time),
                    );
                }
            } else if let Ok(delta) = grid.beat(pos.y).checked_sub(grid.beat(origin.y))
                && let Ok(start) = original.start_time.checked_add(delta)
            {
                let start = start.max(Beat::ZERO);
                if let Ok(end) = start
                    .checked_sub(original.start_time)
                    .and_then(|d| original.end_time.checked_add(d))
                {
                    current.start_time = start;
                    current.end_time = end;
                }
            }
        }
    }
}

pub fn finish(app: &mut Workbench, gesture: EventGesture) {
    match gesture {
        EventGesture::Create {
            line, kind, event, ..
        } => {
            app.commit_authoring(Command::AddEvent {
                line,
                layer: app.events.layer,
                kind,
                event,
            });
        }
        EventGesture::Edit {
            id,
            current,
            moved: true,
            ..
        } => {
            app.commit_authoring(Command::UpdateEvent { id, event: current });
        }
        _ => {}
    }
}
