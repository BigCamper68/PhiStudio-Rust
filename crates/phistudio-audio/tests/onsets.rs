use phistudio_audio::{Analysis, Control, Selection, StereoSource, analyze};
struct Audio {
    sr: u32,
    frames: Vec<[f32; 2]>,
}
impl StereoSource for Audio {
    fn sample_rate(&self) -> u32 {
        self.sr
    }
    fn frame_count(&self) -> usize {
        self.frames.len()
    }
    fn frame(&self, i: usize) -> [f32; 2] {
        self.frames[i]
    }
}
impl Audio {
    fn new(sr: u32, seconds: f64) -> Self {
        Self {
            sr,
            frames: vec![[0.; 2]; (seconds * f64::from(sr)) as usize],
        }
    }
    fn burst(&mut self, at: f64, hz: f64, gain: f32, attack: f64, decay: f64, stereo: [f32; 2]) {
        let first = (at * f64::from(self.sr)).round() as usize;
        for i in first..self.frames.len() {
            let t = (i - first) as f64 / f64::from(self.sr);
            if t > decay * 8. + attack {
                break;
            }
            let envelope = if t < attack {
                t / attack
            } else {
                (-(t - attack) / decay).exp()
            };
            let sound = (std::f64::consts::TAU * hz * t).cos()
                + 0.35 * (std::f64::consts::TAU * hz * 2.03 * t).cos();
            for (v, ch) in self.frames[i].iter_mut().zip(stereo) {
                *v += gain * envelope as f32 * sound as f32 * ch;
            }
        }
    }
}
fn run(audio: &Audio) -> Analysis {
    analyze(audio, &Control::default(), |_| {}).unwrap()
}
fn detected(data: &Analysis) -> Vec<f64> {
    data.select(Selection::default(), &Control::default())
        .unwrap()
        .iter()
        .map(|o| o.seconds)
        .collect()
}
fn score(actual: &[f64], expected: &[f64], tolerance: f64) -> (usize, usize, usize) {
    let mut used = vec![false; actual.len()];
    let mut hits = 0;
    for time in expected {
        if let Some((i, _)) = actual
            .iter()
            .enumerate()
            .filter(|(i, t)| !used[*i] && (**t - time).abs() <= tolerance)
            .min_by(|a, b| (a.1 - time).abs().total_cmp(&(b.1 - time).abs()))
        {
            used[i] = true;
            hits += 1;
        }
    }
    (hits, actual.len() - hits, expected.len() - hits)
}
#[test]
fn percussive_attacks_are_timed_at_the_leading_edge_in_all_supported_sample_rates() {
    let expected = [0.3, 0.7, 1.2, 1.7];
    for sr in [8000, 22050, 44100, 48000, 96000] {
        let mut audio = Audio::new(sr, 2.2);
        for (i, t) in expected.iter().enumerate() {
            audio.burst(
                *t,
                [90., 600., 4000., 180.][i],
                0.35,
                0.001,
                0.045,
                [1., -1.],
            );
        }
        let actual = detected(&run(&audio));
        let result = score(&actual, &expected, 0.02);
        eprintln!("{sr}: {actual:?}, score={result:?}");
        assert_eq!(result.0, expected.len(), "{sr}: {actual:?}");
        assert!(result.1 <= 1, "{sr}: {actual:?}");
    }
}
#[test]
fn soft_and_quiet_attacks_are_found_over_a_loud_sustained_tone() {
    let mut audio = Audio::new(24000, 3.);
    for (i, frame) in audio.frames.iter_mut().enumerate() {
        let t = i as f64 / 24000.;
        let envelope = (t / 0.1).min(1.) * 0.4;
        *frame = [(envelope * (std::f64::consts::TAU * 110. * t).sin()) as f32; 2];
    }
    let expected = [0.5, 1.2, 2.];
    for (i, t) in expected.iter().enumerate() {
        audio.burst(*t, [660., 1100., 3300.][i], 0.025, 0.018, 0.12, [0., 1.]);
    }
    let actual = detected(&run(&audio));
    let result = score(&actual, &expected, 0.035);
    eprintln!("quiet: {actual:?}, score={result:?}");
    assert_eq!(result.0, 3, "{actual:?}");
    assert!(
        actual.iter().filter(|t| **t > 0.2).count() <= 5,
        "{actual:?}"
    );
}
#[test]
fn silence_dc_and_sustained_vibrato_do_not_create_a_stream_of_attacks() {
    let silence = Audio::new(24000, 1.);
    assert!(detected(&run(&silence)).is_empty());
    let mut dc = Audio::new(24000, 1.);
    dc.frames.fill([0.2, -0.2]);
    assert!(detected(&run(&dc)).iter().all(|t| *t < 0.15));
    let mut vibrato = Audio::new(24000, 3.);
    let mut phase = 0.;
    for (i, f) in vibrato.frames.iter_mut().enumerate() {
        let t = i as f64 / 24000.;
        phase +=
            std::f64::consts::TAU * 440. * (1. + 0.025 * (std::f64::consts::TAU * 5. * t).sin())
                / 24000.;
        let amp = (t / 0.1).min(1.) * 0.3;
        *f = [(amp * phase.sin()) as f32; 2];
    }
    let actual = detected(&run(&vibrato));
    eprintln!("vibrato: {actual:?}");
    assert!(
        actual.iter().filter(|t| **t > 0.3 && **t < 2.8).count() <= 2,
        "{actual:?}"
    );
}
#[test]
fn dense_hits_have_no_duplicates_and_selection_is_deterministic_and_cancelable() {
    let mut audio = Audio::new(24000, 2.);
    let expected: Vec<_> = (0..12).map(|i| 0.3 + i as f64 * 0.09).collect();
    for t in &expected {
        audio.burst(*t, 1700., 0.4, 0.001, 0.015, [1., 1.]);
    }
    let data = run(&audio);
    let actual = detected(&data);
    eprintln!("dense: {actual:?}");
    assert!(score(&actual, &expected, 0.02).0 >= 11);
    assert!(actual.windows(2).all(|p| p[1] - p[0] >= 0.059));
    let low = data
        .select(
            Selection {
                sensitivity: 0.2,
                ..Default::default()
            },
            &Control::default(),
        )
        .unwrap();
    let high = data
        .select(
            Selection {
                sensitivity: 0.9,
                ..Default::default()
            },
            &Control::default(),
        )
        .unwrap();
    assert!(
        low.iter()
            .all(|o| high.iter().any(|h| h.seconds == o.seconds))
    );
    let c = Control::default();
    c.cancel();
    assert!(analyze(&audio, &c, |_| {}).is_err());
    assert!(data.select(Selection::default(), &c).is_err());
}

#[test]
fn hard_note_releases_are_not_attacks_and_marker_colors_follow_the_new_spectral_energy() {
    let mut audio = Audio::new(24000, 2.1);
    let expected = [0.3, 0.9, 1.5];
    for (i, time) in expected.iter().enumerate() {
        audio.burst(*time, [650., 100., 3300.][i], 0.4, 0.001, 0.035, [1., -1.]);
    }
    for (i, frame) in audio.frames.iter_mut().enumerate() {
        let time = i as f64 / 24000.;
        if expected.iter().any(|t| time >= t + 0.22 && time < t + 0.4) {
            *frame = [0.; 2];
        }
    }
    let result = run(&audio)
        .select(Selection::default(), &Control::default())
        .unwrap();
    assert_eq!(result.len(), 3, "{result:?}");
    for (i, onset) in result.iter().enumerate() {
        assert!((onset.seconds - expected[i]).abs() < 0.02, "{result:?}");
        assert_eq!(onset.band, [2, 1, 3][i], "{result:?}");
    }
}

#[test]
fn attacks_remain_visible_in_stationary_noise_with_controlled_false_positives() {
    let mut audio = Audio::new(24000, 3.);
    let mut seed = 731_u32;
    for frame in &mut audio.frames {
        for v in frame {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            *v = (seed as f64 / u32::MAX as f64 * 2. - 1.) as f32 * 0.025;
        }
    }
    let expected = [0.35, 0.8, 1.2, 1.7, 2.3];
    for (i, time) in expected.iter().enumerate() {
        audio.burst(
            *time,
            [100., 600., 3300., 180., 1200.][i],
            0.2,
            0.002,
            0.06,
            [1., 1.],
        );
    }
    let data = run(&audio);
    let actual = detected(&data);
    let result = score(&actual, &expected, 0.025);
    assert_eq!(result.0, expected.len(), "{result:?}: {actual:?}");
    assert!(result.1 <= 2, "{result:?}: {actual:?}");
}
