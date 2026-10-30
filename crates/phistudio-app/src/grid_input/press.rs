use super::*;

pub(super) fn begin(app: &mut Workbench, grid: Grid, pos: Pos2, additive: bool) {
    app.grid_input.pointer_active = true;
    app.navigation_axis = None;
    app.grid_input.panel_navigation = false;
    app.grid_input.panel_offset = app.multi_line.offsets[usize::from(app.multi_line.events)];
    if grid.rect.contains(pos) {
        app.tools.scope = crate::toolkit::Scope::Notes;
    } else if grid.event_rect.contains(pos) {
        app.tools.scope = crate::toolkit::Scope::Events;
    }
    if !additive
        && grid.rect.contains(pos)
        && let Some(gesture) = crate::selection_scaling::begin(app, grid, pos)
    {
        let mut gesture = gesture;
        if let Gesture::Box { grid, .. } | Gesture::Erase { grid, .. } = &mut gesture {
            *grid = app.multi_base(*grid);
        }
        app.drag = Some(gesture);
        return;
    }
    if grid.event_rect.contains(pos)
        && app.advanced.right_mode == crate::advanced_ui::RightMode::Shaders
    {
        app.drag = crate::shader_ui::grid::begin(app, grid.events(), pos);
        return;
    }
    if grid.event_rect.contains(pos)
        && crate::story_grid::active(app)
        && app.tool != Tool::Erase
        && app.tool != Tool::BoxSelect
    {
        app.drag = Some(crate::story_grid::begin(app, grid.events(), pos, additive));
        return;
    }
    let pending = |intent| Gesture::Tap {
        origin: pos,
        grid,
        intent,
        moved: false,
    };
    let gesture = if pos.x < grid.rect.left() {
        pending(Intent::Seek)
    } else if app.tool == Tool::BoxSelect {
        let selected = app.selection();
        let note = grid.rect.contains(pos)
            && pick(app, grid, pos)
                .is_some_and(|(id, _)| selected.notes.binary_search(&id).is_ok());
        let story = crate::story_grid::active(app)
            && grid.event_rect.contains(pos)
            && crate::story_grid::hits(app, grid.events(), pos)
                .iter()
                .any(|id| app.story_selection().contains(id));
        let event = !crate::story_grid::active(app)
            && grid.event_rect.contains(pos)
            && crate::event_grid::pick(app, grid.events(), pos)
                .is_some_and(|(id, _)| selected.events.binary_search(&id).is_ok());
        if !additive && story {
            crate::story_grid::begin(app, grid.events(), pos, false)
        } else if !additive && (note || event) {
            selection_move(app, grid, pos, note)
        } else {
            Gesture::Box {
                origin: pos,
                current: pos,
                grid,
                additive,
            }
        }
    } else if app.tool == Tool::Erase {
        Gesture::Erase {
            stories: if crate::story_grid::active(app) {
                crate::story_grid::hits(app, grid.events(), pos)
                    .into_iter()
                    .collect()
            } else {
                Default::default()
            },
            selection: erase_at(app, grid, pos),
            grid,
            last: pos,
        }
    } else if grid.event_rect.contains(pos) {
        app.tools.scope = crate::toolkit::Scope::Events;
        let picked = crate::event_grid::pick(app, grid.events(), pos).map(|(id, _)| id);
        if additive && app.tool != Tool::Event {
            pending(Intent::SelectEvent(picked, true))
        } else if app.tool != Tool::Event
            && picked.is_some_and(|id| app.selection().events.binary_search(&id).is_ok())
            && app.selection().len() + app.story_selection().len() > 1
        {
            selection_move(app, grid, pos, false)
        } else {
            crate::event_grid::begin(app, grid.events(), pos)
        }
    } else {
        match app.tool {
            Tool::BoxSelect | Tool::Erase => pending(Intent::Pan),
            Tool::Place(NoteKind::Hold) => {
                let start = grid.beat(pos.y);
                Gesture::Create {
                    start,
                    end: start
                        .checked_add(Beat::new(0, 1, grid.division).unwrap())
                        .unwrap_or(start),
                    x: grid.note_x(pos.x),
                    kind: NoteKind::Hold,
                    grid,
                }
            }
            Tool::Place(kind) => pending(Intent::Place(kind)),
            Tool::Select | Tool::Event => {
                app.tools.scope = crate::toolkit::Scope::Notes;
                let picked = if additive {
                    pick(app, grid, pos)
                } else {
                    pick_for_drag(app, grid, pos)
                };
                if let Some((id, note)) = picked {
                    app.line = id.line;
                    if additive {
                        pending(Intent::Select(Some(id), true))
                    } else if app.selection().notes.binary_search(&id).is_ok()
                        && app.selection().len() + app.story_selection().len() > 1
                    {
                        selection_move(app, grid, pos, true)
                    } else if app.selected == Some(id) {
                        let start =
                            pos.distance(pos2(grid.x(note.x), grid.y(note.start_time.as_f64())));
                        let end =
                            pos.distance(pos2(grid.x(note.x), grid.y(note.end_time.as_f64())));
                        if note.kind == NoteKind::Hold && start.min(end) <= 14. {
                            Gesture::Resize {
                                id,
                                current: note,
                                start_handle: start <= end,
                                origin: pos,
                                grid,
                                moved: false,
                            }
                        } else {
                            Gesture::Move {
                                id,
                                original: note.clone(),
                                current: note,
                                origin: pos,
                                grid,
                                moved: false,
                            }
                        }
                    } else {
                        pending(Intent::Select(Some(id), false))
                    }
                } else {
                    pending(Intent::Select(None, additive))
                }
            }
        }
    };
    let mut gesture = gesture;
    if let Gesture::Box { grid, .. } | Gesture::Erase { grid, .. } = &mut gesture {
        *grid = app.multi_base(*grid);
    }
    app.drag = Some(gesture);
}
