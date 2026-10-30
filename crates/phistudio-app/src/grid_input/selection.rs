use super::*;

fn note_hits_one(app: &mut Workbench, grid: Grid, pos: Pos2) -> Vec<(NoteId, Note)> {
    if !grid.rect.contains(pos) {
        return vec![];
    }
    let notes = app.notes();
    app.note_candidates(grid.raw_beat(pos.y + 27.), grid.raw_beat(pos.y - 27.))
        .into_iter()
        .rev()
        .filter_map(|candidate| {
            let (index, note) = &notes[candidate];
            let y = if note.kind == NoteKind::Hold {
                pos.y.clamp(
                    grid.y(note.end_time.as_f64()),
                    grid.y(note.start_time.as_f64()),
                )
            } else {
                grid.y(note.start_time.as_f64())
            };
            (((pos.x - grid.x(note.x)) / 26_f32.max(grid.note_width / 2.)).powi(2)
                + ((pos.y - y) / 26.).powi(2)
                <= 1.)
                .then(|| {
                    (
                        NoteId {
                            line: app.line,
                            index: *index,
                        },
                        note.clone(),
                    )
                })
        })
        .collect()
}

pub(super) fn pick(app: &mut Workbench, grid: Grid, pos: Pos2) -> Option<(NoteId, Note)> {
    note_hits(app, grid, pos).into_iter().next()
}

pub(super) fn pick_for_drag(app: &mut Workbench, grid: Grid, pos: Pos2) -> Option<(NoteId, Note)> {
    let hits = note_hits(app, grid, pos);
    if app.selection().len() == 1
        && let Some(selected) = app.selected
        && let Some(found) = hits.iter().find(|(id, _)| *id == selected)
    {
        return Some(found.clone());
    }
    hits.into_iter().next()
}

pub(super) fn selection_move(
    app: &mut Workbench,
    grid: Grid,
    pos: Pos2,
    note_surface: bool,
) -> Gesture {
    let selection = match app.timing_selection() {
        Ok(s) => s,
        Err(e) => {
            app.error = Some(e.to_string());
            return Gesture::Tap {
                origin: pos,
                grid,
                intent: Intent::Pan,
                moved: false,
            };
        }
    };
    let stories = app.story_selection();
    let earliest =
        phistudio_core::objects::ObjectClipboard::copy(app.editor.document(), &selection, &stories)
            .map_or(Beat::ZERO, |c| c.first);
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for id in &selection.notes {
        if let Ok(note) = app.editor.document().note(id.line, id.index) {
            lo = lo.min(note.x);
            hi = hi.max(note.x);
        }
    }
    Gesture::SelectionMove {
        selection,
        stories,
        origin: pos,
        grid,
        beats: Beat::ZERO,
        x: 0.,
        earliest,
        x_range: [lo, hi],
        note_surface,
        moved: false,
    }
}

pub(super) fn erase_at(app: &mut Workbench, grid: Grid, pos: Pos2) -> Selection {
    let grid = app.multi_base(grid);
    let mut selection = Selection::default();
    if grid.rect.contains(pos) {
        selection
            .notes
            .extend(note_hits(app, grid, pos).into_iter().map(|(id, _)| id));
    } else if grid.event_rect.contains(pos) && !crate::story_grid::active(app) {
        let line = app.line;
        for (source, panel) in app.line_panels(grid, true) {
            app.line = source;
            selection.events.extend(
                crate::event_grid::pick(app, panel.events(), pos)
                    .map(|(id, _)| id)
                    .filter(|id| id.layer < 4),
            );
        }
        app.line = line;
    }
    selection.normalized()
}

pub(crate) fn note_hits(app: &mut Workbench, grid: Grid, pos: Pos2) -> Vec<(NoteId, Note)> {
    let base = app.multi_base(grid);
    if !base.rect.contains(pos) {
        return vec![];
    }
    let line = app.line;
    let mut hits = Vec::new();
    for (source, panel) in app.line_panels(base, false).into_iter().rev() {
        if !panel.rect.contains(pos) {
            continue;
        }
        app.line = source;
        hits.extend(note_hits_one(app, panel, pos));
    }
    app.line = line;
    hits
}

pub(super) fn box_selection(
    app: &mut Workbench,
    grid: Grid,
    origin: Pos2,
    current: Pos2,
) -> Selection {
    let grid = app.multi_base(grid);
    let line = app.line;
    let mut selected = Selection::default();
    for (source, panel) in app.line_panels(grid, false) {
        app.line = source;
        let s = box_single(
            app,
            Grid {
                event_rect: Rect::NOTHING,
                ..panel
            },
            origin,
            current,
        );
        selected.notes.extend(s.notes);
    }
    if !crate::story_grid::active(app) {
        for (source, panel) in app.line_panels(grid, true) {
            app.line = source;
            let s = box_single(
                app,
                Grid {
                    rect: Rect::NOTHING,
                    ..panel
                },
                origin,
                current,
            );
            selected.events.extend(s.events);
        }
    }
    app.line = line;
    selected.normalized()
}

fn box_single(app: &mut Workbench, grid: Grid, origin: Pos2, current: Pos2) -> Selection {
    let rect = Rect::from_two_pos(origin, current);
    let mut selection = Selection::default();
    if rect.intersects(grid.rect) {
        let notes = app.notes();
        for candidate in app.note_candidates(
            grid.raw_beat(rect.bottom() + 7.),
            grid.raw_beat(rect.top() - 7.),
        ) {
            let (index, note) = &notes[candidate];
            if rect.intersects(grid.note_rect(note).intersect(grid.rect)) {
                selection.notes.push(NoteId {
                    line: app.line,
                    index: *index,
                });
            }
        }
    }
    if rect.intersects(grid.event_rect) && !crate::story_grid::active(app) {
        for (id, event) in app.event_rows().iter() {
            if rect.intersects(
                crate::event_grid::event_rect(grid.events(), id.kind, event)
                    .intersect(grid.event_rect),
            ) {
                selection.events.push(*id);
            }
        }
    }
    selection.normalized()
}

pub(super) fn add(app: &mut Workbench, kind: NoteKind, start: Beat, end: Beat, x: f64) {
    let index = app.notes().len();
    app.execute(Command::AddNote {
        line: app.line,
        note: Note {
            kind,
            start_time: start,
            end_time: end,
            x,
            ..Note::default()
        },
    });
    if app.error.is_none() {
        app.select(Some(NoteId {
            line: app.line,
            index,
        }));
    }
}
