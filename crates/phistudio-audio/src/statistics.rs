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
    let mut sorted = values.to_vec();
    sorted.sort_unstable_by(f32::total_cmp);
    // Do not turn floating-point leakage or imperceptible spectral ripples of
    // a sustained tone into full-strength peaks when local variance is tiny.
    let floor = (sorted[sorted.len() * 95 / 100] * 0.005).max(0.01);
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
