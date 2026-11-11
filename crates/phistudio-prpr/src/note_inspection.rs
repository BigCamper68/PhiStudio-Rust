//! Bounded geometry queries using the already parsed original prpr animations.
//! No rendering, audio, judging, history or playback cursors are advanced.
mod tracing;
use crate::Preview;
use anyhow::{Result, anyhow, ensure};
use phistudio_sdk::{PreviewInspection, PreviewInspectionPoint, PreviewInspectionResult};
use prpr::core::{AnimFloat, CtrlObject, Matrix, NoteKind, Object, Point, Vector};
pub use tracing::{NoteTraceBoundaries, NoteTraceDestination, NoteTraceSample};

#[derive(Debug, PartialEq)]
struct Cursor(Vec<(f64, usize)>);
impl Cursor {
    fn capture(anim: &AnimFloat) -> Self {
        let mut values = vec![];
        let mut current = Some(anim);
        while let Some(anim) = current {
            values.push((anim.time, anim.cursor));
            current = anim.next.as_deref();
        }
        Self(values)
    }
    fn restore(self, anim: &mut AnimFloat) {
        let mut current = Some(anim);
        for (time, cursor) in self.0 {
            let Some(anim) = current else { break };
            anim.time = time;
            anim.cursor = cursor;
            current = anim.next.as_deref_mut();
        }
    }
}
fn seek(anim: &mut AnimFloat, time: f64) {
    // Native set_time walks from its playback cursor. Binary positioning avoids
    // an unbounded walk when inspecting a distant note in a long event track.
    let mut current = Some(anim);
    while let Some(anim) = current {
        anim.cursor = anim
            .keyframes
            .partition_point(|k| k.time <= time)
            .saturating_sub(1);
        anim.time = time;
        current = anim.next.as_deref_mut();
    }
}
fn animations(object: &mut Object) -> [&mut AnimFloat; 6] {
    [
        &mut object.alpha,
        &mut object.scale.0,
        &mut object.scale.1,
        &mut object.rotation,
        &mut object.translation.0,
        &mut object.translation.1,
    ]
}
fn controls(object: &mut CtrlObject) -> [&mut AnimFloat; 4] {
    [
        &mut object.alpha,
        &mut object.size,
        &mut object.pos,
        &mut object.y,
    ]
}
fn capture(object: &mut Object) -> Vec<Cursor> {
    animations(object)
        .into_iter()
        .map(|a| Cursor::capture(a))
        .collect()
}
fn restore(object: &mut Object, saved: Vec<Cursor>) {
    for (anim, saved) in animations(object).into_iter().zip(saved) {
        saved.restore(anim)
    }
}
fn seek_object(object: &mut Object, time: f64) {
    for anim in animations(object) {
        seek(anim, time)
    }
}
fn weight(anim: &AnimFloat) -> usize {
    std::iter::successors(Some(anim), |a| a.next.as_deref()).count()
}
fn object_weight(object: &Object) -> usize {
    [
        &object.alpha,
        &object.scale.0,
        &object.scale.1,
        &object.rotation,
        &object.translation.0,
        &object.translation.1,
    ]
    .into_iter()
    .map(weight)
    .sum()
}

struct NotePose {
    transform: Matrix,
    origin: Point,
    base: f64,
    speed: f64,
    control_height: Option<f64>,
    rotation: f64,
}
impl Preview {
    fn inspection_note(&mut self, line_index: usize, source_note: usize) -> Result<usize> {
        ensure!(
            line_index < self.anchors.source_lines,
            "Source line is unavailable in the native preview"
        );
        if self.picking.inspection_offsets.is_empty() {
            let mut offset = 0;
            self.picking.inspection_offsets = self
                .chart
                .lines
                .iter()
                .map(|line| {
                    let start = offset;
                    offset += line.notes.len();
                    start
                })
                .collect();
        }
        if let std::collections::btree_map::Entry::Vacant(entry) =
            self.picking.inspection.entry(line_index)
        {
            let count = self.chart.lines[line_index].notes.len();
            ensure!(
                count <= 500_000,
                "Source line exceeds the native inspection note budget"
            );
            let mut index = vec![usize::MAX; count];
            let offset = self.picking.inspection_offsets[line_index];
            for (native, source) in self.picking.sources[offset..offset + count]
                .iter()
                .enumerate()
            {
                if let Some(source) = source
                    && source.line == line_index
                    && let Some(slot) = index.get_mut(source.index)
                {
                    *slot = native
                }
            }
            entry.insert(index);
        }
        let note_index = self.picking.inspection[&line_index]
            .get(source_note)
            .copied()
            .filter(|i| *i != usize::MAX)
            .ok_or_else(|| anyhow!("Source note is unavailable in the native preview"))?;
        Ok(note_index)
    }
    fn note_pose(&self, line_index: usize, note_index: usize) -> NotePose {
        let line = &self.chart.lines[line_index];
        let note = &line.notes[note_index];
        let mut ctrl = line.ctrl_obj.borrow_mut();
        let line_height = f64::from(line.height.now());
        let y = f64::from(note.object.translation.1.now());
        let height = (note.height - line_height + y / note.speed) * 450.;
        for anim in controls(&mut ctrl) {
            seek(anim, height);
        }
        let speed = note.speed * f64::from(ctrl.y.now_opt().unwrap_or(1.));
        let base = (note.height - line_height) / f64::from(self.resource.aspect_ratio) * speed;
        let hold = matches!(note.kind, NoteKind::Hold { .. });
        let transform = line.now_transform(&self.resource, &self.chart.lines)
            * Matrix::new_nonuniform_scaling(&Vector::new(1., if note.above { 1. } else { -1. }))
            * note.now_transform(
                &self.resource,
                &ctrl,
                if hold { 0. } else { base as f32 },
                if hold {
                    0.
                } else {
                    line.incline.now_opt().unwrap_or(0.).to_radians().sin()
                },
            );
        let origin = if hold && self.resource.time < note.time {
            Point::new(0., base as f32)
        } else {
            Point::origin()
        };
        NotePose {
            transform,
            origin,
            base,
            speed,
            control_height: height.is_finite().then_some(height),
            rotation: f64::from(line.fetch_rot(&self.chart.lines))
                + f64::from(note.object.rotation.now()) * if note.above { 1. } else { -1. },
        }
    }
    pub fn inspect_note(&mut self, query: &PreviewInspection) -> Result<PreviewInspectionResult> {
        query.validate().map_err(|e| anyhow!(e))?;
        let line_index = query.line;
        let note_index = self.inspection_note(line_index, query.note)?;
        let mut chain = vec![line_index];
        while let Some(parent) = self.chart.lines[*chain.last().unwrap()].parent {
            ensure!(
                parent < self.chart.lines.len() && !chain.contains(&parent) && chain.len() < 64,
                "Preview parent chain is invalid or exceeds 64 lines"
            );
            chain.push(parent);
        }
        let cost: usize = chain
            .iter()
            .map(|i| {
                let line = &self.chart.lines[*i];
                object_weight(&line.object) + weight(&line.height) + weight(&line.incline)
            })
            .sum();
        let cost =
            (cost + object_weight(&self.chart.lines[line_index].notes[note_index].object) + 4)
                .saturating_mul(chain.len())
                .saturating_mul(query.offsets.len());
        ensure!(
            cost <= 65_536,
            "Native inspection sample budget exceeded; request fewer offsets"
        );
        let mut saved = Vec::with_capacity(chain.len());
        for index in &chain {
            let line = &mut self.chart.lines[*index];
            saved.push((
                capture(&mut line.object),
                Cursor::capture(&line.height),
                Cursor::capture(&line.incline),
            ));
        }
        let note_saved = capture(&mut self.chart.lines[line_index].notes[note_index].object);
        let line = &self.chart.lines[line_index];
        let ctrl = line.ctrl_obj.borrow();
        let ctrl_saved: Vec<_> = [&ctrl.alpha, &ctrl.size, &ctrl.pos, &ctrl.y]
            .into_iter()
            .map(Cursor::capture)
            .collect();
        drop(ctrl);
        let judgment_time = line.notes[note_index].time;
        let end_time = match line.notes[note_index].kind {
            NoteKind::Hold { end_time, .. } => end_time,
            _ => judgment_time,
        };
        let resource_time = self.resource.time;
        let ratio = f64::from(self.resource.aspect_ratio);
        let mut points = Vec::with_capacity(query.offsets.len());
        for offset in &query.offsets {
            let time = judgment_time + offset;
            self.resource.time = time;
            for index in &chain {
                let line = &mut self.chart.lines[*index];
                seek_object(&mut line.object, time);
                seek(&mut line.height, time);
                seek(&mut line.incline, time);
            }
            seek_object(
                &mut self.chart.lines[line_index].notes[note_index].object,
                time,
            );
            let pose = self.note_pose(line_index, note_index);
            let line = &self.chart.lines[line_index];
            let note = &line.notes[note_index];
            let ctrl = line.ctrl_obj.borrow();
            let line_height = f64::from(line.height.now());
            let speed = pose.speed;
            let base = pose.base;
            let display = Matrix::new_nonuniform_scaling(&Vector::new(
                if self.resource.config.flip_x() {
                    -1.
                } else {
                    1.
                },
                -1.,
            ));
            let position = display.transform_point(&pose.transform.transform_point(&pose.origin));
            let position = [
                f64::from(position.x) / 2. + 0.5,
                f64::from(position.y) * ratio / 2. + 0.5,
            ];
            let inside = position
                .iter()
                .all(|v| v.is_finite() && (-1e-7..=1.0000001).contains(v));
            let width = f64::from(
                note.object.scale.0.now_opt().unwrap_or(1.) * ctrl.size.now_opt().unwrap_or(1.),
            );
            let alpha = line.object.alpha.now_opt().unwrap_or(1.);
            let mut visible = inside
                && width != 0.
                && note.object.now_alpha() * ctrl.alpha.now_opt().unwrap_or(1.) > 0.;
            let mut draw_below = line.show_below;
            if alpha < 0. {
                let mode = (-alpha).floor() as u32;
                if !self.chart.settings.pe_alpha_extension || mode == 1 {
                    visible = false
                }
                if mode == 2 {
                    draw_below = false
                }
                if (100..1000).contains(&mode) {
                    let mut bpm = self.chart.bpm_list.borrow_mut();
                    let beat = bpm.beat(judgment_time);
                    if time < bpm.time_beats(beat - f64::from(mode - 100) / 10.) {
                        visible = false
                    }
                }
            }
            if !draw_below && judgment_time > time {
                let cover = match note.kind {
                    NoteKind::Hold { end_height, .. } if self.chart.settings.hold_partial_cover => {
                        (end_height - line_height) / ratio * speed
                    }
                    _ => base,
                };
                visible &= cover > -0.001;
            }
            points.push(PreviewInspectionPoint {
                offset: *offset,
                position: position.iter().all(|v| v.is_finite()).then_some(position),
                inside,
                visible,
                width: width.is_finite().then_some(width),
            });
        }
        self.resource.time = resource_time;
        // BpmList's private cursor is a search cache; return it to the current
        // playback time after alpha-extension queries at diagnostic times.
        self.chart.bpm_list.borrow_mut().beat(resource_time);
        for (index, (object, height, incline)) in chain.into_iter().zip(saved) {
            let line = &mut self.chart.lines[index];
            restore(&mut line.object, object);
            height.restore(&mut line.height);
            incline.restore(&mut line.incline);
        }
        restore(
            &mut self.chart.lines[line_index].notes[note_index].object,
            note_saved,
        );
        let mut ctrl = self.chart.lines[line_index].ctrl_obj.borrow_mut();
        for (anim, saved) in controls(&mut ctrl).into_iter().zip(ctrl_saved) {
            saved.restore(anim);
        }
        Ok(PreviewInspectionResult {
            version: query.version,
            line: query.line,
            note: query.note,
            judgment_time,
            end_time,
            aspect_ratio: ratio,
            points,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prpr::core::Keyframe;
    #[test]
    fn inspection_cursor_uses_native_values_and_restores_chained_playback_state() {
        let mut animation =
            AnimFloat::new(vec![Keyframe::new(0., 2., 2), Keyframe::new(10., 12., 0)]);
        animation.next = Some(Box::new(AnimFloat::fixed(3.)));
        animation.set_time(9.);
        let saved = Cursor::capture(&animation);
        seek(&mut animation, 3.);
        assert_eq!(animation.now(), 8.);
        saved.restore(&mut animation);
        assert_eq!(animation.now(), 14.);
        assert_eq!(animation.next.as_ref().unwrap().time, 9.);
    }
}
