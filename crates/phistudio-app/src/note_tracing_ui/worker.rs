use super::*;
use phistudio_prpr::NoteTraceBoundaries;
use std::sync::mpsc::{self, Receiver};

pub(super) fn schedule(
    snapshot: Snapshot,
    boundaries: NoteTraceBoundaries,
    cancel: Arc<AtomicBool>,
) -> Receiver<Result<Completed, String>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let result =
            note_tracing::sampling_times(&snapshot.spec, &snapshot.tempo, &boundaries.times)
                .map_err(|e| e.to_string())
                .and_then(|times| {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Tracing canceled".into());
                    }
                    Ok(Completed::Schedule {
                        snapshot,
                        times,
                        control_heights: boundaries.control_heights,
                    })
                });
        let _ = sender.send(result);
    });
    receiver
}
pub(super) fn after_sampling(
    sample: Sampling,
    revision: u64,
    cancel: Arc<AtomicBool>,
) -> Receiver<Result<Completed, String>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let sample = if !sample.control_heights.is_empty() {
                let refinement = roots::Refinement::new(sample, &cancel)?;
                if !refinement.is_empty() {
                    return Ok(Completed::Refine(refinement));
                }
                refinement.sample
            } else {
                sample
            };
            let points: Vec<_> = sample
                .samples
                .iter()
                .zip(&sample.times)
                .map(|(native, time)| TracePoint {
                    seconds: time.seconds,
                    values: [native.position[0], native.position[1], native.rotation],
                    break_before: time.break_before,
                })
                .collect();
            let step = (sample.samples.len() / 512).max(1);
            let path = sample
                .samples
                .iter()
                .step_by(step)
                .map(|p| p.screen_position)
                .collect();
            let display = points
                .iter()
                .step_by(step)
                .map(|p| [p.values[0], p.values[1]])
                .collect();
            let edit = sample
                .snapshot
                .prepare(revision, &points, phistudio_prpr::slice_event, || {
                    cancel.load(Ordering::Relaxed)
                })
                .map_err(|e| e.to_string());
            Ok(Completed::Prepared(Prepared {
                edit,
                path,
                points: display,
                samples: points.len(),
            }))
        }))
        .map_err(|_| "The native note trace could not be prepared".to_owned())
        .and_then(|result| result);
        let _ = sender.send(result);
    });
    receiver
}
pub(super) fn reschedule(
    refinement: roots::Refinement,
    cancel: Arc<AtomicBool>,
) -> Receiver<Result<Completed, String>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(refinement.finish(&cancel));
    });
    receiver
}
