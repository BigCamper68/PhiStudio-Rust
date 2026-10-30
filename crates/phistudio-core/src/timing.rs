use crate::{Beat, ChartError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct BpmChange {
    #[serde(rename = "startTime")]
    pub start: Beat,
    pub bpm: f64,
}

/// Chart clock (not audio clock). Like prpr, the first BPM key is time zero;
/// values before it extrapolate with that key's tempo. Offset is applied once
/// at the transport boundary, never inside tempo conversion.
#[derive(Clone, Debug)]
pub struct TempoMap {
    keys: Vec<(BpmChange, f64)>,
}

impl TempoMap {
    pub fn new(mut changes: Vec<BpmChange>) -> Result<Self, ChartError> {
        if changes.is_empty() || changes.iter().any(|x| !x.bpm.is_finite() || x.bpm <= 0.) {
            return Err(ChartError::Invalid(
                "BPMList must contain positive, finite tempos".into(),
            ));
        }
        changes.sort_by_key(|x| x.start);
        let mut keys: Vec<(BpmChange, f64)> = Vec::with_capacity(changes.len());
        for change in changes {
            if keys
                .last()
                .is_some_and(|(last, _)| last.start == change.start)
            {
                keys.last_mut().unwrap().0 = change;
                continue;
            }
            let time = keys.last().map_or(0., |(last, t)| {
                t + (change.start.as_f64() - last.start.as_f64()) * 60. / last.bpm
            });
            keys.push((change, time));
        }
        Ok(Self { keys })
    }

    pub fn seconds_at(&self, beat: f64) -> f64 {
        let i = self
            .keys
            .partition_point(|(x, _)| x.start.as_f64() <= beat)
            .saturating_sub(1);
        let (key, time) = self.keys[i];
        time + (beat - key.start.as_f64()) * 60. / key.bpm
    }

    pub fn beat_at(&self, seconds: f64) -> f64 {
        let i = self
            .keys
            .partition_point(|(_, t)| *t <= seconds)
            .saturating_sub(1);
        let (key, time) = self.keys[i];
        key.start.as_f64() + (seconds - time) * key.bpm / 60.
    }

    pub fn bpm_at(&self, beat: f64) -> f64 {
        let i = self
            .keys
            .partition_point(|(x, _)| x.start.as_f64() <= beat)
            .saturating_sub(1);
        self.keys[i].0.bpm
    }
}
