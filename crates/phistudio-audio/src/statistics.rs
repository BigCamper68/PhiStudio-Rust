//! Centered rolling medians in O(N log window), without a spectrogram-sized sort.
use std::collections::BTreeSet;

fn rebalance(low: &mut BTreeSet<(u32, usize)>, high: &mut BTreeSet<(u32, usize)>) {
    while low.len() > high.len() + 1 {
        let v = low.pop_last().unwrap();
        high.insert(v);
    }
    while high.len() > low.len() {
        let v = high.pop_first().unwrap();
        low.insert(v);
    }
}
pub(crate) fn median(values: &[f32], radius: usize) -> Vec<f32> {
    let mut low = BTreeSet::new();
    let mut high = BTreeSet::new();
    let mut end = 0;
    let mut first = 0;
    let mut result = Vec::with_capacity(values.len());
    for i in 0..values.len() {
        let target = (i + radius + 1).min(values.len());
        while end < target {
            let v = (values[end].max(0.).to_bits(), end);
            if low.last().is_none_or(|last| v <= *last) {
                low.insert(v);
            } else {
                high.insert(v);
            }
            rebalance(&mut low, &mut high);
            end += 1;
        }
        while first < i.saturating_sub(radius) {
            let v = (values[first].max(0.).to_bits(), first);
            if !low.remove(&v) {
                high.remove(&v);
            }
            rebalance(&mut low, &mut high);
            first += 1;
        }
        result.push(f32::from_bits(low.last().unwrap().0));
    }
    result
}
pub(crate) fn normalize(values: &[f32]) -> Vec<f32> {
    if values.is_empty() {
        return vec![];
    }
    let median = median(values, 30);
    let residual: Vec<_> = values
        .iter()
        .zip(&median)
        .map(|(v, m)| (v - m).abs())
        .collect();
    let deviation = self::median(&residual, 30);
    let floor = noise_floor(values);
    values
        .iter()
        .zip(median)
        .zip(deviation)
        .map(|((&v, m), d)| {
            let z = (v - m).max(0.) / (1.4826 * d + 0.15 * m + floor);
            1. - (-z / 4.).exp()
        })
        .collect()
}

// Only one percentile is needed; partitioning preserves its exact total order
// statistic without sorting the whole analysis track.
fn noise_floor(values: &[f32]) -> f32 {
    let mut ordered = values.to_vec();
    let rank = ordered.len() * 95 / 100;
    let (_, percentile, _) = ordered.select_nth_unstable_by(rank, f32::total_cmp);
    // Suppress floating-point leakage and quiet ripples of sustained tones.
    (*percentile * 0.005).max(0.01)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reference(values: &[f32]) -> f32 {
        let mut sorted = values.to_vec();
        sorted.sort_unstable_by(f32::total_cmp);
        (sorted[sorted.len() * 95 / 100] * 0.005).max(0.01)
    }
    #[test]
    fn percentile_floor_matches_full_sort_including_duplicates_and_float_edges() {
        for length in [1, 2, 19, 20, 21, 31, 99, 100, 101, 1024, 10001] {
            let values = (0..length)
                .map(|n| ((n * 7919) % 1031) as f32 / 13.)
                .collect::<Vec<_>>();
            for mut values in [values, vec![0.; length], vec![2.; length]] {
                assert_eq!(noise_floor(&values).to_bits(), reference(&values).to_bits());
                for (i, value) in [f32::NEG_INFINITY, -0., 0., f32::INFINITY, f32::NAN]
                    .into_iter()
                    .enumerate()
                {
                    let index = i % length;
                    values[index] = value;
                    assert_eq!(noise_floor(&values).to_bits(), reference(&values).to_bits());
                }
            }
        }
        assert!(normalize(&[]).is_empty());
    }
    #[test]
    #[ignore = "manual percentile timing"]
    fn percentile_performance() {
        use std::{hint::black_box, time::Instant};
        let values = (0..150_000)
            .map(|n| ((n * 7919) % 150_001) as f32)
            .collect::<Vec<_>>();
        let start = Instant::now();
        for _ in 0..20 {
            black_box(reference(&values));
        }
        let sorted = start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        for _ in 0..20 {
            black_box(noise_floor(&values));
        }
        let selected = start.elapsed().as_secs_f64() * 1000.;
        eprintln!(
            "PERF percentile frames=150000 reads=20 sort_ms={sorted:.3} select_ms={selected:.3}"
        );
    }
}
