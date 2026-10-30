//! Offline timing/quality probe. Input audio/annotations stay outside the repo.
use anyhow::{Context, Result, ensure};
use phistudio_audio::{Control, Selection, StereoSource};
use phistudio_prpr::ResourceAudioClip;
struct Source(ResourceAudioClip);
impl StereoSource for Source {
    fn sample_rate(&self) -> u32 {
        self.0.sample_rate()
    }
    fn frame_count(&self) -> usize {
        self.0.frame_count()
    }
    fn frame(&self, i: usize) -> [f32; 2] {
        let f = self.0.frames()[i];
        [f.0, f.1]
    }
}
fn metrics(
    actual: &[phistudio_audio::Onset],
    expected: &[f64],
    tolerance: f64,
) -> serde_json::Value {
    let mut used = vec![false; actual.len()];
    let mut errors = vec![];
    for time in expected {
        if let Some((index, onset)) = actual
            .iter()
            .enumerate()
            .filter(|(i, o)| !used[*i] && (o.seconds - time).abs() <= tolerance)
            .min_by(|a, b| {
                (a.1.seconds - time)
                    .abs()
                    .total_cmp(&(b.1.seconds - time).abs())
            })
        {
            used[index] = true;
            errors.push((onset.seconds - time).abs() * 1000.);
        }
    }
    let tp = errors.len();
    let precision = tp as f64 / actual.len().max(1) as f64;
    let recall = tp as f64 / expected.len().max(1) as f64;
    serde_json::json!({"tolerance_ms":tolerance*1000.,"tp":tp,"fp":actual.len()-tp,"fn":expected.len()-tp,
        "precision":precision,"recall":recall,"f1":2.*precision*recall/(precision+recall).max(1e-12),
        "mean_absolute_error_ms":errors.iter().sum::<f64>()/tp.max(1) as f64})
}
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .context("Usage: audio-analysis-probe AUDIO OUTPUT.json [ANNOTATIONS.onsets]")?;
    let output = args.next().context("Missing report path")?;
    let annotations = args
        .next()
        .map(std::fs::read_to_string)
        .transpose()?
        .map(|text| {
            text.lines()
                .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
                .map(str::parse::<f64>)
                .collect::<std::result::Result<Vec<_>, _>>()
        })
        .transpose()?;
    ensure!(
        std::fs::metadata(&input)?.len() <= 128 * 1024 * 1024,
        "Audio is too large"
    );
    let source = Source(ResourceAudioClip::new(std::fs::read(&input)?)?);
    let started = std::time::Instant::now();
    let analysis = phistudio_audio::analyze(&source, &Control::default(), |_| {})?;
    let elapsed = started.elapsed().as_secs_f64();
    let mut runs = vec![];
    for sensitivity in [0.2, 0.5, 0.8] {
        let selected = analysis.select(
            Selection {
                sensitivity,
                ..Default::default()
            },
            &Control::default(),
        )?;
        runs.push(serde_json::json!({"sensitivity":sensitivity,"minimum_gap_ms":30,"onsets":selected.iter().map(|o|
            serde_json::json!({"seconds":o.seconds,"strength":o.strength,"band":o.band})).collect::<Vec<_>>(),
            "metrics_25ms":annotations.as_ref().map(|a|metrics(&selected,a,0.025)),
            "metrics_50ms":annotations.as_ref().map(|a|metrics(&selected,a,0.05))}));
    }
    let report = serde_json::json!({"algorithm_version":phistudio_audio::ALGORITHM_VERSION,"sample_rate":source.sample_rate(),
        "duration_seconds":analysis.waveform.duration(),"analysis_seconds":elapsed,"waveform_bytes":analysis.waveform.memory_bytes(),
        "candidates":analysis.candidates.len(),"runs":runs});
    std::fs::write(output, serde_json::to_vec_pretty(&report)?)?;
    println!(
        "Audio guide analysis: {:.3}s for {:.3}s of music",
        elapsed,
        analysis.waveform.duration()
    );
    Ok(())
}
