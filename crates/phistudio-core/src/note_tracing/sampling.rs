use super::*;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceTime {
    pub seconds: f64,
    pub sample_seconds: f64,
    pub break_before: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TracePoint {
    pub seconds: f64,
    pub values: [f64; 3],
    pub break_before: bool,
}

/// Preserve both sides of native boundaries at one logical event time.
pub fn sampling_times(
    spec: &Spec,
    tempo: &TempoMap,
    boundaries: &[f64],
) -> Result<Vec<TraceTime>, ChartError> {
    spec.validate()?;
    let begin = tempo.seconds_at(spec.range[0].as_f64());
    let finish = tempo.seconds_at(spec.range[1].as_f64());
    if !begin.is_finite()
        || !finish.is_finite()
        || finish <= begin
        || (finish - begin) * f64::from(spec.sample_rate) > MAX_SAMPLES as f64
    {
        return Err(invalid(
            "Tracing exceeds 60,000 samples; shorten the range or lower the sample rate",
        ));
    }
    let mut cuts = vec![begin, finish];
    cuts.extend(
        boundaries
            .iter()
            .copied()
            .filter(|t| t.is_finite() && *t > begin && *t < finish),
    );
    cuts.extend(
        tempo
            .changes()
            .map(|change| tempo.seconds_at(change.start.as_f64()))
            .filter(|t| *t > begin && *t < finish),
    );
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    let mut times = vec![TraceTime {
        seconds: begin,
        sample_seconds: begin,
        break_before: false,
    }];
    for pair in cuts.windows(2) {
        let [a, b] = [pair[0], pair[1]];
        let steps = ((b - a) * f64::from(spec.sample_rate)).ceil().max(1.) as usize;
        if times.len() + steps + 1 > MAX_SAMPLES {
            return Err(invalid(
                "Tracing exceeds 60,000 samples including native boundaries; choose a shorter range",
            ));
        }
        for i in 1..steps {
            let time = a + (b - a) * i as f64 / steps as f64;
            times.push(TraceTime {
                seconds: time,
                sample_seconds: time,
                break_before: false,
            });
        }
        times.push(TraceTime {
            seconds: b,
            sample_seconds: b - (1e-6_f64).min((b - a) * 1e-5),
            break_before: false,
        });
        times.push(TraceTime {
            seconds: b,
            sample_seconds: b,
            break_before: true,
        });
    }
    Ok(times)
}

pub(super) fn simplify(
    points: &[TracePoint],
    spec: &Spec,
    canceled: &impl Fn() -> bool,
) -> Result<Vec<TracePoint>, ChartError> {
    if points.len() < 2
        || points.len() > MAX_SAMPLES
        || points.iter().any(|p| {
            !p.seconds.is_finite() || p.values.iter().any(|v| !v.is_finite() || v.abs() > 1e9)
        })
    {
        return Err(invalid("Invalid native tracing samples"));
    }
    let mut keep = vec![false; points.len()];
    let mut segments = Vec::new();
    let mut begin = 0;
    for (i, point) in points.iter().enumerate().skip(1) {
        if point.seconds < points[i - 1].seconds
            || (!point.break_before && point.seconds == points[i - 1].seconds)
        {
            return Err(invalid("Tracing sample times must be ordered"));
        }
        if point.break_before {
            segments.push((begin, i - 1));
            begin = i;
        }
    }
    segments.push((begin, points.len() - 1));
    let mut stack = segments;
    while let Some((a, b)) = stack.pop() {
        if canceled() {
            return Err(invalid("Tracing canceled"));
        }
        keep[a] = true;
        keep[b] = true;
        if b <= a + 1 {
            continue;
        }
        let duration = points[b].seconds - points[a].seconds;
        if duration <= 0. {
            return Err(invalid("Tracing intervals must have positive duration"));
        }
        let mut worst = (a, 1_f64);
        for i in a + 1..b {
            let u = (points[i].seconds - points[a].seconds) / duration;
            let error = |axis: usize| {
                (points[i].values[axis]
                    - (points[a].values[axis]
                        + (points[b].values[axis] - points[a].values[axis]) * u))
                    .abs()
            };
            let amount = error(0).hypot(error(1)) / spec.position_tolerance;
            let amount = if spec.rotation {
                amount.max(error(2) / spec.rotation_tolerance)
            } else {
                amount
            };
            if amount > worst.1 {
                worst = (i, amount);
            }
        }
        if worst.0 != a {
            // Balanced splits bound work on long oscillating traces.
            let quarter = (b - a) / 4;
            let mid = if worst.0 < a + quarter || worst.0 > b - quarter {
                (a + b) / 2
            } else {
                worst.0
            };
            stack.push((a, mid));
            stack.push((mid, b));
        }
    }
    Ok(points
        .iter()
        .copied()
        .zip(keep)
        .filter_map(|(point, keep)| keep.then_some(point))
        .collect())
}
