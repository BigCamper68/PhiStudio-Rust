//! A native segment ends at the following point, retaining raw array indices.
use super::*;
#[derive(Clone)]
pub(super) struct EndKey {
    pub index: Option<usize>,
    pub original: Value,
    pub current: Value,
    limit: Option<Beat>,
}
impl EndKey {
    pub fn new(app: &Workbench, next: Option<usize>, fallback: Value, end: Beat) -> Self {
        let values = rows(app);
        let original = next
            .and_then(|i| values.get(i))
            .cloned()
            .unwrap_or(fallback);
        let limit = next.and_then(|_| {
            values
                .iter()
                .filter_map(|v| {
                    noise::beat(v, "time", Format::Native, app.editor.document().tempo()).ok()
                })
                .filter(|b| *b > end)
                .min()
        });
        Self {
            index: next,
            current: original.clone(),
            original,
            limit,
        }
    }
    pub fn changed(&self) -> bool {
        self.current != self.original
    }
}
pub(super) fn update(
    app: &Workbench,
    d: &mut Edit,
    at: Beat,
) -> Result<Value, phistudio_core::ChartError> {
    let tempo = app.editor.document().tempo();
    let tick = Beat::new(0, 1, d.grid.division)?;
    let mut value = d.original.clone();
    if let Some(end) = &mut d.end_key {
        let minimum = d.range[0].checked_add(tick)?;
        let at = if let Some(limit) = end.limit {
            at.max(minimum.min(limit)).min(limit)
        } else {
            at.max(minimum)
        };
        let mut tail = if d.creating {
            noise::new_key(
                &app.noise.rows[app.noise.selected.ok_or_else(|| {
                    phistudio_core::ChartError::Invalid("Noise Field no longer exists".into())
                })?],
                app.noise.track,
                at,
                at,
                tempo,
            )?
        } else {
            end.original.clone()
        };
        noise::put_beat(&mut tail, "time", at, Format::Native, tempo)?;
        end.current = tail;
    } else if d.handle == Some(0) {
        let at = if d.range[1] > d.range[0] {
            at.min(d.range[1].checked_sub(tick)?.max(d.range[0]))
        } else {
            at
        };
        noise::put_beat(&mut value, "time", at, Format::Native, tempo)?;
    } else {
        value = noise::shift_key(
            &d.original,
            app.noise.track,
            Format::Native,
            d.grid.beat(d.origin.y),
            at,
            tempo,
        )?;
    }
    Ok(value)
}
