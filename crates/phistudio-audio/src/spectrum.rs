//! Independent implementation of log-filtered, frequency-max spectral flux and
//! complex-domain prediction error. Stereo is packed into a single complex FFT;
//! channels are recovered before magnitudes, so opposite phases never cancel.
use crate::{Control, Error, StereoSource};
use rustfft::{Fft, FftPlanner, num_complex::Complex32 as C};
use std::sync::Arc;

#[derive(Clone, Copy, Default)]
pub(crate) struct Features {
    pub flux: [[f32; 3]; 3],
    pub phase: [f32; 3],
    pub power: [f32; 3],
}
struct Filter {
    bins: Vec<(usize, f32)>,
    group: usize,
    inverse_weight: f32,
}
struct Scale {
    n: usize,
    fft: Arc<dyn Fft<f32>>,
    buffer: Vec<C>,
    scratch: Vec<C>,
    window: Vec<f32>,
    filters: Vec<Filter>,
    previous: Vec<f32>,
    logs: Vec<f32>,
    global_peak: f32,
    band_peak: [f32; 3],
    prev_channels: Vec<[C; 2]>,
    older_channels: Vec<[C; 2]>,
    magnitudes: Vec<f32>,
    errors: Vec<f32>,
}
impl Scale {
    fn new(n: usize, sr: u32, planner: &mut FftPlanner<f32>) -> Self {
        let fft = planner.plan_fft_forward(n);
        let mut filters = vec![];
        let max = f64::from(sr).mul_add(0.48, 0.).min(16000.);
        let ratio = 2_f64.powf(1. / 24.);
        let mut center = 35.;
        let mut last = None;
        while center < max {
            let bin =
                |hz: f64| ((hz * n as f64 / f64::from(sr)).round() as usize).clamp(1, n / 2 - 1);
            let c = bin(center);
            let l = bin(center / ratio).min(c.saturating_sub(1));
            let r = bin(center * ratio).max(c + 1).min(n / 2);
            if last != Some((l, c, r)) {
                let bins: Vec<_> = (l..=r)
                    .map(|k| {
                        (
                            k,
                            if k <= c {
                                (k - l) as f32 / (c - l) as f32
                            } else {
                                (r - k) as f32 / (r - c) as f32
                            },
                        )
                    })
                    .filter(|(_, w)| *w > 0.)
                    .collect();
                let inverse_weight = 1. / bins.iter().map(|(_, w)| w).sum::<f32>();
                filters.push(Filter {
                    bins,
                    inverse_weight,
                    group: if center < 250. {
                        0
                    } else if center < 2500. {
                        1
                    } else {
                        2
                    },
                });
                last = Some((l, c, r));
            }
            center *= ratio;
        }
        let count = filters.len();
        Self {
            n,
            scratch: vec![C::default(); fft.get_inplace_scratch_len()],
            fft,
            buffer: vec![C::default(); n],
            window: (0..n)
                .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos())
                .collect(),
            filters,
            previous: vec![0.; count],
            logs: vec![0.; count],
            global_peak: 0.,
            band_peak: [0.; 3],
            prev_channels: vec![[C::default(); 2]; n / 2 + 1],
            older_channels: vec![[C::default(); 2]; n / 2 + 1],
            magnitudes: vec![0.; n / 2 + 1],
            errors: vec![0.; n / 2 + 1],
        }
    }
    fn sample(
        &mut self,
        source: &impl StereoSource,
        center: usize,
        phase: bool,
        dc: [f32; 2],
    ) -> ([f32; 3], [f32; 3], [f32; 3]) {
        let first = center as isize - self.n as isize / 2;
        let mean = C::new(dc[0], dc[1]);
        for (j, v) in self.buffer.iter_mut().enumerate() {
            let i = first + j as isize;
            let length = source.frame_count() as isize;
            let index = if i >= 0 && i < length {
                i
            } else {
                let reflected = i.rem_euclid(2 * length);
                if reflected >= length {
                    2 * length - 1 - reflected
                } else {
                    reflected
                }
            };
            let [l, r] = source.frame(index as usize);
            *v = C::new(l, r);
        }
        for (v, w) in self.buffer.iter_mut().zip(&self.window) {
            *v = (*v - mean) * w;
        }
        self.fft
            .process_with_scratch(&mut self.buffer, &mut self.scratch);
        for k in 1..=self.n / 2 {
            let z = self.buffer[k];
            let opposite = self.buffer[self.n - k].conj();
            let channels = [(z + opposite) * 0.5, (z - opposite) * C::new(0., -0.5)];
            let mut energy = 0.;
            let mut error = 0.;
            for (channel, current) in channels.into_iter().enumerate() {
                let magnitude = current.norm();
                energy += magnitude * magnitude;
                if phase {
                    let p = self.prev_channels[k][channel];
                    let q = self.older_channels[k][channel];
                    let amplitude = p.norm();
                    let denominator = amplitude * q.norm();
                    if magnitude >= amplitude && denominator > 1e-12 {
                        let predicted = p * p * q.conj() / denominator;
                        error += (current - predicted).norm_sqr();
                    }
                    self.older_channels[k][channel] = p;
                    self.prev_channels[k][channel] = current;
                }
            }
            self.magnitudes[k] = (energy * 0.5).sqrt() * 2. / self.n as f32;
            self.errors[k] = (error * 0.5).sqrt() * 2. / self.n as f32;
        }
        let global_peak = self.magnitudes.iter().copied().fold(0_f32, f32::max);
        let mut group_peak = [0_f32; 3];
        let mut power = [0_f32; 3];
        for (k, mag) in self.magnitudes.iter().enumerate().skip(1) {
            let hz = k as f32 * source.sample_rate() as f32 / self.n as f32;
            let group = if hz < 250. {
                0
            } else if hz < 2500. {
                1
            } else {
                2
            };
            group_peak[group] = group_peak[group].max(*mag);
            power[group] += mag * mag;
        }
        let mut flux = [0.; 3];
        let mut complex = [0.; 3];
        let mut counts = [0_f32; 3];
        for (index, filter) in self.filters.iter().enumerate() {
            let magnitude = filter
                .bins
                .iter()
                .map(|(k, w)| self.magnitudes[*k] * w)
                .sum::<f32>()
                * filter.inverse_weight;
            let denominator = self.band_peak[filter.group]
                .max(self.global_peak * 0.02)
                .max(0.0001)
                / 8.;
            self.logs[index] = (8. * magnitude / denominator).ln_1p();
            let low = index.saturating_sub(1);
            let high = (index + 2).min(self.previous.len());
            let reference = self.previous[low..high]
                .iter()
                .copied()
                .fold(0_f32, f32::max);
            flux[filter.group] += (self.logs[index] - reference).max(0.);
            if phase {
                let error = filter
                    .bins
                    .iter()
                    .map(|(k, w)| self.errors[*k] * w)
                    .sum::<f32>()
                    * filter.inverse_weight;
                complex[filter.group] += (8. * error / denominator).ln_1p();
            }
            counts[filter.group] += 1.;
        }
        std::mem::swap(&mut self.previous, &mut self.logs);
        // Use the same causal level reference for a newly rising frame. Updating
        // gain before measuring its flux would normalize away an actual attack
        // emerging from broadband background noise.
        self.global_peak = (self.global_peak * 0.995).max(global_peak);
        for (reference, current) in self.band_peak.iter_mut().zip(group_peak) {
            *reference = (*reference * 0.995).max(current);
        }
        for i in 0..3 {
            flux[i] /= counts[i].max(1.);
            complex[i] /= counts[i].max(1.);
        }
        (flux, complex, power)
    }
}
pub(crate) fn extract(
    source: &impl StereoSource,
    dc: [f32; 2],
    control: &Control,
) -> Result<Vec<Features>, Error> {
    let sr = source.sample_rate();
    let small = (sr as usize / 47).next_power_of_two();
    let mut planner = FftPlanner::<f32>::new();
    let mut scales = [
        Scale::new(small, sr, &mut planner),
        Scale::new(small * 2, sr, &mut planner),
        Scale::new(small * 4, sr, &mut planner),
    ];
    let count =
        (source.frame_count() as u64 * crate::FRAME_RATE as u64).div_ceil(u64::from(sr)) as usize;
    let mut features = vec![Features::default(); count];
    for (i, feature) in features.iter_mut().enumerate() {
        if i % 16 == 0 {
            control.progress(200 + (i as u64 * 600 / count as u64) as u32)?;
        }
        let center = ((i as u64 * u64::from(sr) + 50) / crate::FRAME_RATE as u64) as usize;
        for (j, scale) in scales.iter_mut().enumerate() {
            let (flux, phase, power) = scale.sample(source, center, j == 1, dc);
            feature.flux[j] = flux;
            if j == 1 {
                feature.phase = phase;
                feature.power = power;
            }
        }
    }
    Ok(features)
}
