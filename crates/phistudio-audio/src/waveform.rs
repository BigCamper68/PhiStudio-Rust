use crate::{Control, Error, StereoSource};

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(crate) struct Bin {
    pub min: f32,
    pub max: f32,
    pub energy: f32,
    pub count: u32,
}
impl Bin {
    fn add(self, other: Self) -> Self {
        if self.count == 0 {
            return other;
        }
        if other.count == 0 {
            return self;
        }
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
            energy: self.energy + other.energy,
            count: self.count + other.count,
        }
    }
}
#[derive(Clone, Copy, Default, Debug)]
pub struct WaveRange {
    pub min: f32,
    pub max: f32,
    pub rms: f32,
}
pub struct Waveform {
    pub(crate) sample_rate: u32,
    pub(crate) frames: usize,
    pub(crate) block: usize,
    pub(crate) levels: Vec<Vec<Bin>>,
}
impl Waveform {
    pub fn duration(&self) -> f64 {
        self.frames as f64 / f64::from(self.sample_rate)
    }
    pub fn resolution(&self) -> f64 {
        self.block as f64 / f64::from(self.sample_rate)
    }
    pub fn memory_bytes(&self) -> usize {
        self.levels
            .iter()
            .map(|v| v.len() * std::mem::size_of::<Bin>())
            .sum()
    }
    pub(crate) fn from_bins(sample_rate: u32, frames: usize, block: usize, bins: Vec<Bin>) -> Self {
        let mut levels = vec![bins];
        while levels.last().unwrap().len() > 1 {
            let next = levels
                .last()
                .unwrap()
                .chunks(2)
                .map(|v| v[0].add(v.get(1).copied().unwrap_or_default()))
                .collect();
            levels.push(next);
        }
        Self {
            sample_rate,
            frames,
            block,
            levels,
        }
    }
    /// Range maxima preserve short transients while zooming out. Cost O(log N),
    /// independent of the number of audio samples covered by a pixel.
    pub fn range(&self, start: f64, end: f64) -> WaveRange {
        if !start.is_finite()
            || !end.is_finite()
            || end <= start
            || end <= 0.
            || start >= self.duration()
        {
            return WaveRange::default();
        }
        let scale = f64::from(self.sample_rate) / self.block as f64;
        let mut first = (start.max(0.) * scale).floor() as usize;
        let last = ((end.min(self.duration()) * scale).ceil() as usize).min(self.levels[0].len());
        let mut result = Bin::default();
        while first < last {
            let fit = (usize::BITS - 1 - (last - first).leading_zeros()) as usize;
            let aligned = if first == 0 {
                fit
            } else {
                first.trailing_zeros() as usize
            };
            let level = fit.min(aligned).min(self.levels.len() - 1);
            result = result.add(self.levels[level][first >> level]);
            first += 1 << level;
        }
        WaveRange {
            min: result.min,
            max: result.max,
            rms: if result.count == 0 {
                0.
            } else {
                (result.energy / result.count as f32).max(0.).sqrt()
            },
        }
    }
}
pub(crate) fn build(
    source: &impl StereoSource,
    control: &Control,
) -> Result<(Waveform, Vec<f32>, [f32; 2]), Error> {
    let sr = source.sample_rate() as usize;
    let frames = source.frame_count();
    let block = sr.div_ceil(500).next_power_of_two();
    let mut bins = Vec::with_capacity(frames.div_ceil(block));
    let energy_frames = frames.saturating_mul(1000).div_ceil(sr);
    let mut energy = vec![0.; energy_frames];
    let mut counts = vec![0_u32; energy_frames];
    let mut bin = Bin::default();
    let mut dc = [0_f64; 2];
    for index in 0..frames {
        if index % 8192 == 0 {
            control.progress(100 + (index as u64 * 100 / frames as u64) as u32)?;
        }
        let [l, r] = source.frame(index);
        if !l.is_finite() || !r.is_finite() || l.abs() > 16. || r.abs() > 16. {
            return Err(Error::InvalidAudio);
        }
        dc[0] += f64::from(l);
        dc[1] += f64::from(r);
        let e = (l * l + r * r) * 0.5;
        bin = bin.add(Bin {
            min: l.min(r),
            max: l.max(r),
            energy: e,
            count: 1,
        });
        let at = index * 1000 / sr;
        energy[at] += e;
        counts[at] += 1;
        if (index + 1) % block == 0 {
            bins.push(bin);
            bin = Bin::default();
        }
    }
    if bin.count != 0 {
        bins.push(bin);
    }
    for (e, count) in energy.iter_mut().zip(counts) {
        *e = (*e / count.max(1) as f32).sqrt();
    }
    Ok((
        Waveform::from_bins(sr as u32, frames, block, bins),
        energy,
        [
            (dc[0] / frames as f64) as f32,
            (dc[1] / frames as f64) as f32,
        ],
    ))
}
