//! Native note poses expressed in a destination's authored coordinate space.
use super::*;
use phistudio_core::NoteId;

/// Native animation-chain indices captured from the destination's RPE layers.
#[derive(Clone, Copy, Debug)]
pub struct NoteTraceDestination {
    pub line: usize,
    pub components: [Option<usize>; 3],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteTraceSample {
    pub seconds: f64,
    pub position: [f64; 2],
    pub rotation: f64,
    pub control_height: Option<f64>,
    pub screen_position: [f64; 2],
}
pub struct NoteTraceBoundaries {
    pub times: Vec<f64>,
    pub control_heights: Vec<f64>,
}
fn parents(preview: &Preview, start: usize) -> Result<Vec<usize>> {
    ensure!(
        start < preview.chart.lines.len(),
        "Tracing line is unavailable"
    );
    let mut chain = vec![start];
    while let Some(parent) = preview.chart.lines[*chain.last().unwrap()].parent {
        ensure!(
            parent < preview.chart.lines.len() && !chain.contains(&parent) && chain.len() < 64,
            "Tracing parent chain is invalid or exceeds 64 lines"
        );
        chain.push(parent);
    }
    Ok(chain)
}
fn component(anim: &AnimFloat, index: Option<usize>) -> f64 {
    let Some(index) = index else {
        return 0.;
    };
    let Some(anim) = std::iter::successors(Some(anim), |a| a.next.as_deref()).nth(index) else {
        return 0.;
    };
    f64::from(anim.now() - anim.next.as_ref().map_or(0., |next| next.now()))
}
impl Preview {
    pub fn note_trace_boundaries(
        &mut self,
        source: NoteId,
        destination: Option<NoteTraceDestination>,
        range: [f64; 2],
    ) -> Result<NoteTraceBoundaries> {
        ensure!(
            range.iter().all(|t| t.is_finite()) && range[1] > range[0],
            "Choose an ordered tracing time range"
        );
        let native = self.inspection_note(source.line, source.index)?;
        let mut chain = parents(self, source.line)?;
        if let Some(target) = destination {
            for line in parents(self, target.line)? {
                if !chain.contains(&line) {
                    chain.push(line);
                }
            }
        }
        let mut times = Vec::new();
        let mut append = |anim: &AnimFloat| -> Result<()> {
            for anim in std::iter::successors(Some(anim), |a| a.next.as_deref()) {
                let start = anim.keyframes.partition_point(|k| k.time <= range[0]);
                let end = anim.keyframes.partition_point(|k| k.time < range[1]);
                ensure!(
                    times.len() + end.saturating_sub(start)
                        <= phistudio_core::note_tracing::MAX_SAMPLES,
                    "Tracing has too many native boundaries; choose a shorter range"
                );
                times.extend(anim.keyframes[start..end].iter().map(|k| k.time));
            }
            Ok(())
        };
        for index in chain {
            let line = &self.chart.lines[index];
            for anim in [
                &line.object.alpha,
                &line.object.translation.0,
                &line.object.translation.1,
                &line.object.rotation,
                &line.object.scale.0,
                &line.object.scale.1,
                &line.height,
                &line.incline,
            ] {
                append(anim)?;
            }
        }
        let note = &self.chart.lines[source.line].notes[native];
        let object = &note.object;
        for anim in [
            &object.translation.0,
            &object.translation.1,
            &object.rotation,
            &object.scale.0,
            &object.scale.1,
        ] {
            append(anim)?;
        }
        for time in [
            Some(note.time),
            match note.kind {
                NoteKind::Hold { end_time, .. } => Some(end_time),
                _ => None,
            },
        ]
        .into_iter()
        .flatten()
        {
            if time > range[0] && time < range[1] {
                times.push(time);
            }
        }
        let control = self.chart.lines[source.line].ctrl_obj.borrow();
        let mut control_heights = Vec::new();
        for anim in [&control.pos, &control.y] {
            for anim in std::iter::successors(Some(anim), |a| a.next.as_deref()) {
                ensure!(
                    control_heights.len() + anim.keyframes.len() <= 4096,
                    "Tracing has too many distance control boundaries"
                );
                control_heights.extend(
                    anim.keyframes
                        .iter()
                        .map(|k| k.time)
                        .filter(|t| t.is_finite()),
                );
            }
        }
        control_heights.sort_by(f64::total_cmp);
        control_heights.dedup();
        Ok(NoteTraceBoundaries {
            times,
            control_heights,
        })
    }

    /// Bounded incremental sampling preserves the renderer's animation cursors,
    /// music clock, judgment state and control curves after every batch.
    pub fn sample_note_trace(
        &mut self,
        source: NoteId,
        destination: Option<NoteTraceDestination>,
        times: &[f64],
    ) -> Result<Vec<NoteTraceSample>> {
        ensure!(
            !times.is_empty() && times.len() <= 64 && times.iter().all(|t| t.is_finite()),
            "Choose 1–64 finite tracing times"
        );
        let native = self.inspection_note(source.line, source.index)?;
        let mut chain = parents(self, source.line)?;
        if let Some(target) = destination {
            for line in parents(self, target.line)? {
                if !chain.contains(&line) {
                    chain.push(line);
                }
            }
        }
        let weight: usize = chain
            .iter()
            .map(|i| {
                let line = &self.chart.lines[*i];
                object_weight(&line.object)
                    + super::weight(&line.height)
                    + super::weight(&line.incline)
            })
            .sum();
        let cost =
            (weight + object_weight(&self.chart.lines[source.line].notes[native].object) + 4)
                .saturating_mul(chain.len())
                .saturating_mul(times.len());
        ensure!(
            cost <= 65_536,
            "Native tracing sample budget exceeded; request fewer times"
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
        let note_saved = capture(&mut self.chart.lines[source.line].notes[native].object);
        let ctrl = self.chart.lines[source.line].ctrl_obj.borrow();
        let ctrl_saved: Vec<_> = [&ctrl.alpha, &ctrl.size, &ctrl.pos, &ctrl.y]
            .into_iter()
            .map(Cursor::capture)
            .collect();
        drop(ctrl);
        let resource_time = self.resource.time;
        let result = (|| -> Result<Vec<NoteTraceSample>> {
            let mut samples = Vec::with_capacity(times.len());
            for &seconds in times {
                self.resource.time = seconds;
                for index in &chain {
                    let line = &mut self.chart.lines[*index];
                    seek_object(&mut line.object, seconds);
                    seek(&mut line.height, seconds);
                    seek(&mut line.incline, seconds);
                }
                seek_object(
                    &mut self.chart.lines[source.line].notes[native].object,
                    seconds,
                );
                let pose = self.note_pose(source.line, native);
                let mut position = pose.transform.transform_point(&pose.origin);
                let screen_position = [
                    f64::from(position.x)
                        * if self.resource.config.flip_x() {
                            -0.5
                        } else {
                            0.5
                        }
                        + 0.5,
                    -f64::from(position.y) * f64::from(self.resource.aspect_ratio) * 0.5 + 0.5,
                ];
                let mut rotation = pose.rotation;
                let mut other = [0.; 3];
                if let Some(destination) = destination {
                    let line = &self.chart.lines[destination.line];
                    if let Some(parent) = line.parent {
                        let parent = &self.chart.lines[parent];
                        let inverse = parent
                            .now_transform(&self.resource, &self.chart.lines)
                            .try_inverse()
                            .ok_or_else(|| anyhow!("Destination parent transform is singular"))?;
                        position = inverse.transform_point(&position);
                        if line.rot_with_parent {
                            rotation -= f64::from(parent.fetch_rot(&self.chart.lines));
                        }
                    }
                    for (slot, (anim, selected)) in other.iter_mut().zip([
                        (&line.object.translation.0, destination.components[0]),
                        (&line.object.translation.1, destination.components[1]),
                        (&line.object.rotation, destination.components[2]),
                    ]) {
                        *slot = f64::from(anim.now()) - component(anim, selected);
                    }
                }
                let sample = NoteTraceSample {
                    seconds,
                    control_height: pose.control_height,
                    screen_position,
                    position: [
                        (f64::from(position.x) - other[0]) * 675.,
                        (f64::from(position.y) * f64::from(self.resource.aspect_ratio) - other[1])
                            * 450.,
                    ],
                    rotation: -(rotation - other[2]),
                };
                ensure!(
                    sample
                        .position
                        .iter()
                        .chain([&sample.rotation])
                        .all(|v| v.is_finite() && v.abs() <= 1e9),
                    "The native note pose exceeds the editable tracing range"
                );
                samples.push(sample);
            }
            Ok(samples)
        })();
        self.resource.time = resource_time;
        for (index, (object, height, incline)) in chain.into_iter().zip(saved) {
            let line = &mut self.chart.lines[index];
            restore(&mut line.object, object);
            height.restore(&mut line.height);
            incline.restore(&mut line.incline);
        }
        restore(
            &mut self.chart.lines[source.line].notes[native].object,
            note_saved,
        );
        let mut ctrl = self.chart.lines[source.line].ctrl_obj.borrow_mut();
        for (anim, saved) in controls(&mut ctrl).into_iter().zip(ctrl_saved) {
            saved.restore(anim);
        }
        result
    }
}
