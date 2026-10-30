use phistudio_core::*;
use serde_json::json;
const ID: NoteId = NoteId { line: 0, index: 0 };
fn editor() -> Editor {
    let mut e = Editor::new(ChartDocument::empty("Note properties", 120.).unwrap());
    e.execute(Command::AddNote {
        line: 0,
        note: Note::default(),
    })
    .unwrap();
    e
}
#[test]
fn optional_note_fields_preserve_missing_null_and_unknowns_and_undo_exactly() {
    for null in [false, true] {
        let mut raw = editor().document().as_json().clone();
        let note = &mut raw["judgeLineList"][0]["notes"][0];
        note["unknown"] = json!({"keep":[1,null,3]});
        if null {
            note["judgeArea"] = json!(null);
            note["tintHitEffects"] = json!(null);
        }
        let mut e = Editor::new(ChartDocument::from_value(raw).unwrap());
        let before = e.document().as_json().clone();
        let mut note = e.document().note(0, 0).unwrap();
        note.x = 42.;
        e.execute(Command::UpdateNote { id: ID, note }).unwrap();
        let after = e.document().as_json();
        for key in ["judgeArea", "tintHitEffects", "unknown"] {
            assert_eq!(
                after["judgeLineList"][0]["notes"][0].get(key),
                before["judgeLineList"][0]["notes"][0].get(key)
            );
        }
        e.undo();
        assert_eq!(e.document().as_json(), &before);
        let mut note = e.document().note(0, 0).unwrap();
        note.judge_area = Some(1.75);
        note.tint_hit_effects = Some([12, 34, 56]);
        e.execute(Command::UpdateNote { id: ID, note }).unwrap();
        let custom = e.document().as_json().clone();
        assert_eq!(
            custom["judgeLineList"][0]["notes"][0]["tintHitEffects"],
            json!([12, 34, 56])
        );
        e.undo();
        assert_eq!(e.document().as_json(), &before);
        e.redo();
        assert_eq!(e.document().as_json(), &custom);
        let mut note = e.document().note(0, 0).unwrap();
        note.tint_hit_effects = None;
        e.execute(Command::UpdateNote { id: ID, note }).unwrap();
        assert!(
            e.document().as_json()["judgeLineList"][0]["notes"][0]
                .get("tintHitEffects")
                .is_none()
        );
    }
    for invalid in [-1., f64::NAN, f64::INFINITY, 1e100] {
        assert!(
            Note {
                judge_area: Some(invalid),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
}
#[test]
fn gain_is_reversible_restores_source_and_survives_clipboard_asset_remapping() {
    use phistudio_core::objects::ObjectClipboard;
    let mut e = editor();
    e.execute(Command::SetNoteHitsound {
        ids: vec![ID],
        reference: Some("original.wav".into()),
    })
    .unwrap();
    let original = e.document().as_json().clone();
    let gain = NoteSoundGain {
        source: Some("original.wav".into()),
        rendered: "rendered.wav".into(),
        volume: 0.5,
    };
    e.execute(Command::SetNoteVolume {
        id: ID,
        gain: Some(gain.clone()),
    })
    .unwrap();
    assert_eq!(
        e.document().note_hitsound(ID).unwrap(),
        Some("rendered.wav")
    );
    assert_eq!(e.document().note_sound_gain(ID), Some(gain.clone()));
    let half = e.document().as_json().clone();
    e.undo();
    assert_eq!(e.document().as_json(), &original);
    e.redo();
    assert_eq!(e.document().as_json(), &half);
    let mut clipboard = ObjectClipboard::copy(
        e.document(),
        &tools::Selection {
            notes: vec![ID],
            events: vec![],
        },
        &[],
    )
    .unwrap();
    assert_eq!(clipboard.custom_hitsounds().unwrap().len(), 2);
    clipboard.remap_hitsounds(
        &[
            ("original.wav".into(), "new-original.wav".into()),
            ("rendered.wav".into(), "new-rendered.wav".into()),
        ]
        .into(),
    );
    let text = serde_json::to_string(&clipboard).unwrap();
    assert!(text.contains("new-original.wav") && text.matches("new-rendered.wav").count() == 2);
    e.execute(Command::SetNoteVolume { id: ID, gain: None })
        .unwrap();
    assert_eq!(e.document().as_json(), &original);
    e.undo();
    assert_eq!(e.document().as_json(), &half);
    e.execute(Command::SetNoteHitsound {
        ids: vec![ID],
        reference: None,
    })
    .unwrap();
    assert!(e.document().note_sound_gain(ID).is_none());
    e.undo();
    assert_eq!(e.document().as_json(), &half);
    let invalid = NoteSoundGain {
        volume: f64::NAN,
        ..gain
    };
    assert!(
        e.execute(Command::SetNoteVolume {
            id: ID,
            gain: Some(invalid)
        })
        .is_err()
    );
    assert_eq!(e.document().as_json(), &half);
}
#[test]
fn generated_clips_are_deduplicated_and_original_files_are_immutable() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("original.wav"), b"original bytes").unwrap();
    let a = write_generated_hitsound(root.path(), b"generated wav").unwrap();
    let b = write_generated_hitsound(root.path(), b"generated wav").unwrap();
    assert_eq!(a, b);
    assert_ne!(
        a,
        write_generated_hitsound(root.path(), b"different wav").unwrap()
    );
    assert_eq!(
        std::fs::read(root.path().join("original.wav")).unwrap(),
        b"original bytes"
    );
    assert_eq!(
        std::fs::read(root.path().join(a)).unwrap(),
        b"generated wav"
    );
}
