use super::*;

pub(super) fn finish(app: &mut Workbench) {
    if app.timeline.navigation_axis.take() == Some(true)
        && !std::mem::take(&mut app.timeline.input.panel_navigation)
    {
        let step = 1350. / (app.timeline.lanes.max(2) - 1) as f64;
        let nearest = (app.timeline.camera_x / step).round() * step;
        app.timeline.camera_x = if app.timeline.camera_x.abs() < step * 0.45 {
            0.
        } else {
            nearest
        };
    }
    app.timeline.input.pointer_active = false;
    let gesture = app.timeline.drag.take();
    let preserve_properties_position = match gesture.as_ref() {
        Some(
            Gesture::Move { moved, .. }
            | Gesture::Resize { moved, .. }
            | Gesture::SelectionMove { moved, .. }
            | Gesture::SelectionScale { moved, .. },
        ) => *moved,
        Some(Gesture::Event(crate::event_grid::EventGesture::Edit { moved, .. })) => *moved,
        Some(Gesture::Story(story)) => story.id.is_some() && story.moved,
        Some(Gesture::Shader(shader)) => shader.edits_existing(),
        Some(Gesture::Noise(noise)) => noise.edits_existing(),
        _ => false,
    };
    let properties_scroll = app.windows.scroll_to_properties;
    if let Some(gesture) = &gesture
        && crate::picking::finish_tap(app, gesture)
    {
        return;
    }
    match gesture {
        Some(Gesture::SelectionScale {
            selection,
            anchor,
            factor,
            moved: true,
            ..
        }) => {
            let edit = phistudio_core::tools::transform(
                app.editor.document(),
                &selection,
                Transform::ScaleX { anchor, factor },
            );
            app.apply_tool(edit);
        }
        Some(Gesture::Box {
            origin,
            current,
            grid,
            additive,
        }) => {
            if origin.distance(current) <= SLOP {
                if grid.rect.contains(current) {
                    let id = pick(app, grid, current).map(|(id, _)| id);
                    app.toggle_note(id, additive);
                } else if grid.event_rect.contains(current) {
                    if crate::story_grid::active(app) {
                        let id = crate::story_grid::hits(app, grid.events(), current)
                            .first()
                            .copied();
                        app.toggle_story(id, additive);
                    } else {
                        let id =
                            crate::event_grid::pick(app, grid.events(), current).map(|(id, _)| id);
                        app.toggle_event(id, additive);
                    }
                }
                app.transport.playing = false;
                return;
            }
            if Rect::from_two_pos(origin, current).intersects(grid.rect)
                && Rect::from_two_pos(origin, current).intersects(grid.event_rect)
            {
                app.tools.scope = crate::toolkit::Scope::Both;
            }
            let mut selection = box_selection(app, grid, origin, current);
            if additive {
                let old = app.selection();
                selection.notes.extend(old.notes);
                selection.events.extend(old.events);
            }
            let mut stories = if crate::story_grid::active(app) {
                crate::story_grid::box_hits(app, grid.events(), Rect::from_two_pos(origin, current))
            } else {
                vec![]
            };
            if additive {
                stories.extend(app.story_selection());
            }
            app.set_object_selection(selection, stories);
            app.transport.playing = false;
        }
        Some(Gesture::Erase {
            selection, stories, ..
        }) => {
            let anchor = app.anchor();
            let result = app.link_selection(&selection).and_then(|selection| {
                app.editor.erase_objects(
                    &selection,
                    stories
                        .into_iter()
                        .map(|id| (id.line, id.kind, id.index))
                        .collect(),
                )
            });
            match result {
                Ok(changed) => {
                    if changed {
                        app.remember_anchor(anchor);
                        app.transport.playing = false;
                        app.select(None);
                    }
                    app.error = None;
                }
                Err(error) => app.error = Some(error.to_string()),
            }
        }
        Some(Gesture::SelectionMove {
            selection,
            stories,
            beats,
            x,
            moved: true,
            ..
        }) => {
            app.apply_object_tool(phistudio_core::objects::transform(
                app.editor.document(),
                &selection,
                &stories,
                Transform::Shift { beats, x },
            ));
        }
        Some(Gesture::Tap {
            origin,
            grid,
            intent,
            moved: false,
        }) => match intent {
            Intent::SelectEvent(id, additive) => app.toggle_event(id, additive),
            Intent::SelectShader(id) => crate::shader_ui::grid::select(app, id),
            Intent::SelectStory(id, additive) => {
                app.transport.playing = false;
                app.toggle_story(id, additive);
            }
            Intent::Place(kind) => {
                app.transport.playing = false;
                let start = grid.beat(origin.y);
                add(app, kind, start, start, grid.note_x(origin.x));
            }
            Intent::Select(id, additive) => {
                app.transport.playing = false;
                app.toggle_note(id, additive);
            }
            Intent::Seek => {
                app.set_grid_beat(grid.beat(origin.y).as_f64());
            }
            Intent::Pan => {}
        },
        Some(Gesture::Create {
            start,
            end,
            x,
            kind,
            ..
        }) => {
            app.transport.playing = false;
            add(app, kind, start, end, x);
        }
        Some(Gesture::Event(gesture)) => crate::event_grid::finish(app, gesture),
        Some(Gesture::Story(d)) => crate::story_grid::finish(app, *d),
        Some(Gesture::Noise(d)) => crate::noise_ui::grid::finish(app, *d),
        Some(Gesture::Shader(d)) => crate::shader_ui::grid::finish(app, *d),
        Some(
            Gesture::Move {
                id,
                current,
                moved: true,
                ..
            }
            | Gesture::Resize {
                id,
                current,
                moved: true,
                ..
            },
        ) => {
            app.transport.playing = false;
            app.execute(Command::UpdateNote { id, note: current });
        }
        _ => {}
    }
    if preserve_properties_position {
        // Commits can remap source IDs and reselect the edited objects. Only an
        // explicit selection/creation should reveal Properties, not that remap.
        app.windows.scroll_to_properties = properties_scroll;
    }
}
