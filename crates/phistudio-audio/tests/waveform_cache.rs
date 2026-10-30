use phistudio_audio::{Control, StereoSource, analyze, cache};
struct Audio(Vec<[f32; 2]>);
impl StereoSource for Audio {
    fn sample_rate(&self) -> u32 {
        8000
    }
    fn frame_count(&self) -> usize {
        self.0.len()
    }
    fn frame(&self, i: usize) -> [f32; 2] {
        self.0[i]
    }
}
#[test]
fn waveform_preserves_opposite_phase_transients_and_rms_through_zoom_levels() {
    let mut source = Audio(vec![[0.; 2]; 8003]);
    source.0[1023] = [1., -1.];
    source.0[5000] = [-0.5, 0.25];
    let result = analyze(&source, &Control::default(), |_| {}).unwrap();
    let all = result.waveform.range(0., 2.);
    assert_eq!(all.min, -1.);
    assert_eq!(all.max, 1.);
    assert!((all.rms - ((1. + (0.25 + 0.0625) * 0.5) / 8003_f32).sqrt()).abs() < 1e-7);
    let impulse = result.waveform.range(1023. / 8000., 1024. / 8000.);
    assert_eq!(impulse.min, -1.);
    assert_eq!(impulse.max, 1.);
    assert_eq!(result.waveform.range(-1., 0.).rms, 0.);
    assert_eq!(result.waveform.range(3., 4.).max, 0.);
    assert_eq!(result.waveform.range(f64::NAN, 4.).max, 0.);
    assert_eq!(result.waveform.range(0., 0.1).rms, 0.);
    assert!(result.waveform.memory_bytes() < 40_000);
}
#[test]
fn cache_roundtrips_validates_checksums_and_invalidates_changed_pcm() {
    let root = tempfile::tempdir().unwrap();
    let mut source = Audio(vec![[0.; 2]; 4000]);
    source.0[1000] = [0.7, -0.3];
    let control = Control::default();
    let key = cache::fingerprint(&source, &control).unwrap();
    let data = analyze(&source, &control, |_| {}).unwrap();
    cache::store(root.path(), &key, &data, &control).unwrap();
    let loaded = cache::load(root.path(), &key, &control).unwrap();
    assert_eq!(
        loaded.waveform.range(0., 1.).max,
        data.waveform.range(0., 1.).max
    );
    assert_eq!(
        loaded.select(Default::default(), &control).unwrap(),
        data.select(Default::default(), &control).unwrap()
    );
    source.0[2000] = [0.1, 0.1];
    assert_ne!(key, cache::fingerprint(&source, &control).unwrap());
    let file = std::fs::read_dir(root.path().join("audio-analysis-cache"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut bytes = std::fs::read(&file).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(&file, &bytes).unwrap();
    assert!(cache::load(root.path(), &key, &control).is_none());
    std::fs::write(&file, &bytes[..90]).unwrap();
    assert!(cache::load(root.path(), &key, &control).is_none());
    assert!(cache::load(root.path(), "../outside", &control).is_none());
    control.cancel();
    assert!(cache::load(root.path(), &key, &control).is_none());
}
#[test]
fn partial_waveform_is_ready_before_cancellation_and_invalid_audio_is_local() {
    let source = Audio(vec![[0.; 2]; 16000]);
    let control = Control::default();
    let mut published = false;
    let result = analyze(&source, &control, |wave| {
        assert_eq!(wave.duration(), 2.);
        published = true;
        control.cancel();
    });
    assert!(published && result.is_err());
    let invalid = Audio(vec![[f32::NAN, 0.]; 100]);
    assert!(analyze(&invalid, &Control::default(), |_| {}).is_err());
}
