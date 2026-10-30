//! Port of PhiStudio EventCloneOperation / BatchValueTransform at bb2e9b3.
//! Named ordered destinations, copy/move, incremental timing and offset profiles.
use super::*;
#[derive(Clone, Debug, PartialEq)]
pub struct ValueProfile {
    pub from: f64,
    pub to: f64,
    pub easing: i32,
    pub sequence: Vec<f64>,
    pub disturbance: f64,
    pub seed: u64,
}
impl Default for ValueProfile {
    fn default() -> Self {
        Self {
            from: 0.,
            to: 0.,
            easing: 1,
            sequence: vec![1.],
            disturbance: 0.,
            seed: 1,
        }
    }
}
impl ValueProfile {
    /// PhiStudio's BatchValueTransform calls Easing.apply directly. Do not use
    /// native playback tween normalization here: Expo/Elastic endpoints differ.
    pub fn java_values(&self, count: usize) -> Result<Vec<f64>, ChartError> {
        self.values(count, java_easing)
    }
    pub fn values(
        &self,
        count: usize,
        mut tween: impl FnMut(i32, f64) -> f64,
    ) -> Result<Vec<f64>, ChartError> {
        if count > MAX_OBJECTS
            || self.sequence.is_empty()
            || self.sequence.len() > 256
            || !(1..=29).contains(&self.easing)
            || [self.from, self.to, self.disturbance]
                .iter()
                .chain(self.sequence.iter())
                .any(|v| !v.is_finite())
        {
            return Err(invalid("Invalid value profile"));
        }
        // java.util.Random's exact 48-bit stream preserves deterministic Java previews.
        let mut seed = (self.seed ^ 0x5DEECE66D) & ((1 << 48) - 1);
        let mut next = |bits: u32| {
            seed = (seed.wrapping_mul(0x5DEECE66D).wrapping_add(11)) & ((1 << 48) - 1);
            seed >> (48 - bits)
        };
        (0..count)
            .map(|i| {
                let t = if count <= 1 {
                    0.
                } else {
                    i as f64 / (count - 1) as f64
                };
                let base = self.from + (self.to - self.from) * tween(self.easing, t);
                let noise = if self.disturbance == 0. {
                    0.
                } else {
                    let r = ((next(26) << 27) + next(27)) as f64 / (1_u64 << 53) as f64;
                    (r * 2. - 1.) * self.disturbance.abs()
                };
                let value = (base + noise) * self.sequence[i % self.sequence.len()];
                if value.is_finite() {
                    Ok(value)
                } else {
                    Err(invalid("Value profile overflow"))
                }
            })
            .collect()
    }
}

/// Direct translation of PhiStudio model/Easing.java applyUnclamped (bb2e9b3).
/// This belongs to the Java clone operation; playback retains the original prpr.
fn java_easing(kind: i32, input: f64) -> f64 {
    let t = if input.is_finite() {
        input.clamp(0., 1.)
    } else {
        0.
    };
    let c1 = 1.70158;
    let c3 = c1 + 1.;
    let c2 = c1 * 1.525;
    let pi = std::f64::consts::PI;
    let elastic = |t: f64| {
        if t == 0. || t == 1. {
            t
        } else {
            -2_f64.powf(10. * t - 10.) * ((t * 10. - 10.75) * (2. * pi / 3.)).sin()
        }
    };
    let bounce = |t: f64| {
        let n = 7.5625;
        let d = 2.75;
        if t < 1. / d {
            n * t * t
        } else if t < 2. / d {
            n * (t - 1.5 / d).powi(2) + 0.75
        } else if t < 2.5 / d {
            n * (t - 2.25 / d).powi(2) + 0.9375
        } else {
            n * (t - 2.625 / d).powi(2) + 0.984375
        }
    };
    match kind {
        2 => (t * pi / 2.).sin(),
        3 => 1. - (t * pi / 2.).cos(),
        4 => 1. - (1. - t) * (1. - t),
        5 => t * t,
        6 => (1. - (pi * t).cos()) / 2.,
        7 => {
            if t < 0.5 {
                2. * t * t
            } else {
                1. - (-2. * t + 2.).powi(2) / 2.
            }
        }
        8 => 1. - (1. - t).powi(3),
        9 => t * t * t,
        10 => 1. - (1. - t).powi(4),
        11 => t.powi(4),
        12 => {
            if t < 0.5 {
                4. * t * t * t
            } else {
                1. - (-2. * t + 2.).powi(3) / 2.
            }
        }
        13 => {
            if t < 0.5 {
                8. * t.powi(4)
            } else {
                1. - (-2. * t + 2.).powi(4) / 2.
            }
        }
        14 => 1. - (1. - t).powi(5),
        15 => t.powi(5),
        16 => {
            if t == 1. {
                1.
            } else {
                1. - 2_f64.powf(-10. * t)
            }
        }
        17 => {
            if t == 0. {
                0.
            } else {
                2_f64.powf(10. * t - 10.)
            }
        }
        18 => (1. - (t - 1.).powi(2)).sqrt(),
        19 => 1. - (1. - t * t).sqrt(),
        20 => 1. + c3 * (t - 1.).powi(3) + c1 * (t - 1.).powi(2),
        21 => c3 * t * t * t - c1 * t * t,
        22 => {
            if t < 0.5 {
                (1. - (1. - (2. * t).powi(2)).sqrt()) / 2.
            } else {
                ((1. - (-2. * t + 2.).powi(2)).sqrt() + 1.) / 2.
            }
        }
        23 => {
            if t < 0.5 {
                (2. * t).powi(2) * ((c2 + 1.) * 2. * t - c2) / 2.
            } else {
                ((2. * t - 2.).powi(2) * ((c2 + 1.) * (t * 2. - 2.) + c2) + 2.) / 2.
            }
        }
        24 => {
            if t == 0. || t == 1. {
                t
            } else {
                2_f64.powf(-10. * t) * ((t * 10. - 0.75) * (2. * pi / 3.)).sin() + 1.
            }
        }
        25 => elastic(t),
        26 => bounce(t),
        27 => 1. - bounce(1. - t),
        28 => {
            if t < 0.5 {
                (1. - bounce(1. - 2. * t)) / 2.
            } else {
                (1. + bounce(2. * t - 1.)) / 2.
            }
        }
        29 => {
            if t == 0. || t == 1. {
                t
            } else if t * 2. < 1. {
                elastic(t * 2.) / 2.
            } else {
                1. - elastic(2. - t * 2.) / 2.
            }
        }
        _ => t,
    }
}
#[derive(Clone, Debug)]
pub struct CloneSpec {
    pub targets: Vec<usize>,
    pub delay: Beat,
    pub keep_source: bool,
    pub offsets: [Vec<f64>; 4],
}
pub fn clone_events(
    doc: &ChartDocument,
    selected: &Selection,
    spec: &CloneSpec,
) -> Result<ToolEdit, ChartError> {
    let selected = selected.checked(doc)?;
    let first = selected
        .events
        .first()
        .ok_or_else(|| invalid("Select events to clone"))?;
    if !selected.notes.is_empty()
        || selected
            .events
            .iter()
            .any(|id| id.line != first.line || id.layer != first.layer)
    {
        return Err(invalid("Clone events from one source line and layer"));
    }
    if spec.targets.is_empty()
        || spec.targets.len() > 256
        || spec.targets.iter().copied().collect::<BTreeSet<_>>().len() != spec.targets.len()
        || spec.targets.iter().any(|i| *i >= doc.line_count())
    {
        return Err(invalid("Choose 1–256 distinct destination lines"));
    }
    if selected.events.len() * spec.targets.len() > 4096 {
        return Err(invalid("Clone at most 4,096 events at once"));
    }
    if spec
        .offsets
        .iter()
        .any(|v| v.len() != spec.targets.len() || v.iter().any(|v| !v.is_finite()))
    {
        return Err(invalid("Invalid clone offsets"));
    }
    let source = objects(doc, &selected)?;
    let mut items = Vec::new();
    let mut shift = Beat::ZERO;
    for (i, line) in spec.targets.iter().enumerate() {
        for object in &source {
            let mut clone = object.clone();
            clone.line = *line;
            clone.with_times(
                object.start()?.checked_add(shift)?,
                object.end()?.checked_add(shift)?,
            )?;
            let (_, kind) = clone.event.unwrap();
            let axis = match kind {
                EventType::MoveX => Some(0),
                EventType::MoveY => Some(1),
                EventType::Rotate => Some(2),
                EventType::Alpha => Some(3),
                EventType::Speed => None,
            };
            if let Some(axis) = axis {
                let mut event = LineEvent::read(kind, &clone.value)?;
                event.start += spec.offsets[axis][i];
                event.end += spec.offsets[axis][i];
                clone.value = event.merge(kind, Some(&clone.value))?;
            }
            items.push(clone);
        }
        shift = shift.checked_add(spec.delay)?;
    }
    let empty = Selection::default();
    plan(
        doc,
        if spec.keep_source { &empty } else { &selected },
        items,
        if spec.keep_source {
            "Clone events"
        } else {
            "Move cloned events"
        },
    )
}
