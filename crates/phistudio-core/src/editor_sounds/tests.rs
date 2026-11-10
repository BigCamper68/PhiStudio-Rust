use super::*;
#[test]
fn legacy_rules_become_a_complete_user_pack_and_survive_repeated_save_load() {
    let old = serde_json::json!({"enabled":false,"volume":0.6,"style":"Mechanical","pitch":1.2,"cooldown_ms":70,"events":{"move_resize":{"enabled":false,"file":"/private/slide.wav","volume":0.7,"pitch":0.8},"place":{"tone":"Glass"}}});
    let migrated: EditorSoundOptions = serde_json::from_value(old).unwrap();
    assert!(!migrated.enabled);
    assert_eq!(migrated.volume, 0.6);
    assert_eq!(migrated.packs.len(), 1);
    let pack = migrated.pack();
    assert_eq!(pack.style, EditorSoundStyle::Mechanical);
    assert_eq!((pack.pitch, pack.cooldown_ms), (1.2, 70));
    assert_eq!(
        pack.rule(EditorSoundEvent::MoveResize).files,
        vec!["/private/slide.wav"]
    );
    assert!(!pack.rule(EditorSoundEvent::MoveResize).enabled);
    assert_eq!(
        pack.rule(EditorSoundEvent::PlaceHold).tone,
        Some(EditorSoundTone::Glass)
    );
    let root = tempfile::tempdir().unwrap();
    let mut preferences = crate::EditorPreferences::default();
    preferences.options.editor_sounds = migrated;
    crate::save_editor_preferences(root.path(), &preferences).unwrap();
    assert_eq!(
        crate::load_editor_preferences(root.path()).unwrap(),
        preferences
    );
    let encoded = serde_json::to_vec(&preferences).unwrap();
    let again: crate::EditorPreferences = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(again, preferences);
}
#[test]
fn empty_and_style_only_legacy_settings_use_the_matching_builtin() {
    let preferences: crate::EditorPreferences = serde_json::from_str("{}").unwrap();
    assert_eq!(
        preferences.options.editor_sounds,
        EditorSoundOptions::default()
    );
    let options: EditorSoundOptions = serde_json::from_str(r#"{"style":"Arcade"}"#).unwrap();
    assert_eq!(
        options.pack(),
        std::borrow::Cow::Owned(EditorSoundPack::builtin(EditorSoundStyle::Arcade))
    );
    assert!(options.packs.is_empty());
}
#[test]
fn independent_pack_copies_and_switches_preserve_every_assignment() {
    let mut options = EditorSoundOptions::default();
    options.add_pack(EditorSoundPack::default());
    options
        .selected_pack_mut()
        .unwrap()
        .events
        .entry(EditorSoundEvent::Click)
        .or_default()
        .volume = 0.2;
    let first = options.selected_pack.clone();
    options.add_pack(options.pack().into_owned());
    options
        .selected_pack_mut()
        .unwrap()
        .events
        .entry(EditorSoundEvent::Click)
        .or_default()
        .volume = 0.8;
    assert_eq!(options.packs.len(), 2);
    assert_eq!(options.rule(EditorSoundEvent::Click).volume, 0.8);
    options.selected_pack = first;
    assert_eq!(options.rule(EditorSoundEvent::Click).volume, 0.2);
    options.selected_pack = EditorSoundPack::builtin(EditorSoundStyle::Soft).id;
    assert_eq!(options.rule(EditorSoundEvent::Click).volume, 1.);
    assert!(options.selected_pack_mut().is_none());
}
#[test]
fn pack_and_category_limits_are_bounded_and_unknown_selection_recovers() {
    let mut options = EditorSoundOptions {
        volume: f32::NAN,
        selected_pack: "missing".into(),
        ..Default::default()
    };
    let mut pack = EditorSoundPack {
        pitch: f32::INFINITY,
        cooldown_ms: 0,
        ..Default::default()
    };
    pack.events.insert(
        EditorSoundEvent::Click,
        EditorSoundRule {
            volume: -1.,
            pitch: 100.,
            files: vec!["a".into(), "a".into(), "".into(), "x".repeat(1025)],
            ..Default::default()
        },
    );
    pack.category_intervals
        .insert(EditorSoundCategory::Objects, 10000);
    options.add_pack(pack);
    options = options.normalized();
    assert_eq!(options.volume, 0.35);
    assert_eq!((options.pack().pitch, options.pack().cooldown_ms), (1., 20));
    assert_eq!(options.rule(EditorSoundEvent::Click).files, vec!["a"]);
    assert_eq!(options.pack().interval(EditorSoundEvent::MoveResize), 500);
    assert_eq!(options.pack().interval(EditorSoundEvent::Seek), 90);
    for _ in 1..EditorSoundOptions::MAX_PACKS {
        assert!(options.add_pack(EditorSoundPack::default()));
    }
    let selected = options.selected_pack.clone();
    assert!(!options.add_pack(EditorSoundPack::default()));
    assert_eq!(options.selected_pack, selected);
    options.selected_pack = "missing".into();
    assert_eq!(
        options.normalized().selected_pack,
        EditorSoundOptions::default().selected_pack
    );
}

#[test]
fn a_full_sound_pack_library_survives_the_preferences_file_size_limit() {
    let mut preferences = crate::EditorPreferences::default();
    let options = &mut preferences.options.editor_sounds;
    let mut pack = EditorSoundPack::default();
    for event in EditorSoundEvent::ALL {
        pack.events.insert(
            event,
            EditorSoundRule {
                files: (0..4)
                    .map(|i| format!("/private/{}-{i}.wav", "a".repeat(980)))
                    .collect(),
                ..Default::default()
            },
        );
    }
    for _ in 0..EditorSoundOptions::MAX_PACKS {
        assert!(options.add_pack(pack.clone()));
    }
    let root = tempfile::tempdir().unwrap();
    crate::save_editor_preferences(root.path(), &preferences).unwrap();
    assert!(
        std::fs::metadata(root.path().join("editor-settings.json"))
            .unwrap()
            .len()
            > 64 * 1024
    );
    assert_eq!(
        crate::load_editor_preferences(root.path()).unwrap(),
        preferences
    );
    std::fs::write(
        root.path().join("editor-settings.json"),
        vec![b' '; 8 * 1024 * 1024 + 1],
    )
    .unwrap();
    assert!(
        crate::load_editor_preferences(root.path())
            .unwrap_err()
            .to_string()
            .contains("8 MiB")
    );
}
