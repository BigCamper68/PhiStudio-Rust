use super::*;
#[test]
fn create_chart_requires_both_media_selections() {
    let root = tempfile::tempdir().unwrap();
    let mut draft = NewChart::default();
    assert!(!draft.ready());
    draft.audio = Some(MediaFile::new(
        root.path().join("audio.wav"),
        "audio.wav".into(),
        false,
    ));
    assert!(!draft.ready());
    draft.illustration = Some(MediaFile::new(
        root.path().join("art.png"),
        "art.png".into(),
        false,
    ));
    assert!(draft.ready());
    draft.name.clear();
    assert!(!draft.ready());
}
