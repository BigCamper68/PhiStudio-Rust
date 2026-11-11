//! Refine distance-control crossings through the original native animations.
use super::*;
struct Root {
    time: [f64; 2],
    threshold: f64,
    below: bool,
}
pub(super) struct Refinement {
    pub sample: Sampling,
    roots: Vec<Root>,
    cursor: usize,
}
impl Refinement {
    pub fn new(sample: Sampling, cancel: &AtomicBool) -> Result<Self, String> {
        let mut roots = Vec::new();
        for (index, pair) in sample.samples.windows(2).enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err("Tracing canceled".into());
            }
            if sample.times[index + 1].break_before {
                continue;
            }
            let (Some(a), Some(b)) = (pair[0].control_height, pair[1].control_height) else {
                continue;
            };
            if (a - b).abs() < 1e-9 || pair[1].seconds - pair[0].seconds <= 1e-7 {
                continue;
            }
            let start = sample.control_heights.partition_point(|h| *h < a.min(b));
            let end = sample.control_heights.partition_point(|h| *h <= a.max(b));
            for &threshold in &sample.control_heights[start..end] {
                // A crossing exactly at the left sample is already covered by
                // the preceding interval; the right sample belongs here.
                if (a - threshold).abs() < 1e-9 {
                    continue;
                }
                if sample.times.len() + (roots.len() + 1) * 2 > note_tracing::MAX_SAMPLES {
                    return Err("Tracing exceeds 60,000 samples including distance controls; choose a shorter range".into());
                }
                roots.push(Root {
                    time: [pair[0].seconds, pair[1].seconds],
                    threshold,
                    below: a < threshold,
                });
            }
        }
        Ok(Self {
            sample,
            roots,
            cursor: 0,
        })
    }
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }
    pub fn advance(&mut self, preview: &mut Preview) -> Result<bool, String> {
        let deadline = Instant::now() + std::time::Duration::from_millis(3);
        let destination = self
            .sample
            .snapshot
            .spec
            .destination
            .map(|line| NoteTraceDestination {
                line,
                components: self.sample.snapshot.native_components,
            });
        while self.cursor < self.roots.len() && Instant::now() < deadline {
            let root = &mut self.roots[self.cursor];
            if root.time[1] - root.time[0] <= 1e-7 {
                self.cursor += 1;
                continue;
            }
            let time = (root.time[0] + root.time[1]) * 0.5;
            let sample = preview
                .sample_note_trace(self.sample.snapshot.spec.source, destination, &[time])
                .map_err(|e| format!("{e:#}"))?;
            let height = sample[0]
                .control_height
                .ok_or("Distance control is undefined at the crossing")?;
            root.time[usize::from((height < root.threshold) != root.below)] = time;
        }
        Ok(self.cursor == self.roots.len())
    }
    pub fn finish(self, cancel: &AtomicBool) -> Result<Completed, String> {
        let mut sample = self.sample;
        let mut cuts: Vec<_> = sample
            .times
            .iter()
            .filter(|t| t.break_before)
            .map(|t| t.seconds)
            .collect();
        cuts.extend(self.roots.into_iter().map(|r| r.time[1]));
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 2e-7);
        if cancel.load(Ordering::Relaxed) {
            return Err("Tracing canceled".into());
        }
        sample.times =
            note_tracing::sampling_times(&sample.snapshot.spec, &sample.snapshot.tempo, &cuts)
                .map_err(|e| e.to_string())?;
        Ok(Completed::Schedule {
            snapshot: sample.snapshot,
            times: sample.times,
            control_heights: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(heights: [f64; 3]) -> Sampling {
        let mut raw = phistudio_core::ChartDocument::empty("Controls", 120.)
            .unwrap()
            .as_json()
            .clone();
        raw["judgeLineList"][0]["notes"] = serde_json::json!([phistudio_core::Note::default()]);
        let doc = phistudio_core::ChartDocument::from_value(raw).unwrap();
        let spec = Spec {
            source: NoteId { line: 0, index: 0 },
            destination: None,
            layer: 0,
            range: [Beat::ZERO, "4".parse().unwrap()],
            sample_rate: 120,
            rotation: false,
            replace: false,
            position_tolerance: 0.15,
            rotation_tolerance: 0.015,
        };
        let times: Vec<_> = (0..3)
            .map(|i| TraceTime {
                seconds: f64::from(i),
                sample_seconds: f64::from(i),
                break_before: false,
            })
            .collect();
        let samples = times
            .iter()
            .zip(heights)
            .map(|(t, h)| NoteTraceSample {
                seconds: t.seconds,
                control_height: Some(h),
                position: [0.; 2],
                rotation: 0.,
                screen_position: [0.; 2],
            })
            .collect();
        Sampling {
            snapshot: Snapshot::capture(&doc, spec).unwrap(),
            times,
            samples,
            control_heights: vec![0.5, 1., 1.5],
        }
    }
    #[test]
    fn root_candidates_cover_both_directions_without_duplicate_left_endpoints() {
        for heights in [[0., 1., 2.], [2., 1., 0.]] {
            let refinement = Refinement::new(sample(heights), &AtomicBool::new(false)).unwrap();
            assert_eq!(refinement.roots.len(), 3);
            assert_eq!(
                refinement
                    .roots
                    .iter()
                    .filter(|r| r.threshold == 1.)
                    .count(),
                1
            );
        }
    }
    #[test]
    fn refinement_reschedules_both_sides_once_and_honors_cancellation() {
        let mut refinement =
            Refinement::new(sample([0., 1., 2.]), &AtomicBool::new(false)).unwrap();
        for root in &mut refinement.roots {
            root.time = [root.threshold - 1e-8, root.threshold];
        }
        let Completed::Schedule {
            times,
            control_heights,
            ..
        } = refinement.finish(&AtomicBool::new(false)).unwrap()
        else {
            panic!("Expected schedule")
        };
        assert!(control_heights.is_empty());
        for cut in [0.5, 1., 1.5] {
            assert_eq!(times.iter().filter(|t| t.seconds == cut).count(), 2);
        }
        assert!(Refinement::new(sample([0., 1., 2.]), &AtomicBool::new(true)).is_err());
    }
    #[test]
    fn jumps_already_covered_by_time_boundaries_are_not_refined_as_continuous_roots() {
        let mut sample = sample([0., 1., 2.]);
        sample.times[1].break_before = true;
        let refinement = Refinement::new(sample, &AtomicBool::new(false)).unwrap();
        assert_eq!(refinement.roots.len(), 1);
        assert_eq!(refinement.roots[0].threshold, 1.5);
    }
}
