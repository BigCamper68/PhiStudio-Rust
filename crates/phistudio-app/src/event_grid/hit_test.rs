use super::*;

pub(super) fn column(grid: Grid, kind: EventType) -> Rect {
    let index = EventType::ALL.iter().position(|v| *v == kind).unwrap();
    let width = grid.rect.width() / 5.;
    Rect::from_min_max(
        pos2(grid.rect.left() + index as f32 * width, grid.rect.top()),
        pos2(
            grid.rect.left() + (index + 1) as f32 * width,
            grid.rect.bottom(),
        ),
    )
}

pub(super) fn kind_at(grid: Grid, pos: Pos2) -> EventType {
    EventType::ALL[(((pos.x - grid.rect.left()) / grid.rect.width() * 5.).floor() as usize).min(4)]
}

pub fn event_rect(grid: Grid, kind: EventType, event: &LineEvent) -> Rect {
    let col = column(grid, kind).shrink2(vec2(4., 0.));
    Rect::from_min_max(
        pos2(col.left(), grid.y(event.end_time.as_f64())),
        pos2(col.right(), grid.y(event.start_time.as_f64())),
    )
}

pub(crate) fn hits(app: &mut Workbench, grid: Grid, pos: Pos2) -> Vec<(EventId, LineEvent)> {
    if !grid.rect.contains(pos) {
        return vec![];
    }
    let kind = kind_at(grid, pos);
    app.event_rows()
        .iter()
        .rev()
        .filter(|(id, event)| {
            id.kind == kind
                && pos.y >= grid.y(event.end_time.as_f64()) - 8.
                && pos.y <= grid.y(event.start_time.as_f64()) + 8.
        })
        .cloned()
        .collect()
}

pub fn pick(app: &mut Workbench, grid: Grid, pos: Pos2) -> Option<(EventId, LineEvent)> {
    hits(app, grid, pos).into_iter().next()
}
