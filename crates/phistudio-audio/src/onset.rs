use crate::{Candidate, Control, Error, FRAME_RATE, StereoSource, spectrum, statistics};

fn refine(seconds: f64, energy: &[f32]) -> f64 {
    let center = (seconds * 1000.).round() as usize;
    let start = center.saturating_sub(40);
    let end = (center + 50).min(energy.len().saturating_sub(1));
    if start >= end {
        return seconds;
    }
    let center = center.min(energy.len() - 1);
    let mut best = center.min(end);
    let mut best_rise = 0.;
    for i in start..=end {
        let before_start = i.saturating_sub(8);
        let before_end = i.saturating_sub(3).max(before_start + 1).min(energy.len());
        let before = energy[before_start..before_end].iter().sum::<f32>()
            / (before_end - before_start) as f32;
        let after_end = (i + 3).min(energy.len());
        let after = energy[i..after_end].iter().sum::<f32>() / (after_end - i).max(1) as f32;
        let distance = (i as f32 - center as f32) / 25.;
        let rise = (after - before).max(0.) / (1. + distance * distance);
        if rise > best_rise {
            best_rise = rise;
            best = i;
        }
    }
    if best_rise < 0.00001 {
        return seconds;
    }
    let before = &energy[best.saturating_sub(25)..best.max(1)];
    let after = &energy[best..(best + 25).min(energy.len())];
    let previous = before.iter().sum::<f32>() / before.len() as f32;
    let following = after.iter().sum::<f32>() / after.len() as f32;
    if following < previous * 1.15 {
        return seconds;
    }
    // Find the leading edge of this rise, rather than the maximum loudness.
    let floor = energy[best.saturating_sub(12)..best.saturating_sub(4).max(1)]
        .iter()
        .copied()
        .fold(f32::INFINITY, f32::min);
    let top = energy[best..(best + 5).min(energy.len())]
        .iter()
        .copied()
        .fold(0_f32, f32::max);
    let threshold = floor + (top - floor).max(0.) * 0.12;
    let mut at = best;
    while at > best.saturating_sub(12) && energy[at - 1] > threshold {
        at -= 1;
    }
    at as f64 / 1000.
}
pub(crate) fn detect(
    source: &impl StereoSource,
    energy: &[f32],
    dc: [f32; 2],
    control: &Control,
) -> Result<Vec<Candidate>, Error> {
    let features = spectrum::extract(source, dc, control)?;
    let count = features.len();
    let mut salience = vec![[0_f32; 4]; count];
    for band in 0..3 {
        let mut normalized = Vec::new();
        for scale in 0..3 {
            control.progress(800 + (band * 30 + scale * 8) as u32)?;
            normalized.push(statistics::normalize(
                &features
                    .iter()
                    .map(|f| f.flux[scale][band])
                    .collect::<Vec<_>>(),
            ));
        }
        let phase =
            statistics::normalize(&features.iter().map(|f| f.phase[band]).collect::<Vec<_>>());
        for i in 0..count {
            let scale = [normalized[0][i], normalized[1][i], normalized[2][i]];
            let max = scale.iter().copied().fold(0_f32, f32::max);
            let mean = scale.iter().sum::<f32>() / 3.;
            let mut ordered = scale;
            ordered.sort_unstable_by(f32::total_cmp);
            let median = ordered[1];
            // Complex-domain evidence supports, but cannot create a detection
            // on its own (vibrato and a note's release are not fresh attacks).
            let before_start = i.saturating_sub(5);
            let before_end = i.saturating_sub(2).max(before_start + 1).min(count);
            let before = features[before_start..before_end]
                .iter()
                .map(|f| f.power[band])
                .sum::<f32>()
                / (before_end - before_start) as f32;
            let after_end = (i + 3).min(count);
            let after = features[i..after_end]
                .iter()
                .map(|f| f.power[band])
                .sum::<f32>()
                / (after_end - i) as f32;
            salience[i][band + 1] = if after > before * 1.5 {
                0.55 * max + 0.30 * mean + 0.15 * phase[i] * max
            } else {
                0.25 * max + 0.4 * mean + 0.2 * median + 0.15 * phase[i] * median
            };
        }
    }
    let mut candidates = vec![];
    for (i, frame) in salience.iter_mut().enumerate() {
        if i % 128 == 0 {
            control.progress(900 + (i as u64 * 90 / count as u64) as u32)?;
        }
        let peak = frame[1..].iter().copied().fold(0_f32, f32::max);
        let mean = frame[1..].iter().sum::<f32>() / 3.;
        frame[0] = 0.85 * peak + 0.15 * mean;
    }
    for i in 0..count {
        let seconds = i as f64 / FRAME_RATE as f64;
        let at = (seconds * 1000.) as usize;
        let low = at.saturating_sub(30).min(energy.len());
        let high = (at + 31).min(energy.len());
        if energy[low..high].iter().copied().fold(0_f32, f32::max) < 0.00001 {
            continue;
        }
        let mut strength = [0.; 4];
        for band in 0..4 {
            let value = salience[i][band];
            if value < 0.15 {
                continue;
            }
            // Strict left / inclusive right chooses one point on a plateau.
            if (i.saturating_sub(2)..i).all(|j| salience[j][band] < value)
                && ((i + 1)..(i + 3).min(count)).all(|j| salience[j][band] <= value)
            {
                strength[band] = value;
            }
        }
        if strength.iter().any(|s| *s > 0.) {
            let seconds = refine(seconds, energy);
            // A hard note ending can introduce fresh broadband spectrum, but
            // is not a new attack. Reject clear transitions into near silence;
            // other attacks remain possible while the overall mix gets quieter.
            let at = (seconds * 1000.).round() as usize;
            let before = &energy
                [at.saturating_sub(12).min(energy.len())..at.saturating_sub(2).min(energy.len())];
            let after = &energy[(at + 2).min(energy.len())..(at + 12).min(energy.len())];
            let previous = before.iter().sum::<f32>() / before.len().max(1) as f32;
            let following = after.iter().sum::<f32>() / after.len().max(1) as f32;
            if previous > 0.00001 && following < previous * 0.12 {
                continue;
            }
            if seconds < source.frame_count() as f64 / f64::from(source.sample_rate()) {
                let previous = features[i.saturating_sub(3)].power;
                let power = features[(i + 1).min(count - 1)].power;
                let rise = std::array::from_fn::<_, 3, _>(|b| (power[b] - previous[b]).max(0.));
                let weight = if rise.iter().any(|p| *p > 1e-10) {
                    rise
                } else {
                    power
                };
                let band = (0..3)
                    .max_by(|a, b| weight[*a].total_cmp(&weight[*b]))
                    .unwrap()
                    + 1;
                candidates.push(Candidate {
                    seconds,
                    strength,
                    band,
                });
            }
        }
    }
    candidates.sort_unstable_by(|a, b| a.seconds.total_cmp(&b.seconds));
    let mut merged: Vec<Candidate> = vec![];
    for c in candidates {
        if let Some(last) = merged.last_mut()
            && (c.seconds - last.seconds).abs() < 0.004
        {
            if c.strength[0] > last.strength[0] {
                last.band = c.band;
            }
            for (a, b) in last.strength.iter_mut().zip(c.strength) {
                *a = a.max(b);
            }
        } else {
            merged.push(c);
        }
    }
    Ok(merged)
}
