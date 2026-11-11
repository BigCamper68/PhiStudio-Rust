use phistudio_core::{text_composition, tools, *};
use serde_json::json;
fn defaults() -> NoteCreationDefaults {
    NoteCreationDefaults {
        size: 1.7,
        speed: 1.25,
        alpha: 127,
        above: 0,
        is_fake: 1,
        visible_time: 8.,
        y_offset: 13.,
        judge_area: Some(2.5),
        tint_hit_effects: Some([12, 34, 56]),
        ..Default::default()
    }
}
fn base() -> ChartDocument {
    ChartDocument::empty("Creation", 120.).unwrap()
}
#[test]
fn placement_defaults_preserve_tool_geometry_and_are_one_exact_undo() {
    for kind in [
        NoteKind::Tap,
        NoteKind::Hold,
        NoteKind::Flick,
        NoteKind::Drag,
    ] {
        let mut editor = Editor::new(base());
        let before = editor.document().as_json().clone();
        let original = Note {
            kind,
            start_time: "4".parse().unwrap(),
            end_time: "6".parse().unwrap(),
            x: 123.,
            ..Default::default()
        };
        let edit = tools::create_note(editor.document(), 0, original.clone())
            .unwrap()
            .with_note_defaults(
                &defaults(),
                &NoteCreationSounds::default(),
                NoteCreationPolicy::Regular,
                || false,
            )
            .unwrap();
        edit.apply(&mut editor).unwrap();
        let note = editor.document().note(0, 0).unwrap();
        assert_eq!(
            (note.kind, note.start_time, note.end_time, note.x),
            (kind, original.start_time, original.end_time, 123.)
        );
        assert_eq!(
            (note.size, note.speed, note.alpha, note.above, note.is_fake),
            (1.7, 1.25, 127, 0, 1)
        );
        assert_eq!(
            (
                note.visible_time,
                note.y_offset,
                note.judge_area,
                note.tint_hit_effects
            ),
            (8., 13., Some(2.5), Some([12, 34, 56]))
        );
        assert_eq!(editor.undo_label(), Some("Add note"));
        let after = editor.document().as_json().clone();
        editor.undo();
        assert_eq!(editor.document().as_json(), &before);
        editor.redo();
        assert_eq!(editor.document().as_json(), &after);
    }
}
#[test]
fn specialized_generators_keep_explicit_geometry_and_accept_remaining_defaults() {
    let raw = json!(Note {
        size: 0.8,
        speed: 3.,
        above: 1,
        is_fake: 0,
        visible_time: 42.,
        y_offset: 200.,
        alpha: 222,
        ..Default::default()
    });
    let audio: Note = serde_json::from_value(
        defaults()
            .decorate(
                &raw,
                &NoteCreationSounds::default(),
                NoteCreationPolicy::Audio,
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        (
            audio.size,
            audio.speed,
            audio.above,
            audio.alpha,
            audio.y_offset
        ),
        (0.8, 3., 1, 222, 200.)
    );
    assert_eq!((audio.visible_time, audio.judge_area), (8., Some(2.5)));
    let spec = text_composition::Spec {
        text: "A".into(),
        mode: text_composition::Mode::Tap,
        note_size: 0.8,
        ..Default::default()
    };
    let doc = base();
    let plan = text_composition::prepare(
        spec,
        include_bytes!("../../../assets/fonts/phira.ttf"),
        doc.tempo(),
    )
    .unwrap();
    let mut editor = Editor::new(doc);
    plan.with_note_defaults(&defaults(), &NoteCreationSounds::default(), || false)
        .unwrap()
        .apply(&mut editor)
        .unwrap();
    for (_, note) in editor.document().notes(1) {
        assert_eq!(
            (note.size, note.speed, note.is_fake, note.above, note.alpha),
            (0.8, 1., 1, 1, 127)
        );
        assert_eq!(note.judge_area, Some(2.5));
    }
}
#[test]
fn gain_metadata_matches_each_kind_and_rejects_stale_or_missing_preparation() {
    let defaults = NoteCreationDefaults {
        volume: 0.6,
        ..defaults()
    };
    let sounds = NoteCreationSounds {
        reference: None,
        gains: ["tap.wav", "flick.wav", "drag.wav"].map(|rendered| {
            Some(NoteSoundGain {
                source: None,
                rendered: rendered.into(),
                volume: 0.6,
            })
        }),
    };
    for kind in [
        NoteKind::Tap,
        NoteKind::Hold,
        NoteKind::Flick,
        NoteKind::Drag,
    ] {
        let raw = defaults
            .decorate(
                &json!(Note {
                    kind,
                    ..Default::default()
                }),
                &sounds,
                NoteCreationPolicy::Regular,
            )
            .unwrap();
        assert_eq!(
            raw["hitsound"],
            sounds.gains[NoteCreationSounds::index(kind)]
                .as_ref()
                .unwrap()
                .rendered
        );
        assert_eq!(raw["phiStudioSoundGain"]["volume"], 0.6);
    }
    assert!(
        defaults
            .decorate(
                &json!(Note::default()),
                &NoteCreationSounds::default(),
                NoteCreationPolicy::Regular
            )
            .is_err()
    );
    let wrong = NoteCreationDefaults {
        volume: 0.5,
        ..defaults
    };
    assert!(
        wrong
            .decorate(
                &json!(Note::default()),
                &sounds,
                NoteCreationPolicy::Regular
            )
            .is_err()
    );
}
#[test]
fn scoped_custom_sounds_and_preferences_round_trip_without_chart_changes() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project-a");
    let defaults = NoteCreationDefaults {
        sound: Some(DefaultNoteSound {
            reference: "sounds/test.wav".into(),
            project: Some(project.clone()),
        }),
        ..defaults()
    };
    assert_eq!(
        defaults.sound_reference(Some(&project)),
        Some("sounds/test.wav")
    );
    assert_eq!(
        defaults.sound_reference(Some(&root.path().join("project-b"))),
        None
    );
    let mut prefs = EditorPreferences::default();
    prefs.options.new_note_defaults = defaults.clone();
    save_editor_preferences(root.path(), &prefs).unwrap();
    assert_eq!(
        load_editor_preferences(root.path())
            .unwrap()
            .options
            .new_note_defaults,
        defaults
    );
    let legacy: EditorPreferences = serde_json::from_value(json!({"options":{}})).unwrap();
    assert_eq!(
        legacy.options.new_note_defaults,
        NoteCreationDefaults::default()
    );
    let normalized = NoteCreationDefaults {
        size: f64::NAN,
        above: 2,
        volume: 100.,
        sound: Some(DefaultNoteSound {
            reference: "../outside.wav".into(),
            project: Some(project),
        }),
        ..Default::default()
    }
    .normalized();
    assert_eq!(
        (
            normalized.size,
            normalized.above,
            normalized.volume,
            normalized.sound
        ),
        (1., 0, 1., None)
    );
}
#[test]
fn old_audio_generator_preferences_keep_explicit_styles_and_new_ones_follow_defaults() {
    assert!(tools::AudioNoteOptions::default().use_creation_defaults);
    let legacy: tools::AudioNoteOptions =
        serde_json::from_value(json!({"size":2.2,"speed":1.3})).unwrap();
    assert!(!legacy.use_creation_defaults);
    assert_eq!((legacy.size, legacy.speed), (2.2, 1.3));
}
#[test]
fn decoration_preserves_extensions_and_never_rewrites_existing_notes_or_copies() {
    let mut raw = json!(Note::default());
    raw["unknown"] = json!({"keep":[1,2,3]});
    let decorated = defaults()
        .decorate(
            &raw,
            &NoteCreationSounds::default(),
            NoteCreationPolicy::Regular,
        )
        .unwrap();
    assert_eq!(decorated["unknown"], raw["unknown"]);
    let mut editor = Editor::new(base());
    editor
        .execute(Command::InsertNote {
            line: 0,
            value: raw.clone(),
        })
        .unwrap();
    let original = editor.document().as_json()["judgeLineList"][0]["notes"][0].clone();
    tools::create_note(editor.document(), 0, Note::default())
        .unwrap()
        .with_note_defaults(
            &defaults(),
            &NoteCreationSounds::default(),
            NoteCreationPolicy::Regular,
            || false,
        )
        .unwrap()
        .apply(&mut editor)
        .unwrap();
    assert_eq!(
        editor.document().as_json()["judgeLineList"][0]["notes"][0],
        original
    );
    assert!(
        tools::create_note(editor.document(), 0, Note::default())
            .unwrap()
            .with_note_defaults(
                &defaults(),
                &NoteCreationSounds::default(),
                NoteCreationPolicy::Regular,
                || true
            )
            .is_err()
    );
}
