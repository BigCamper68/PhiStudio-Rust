use phistudio_core::*;
#[test]
fn preferences_round_trip_layout_and_audio_without_touching_project_or_recovery() {
    let root = tempfile::tempdir().unwrap();
    let saved = create_project(root.path(), &ChartDocument::empty("Saved", 137.).unwrap()).unwrap();
    let original = std::fs::read(&saved.chart).unwrap();
    let recovery = RecoverySnapshot {
        document: ChartDocument::empty("Recovery", 123.).unwrap(),
        location: None,
        beat: 4.,
        line: 0,
    };
    save_recovery(root.path(), &recovery).unwrap();
    let bytes = std::fs::read(root.path().join("recovery/session.json")).unwrap();
    let preferences = EditorPreferences {
        lanes: 21,
        division: 12,
        zoom: 150.,
        timeline_split: 0.6,
        sidebar: false,
        floating_preview: true,
        floating_properties: true,
        grid_window: true,
        music_volume: 0.3,
        hitsound_volume: 0.6,
        options: EditorOptions {
            audio_notes: tools::AudioNoteOptions {
                position: tools::AudioPosition::Random,
                seed: 975,
                every: 3,
                ..Default::default()
            },
            numeric_drag: NumericDragOptions {
                enabled: false,
                sensitivity: 2.5,
                snap: true,
                step: 0.05,
            },
            language: Language::SimplifiedChinese,
            preview_grid_brightness: 0.8,
            audio_guide: AudioGuideOptions {
                waveform: true,
                attacks: false,
                opacity: 0.3,
                gain: 2.,
                sensitivity: 0.7,
                min_gap_ms: 45,
                band: 2,
            },
            ..Default::default()
        },
        ..Default::default()
    };
    save_editor_preferences(root.path(), &preferences).unwrap();
    assert_eq!(load_editor_preferences(root.path()).unwrap(), preferences);
    assert_eq!(std::fs::read(saved.chart).unwrap(), original);
    assert_eq!(
        std::fs::read(root.path().join("recovery/session.json")).unwrap(),
        bytes
    );
}
#[test]
fn invalid_values_are_bounded_and_unknown_versions_are_left_untouched() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("editor-settings.json");
    let preferences = EditorPreferences {
        lanes: 0,
        division: 0,
        zoom: f32::NAN,
        timeline_split: 20.,
        music_volume: -1.,
        hitsound_volume: 20.,
        options: EditorOptions {
            preview_grid_brightness: f32::NAN,
            ..Default::default()
        },
        ..Default::default()
    };
    save_editor_preferences(root.path(), &preferences).unwrap();
    let loaded = load_editor_preferences(root.path()).unwrap();
    assert_eq!(loaded.options.preview_grid_brightness, 0.5);
    assert_eq!((loaded.lanes, loaded.division, loaded.zoom), (11, 4, 96.));
    assert_eq!(
        (
            loaded.timeline_split,
            loaded.music_volume,
            loaded.hitsound_volume
        ),
        (0.7, 0., 1.)
    );
    std::fs::write(&path, b"{\"version\":999,\"future\":true}").unwrap();
    assert!(load_editor_preferences(root.path()).is_err());
    assert_eq!(
        std::fs::read(path).unwrap(),
        b"{\"version\":999,\"future\":true}"
    );
}

#[test]
fn notes_seek_limit_defaults_off_for_legacy_preferences_and_survives_saving() {
    let legacy: EditorOptions = serde_json::from_str("{\"snap_time\":false}").unwrap();
    assert_eq!(legacy.numeric_drag, NumericDragOptions::default());
    assert!(!legacy.seek_notes_only);
    assert!(!legacy.audio_guide.waveform && !legacy.audio_guide.attacks);
    assert_eq!(legacy.preview_grid_brightness, 0.5);
    let root = tempfile::tempdir().unwrap();
    let mut preferences = EditorPreferences::default();
    preferences.options.seek_notes_only = true;
    save_editor_preferences(root.path(), &preferences).unwrap();
    assert!(
        load_editor_preferences(root.path())
            .unwrap()
            .options
            .seek_notes_only
    );
}

#[test]
fn legacy_and_unknown_languages_fall_back_to_english() {
    let legacy: EditorOptions = serde_json::from_str("{}").unwrap();
    assert_eq!(legacy.language, Language::English);
    for code in ["en", "en-US", "future-language"] {
        let options: EditorOptions =
            serde_json::from_value(serde_json::json!({"language": code})).unwrap();
        assert_eq!(options.language, Language::English);
    }
    for code in ["zh-CN", "zh-Hans"] {
        let options: EditorOptions =
            serde_json::from_value(serde_json::json!({"language": code})).unwrap();
        assert_eq!(options.language, Language::SimplifiedChinese);
        assert_eq!(serde_json::to_value(options).unwrap()["language"], "zh-CN");
    }
}

#[test]
fn every_supported_language_round_trips_with_a_stable_code() {
    let root = tempfile::tempdir().unwrap();
    let mut codes = std::collections::BTreeSet::new();
    for language in Language::ALL {
        assert!(codes.insert(language.code()));
        let mut preferences = EditorPreferences::default();
        preferences.options.language = language;
        save_editor_preferences(root.path(), &preferences).unwrap();
        assert_eq!(load_editor_preferences(root.path()).unwrap(), preferences);
        assert_eq!(serde_json::to_value(language).unwrap(), language.code());
    }
    for alias in ["zh-TW", "zh-Hant", "zh-Hant-TW"] {
        assert_eq!(
            serde_json::from_value::<Language>(serde_json::json!(alias)).unwrap(),
            Language::TraditionalChinese
        );
    }
}

#[test]
fn regional_language_codes_resolve_to_the_registered_catalogue() {
    for (language, code, regional, native) in [
        (Language::Japanese, "ja", "ja-JP", "ja-JP"),
        (Language::Korean, "ko", "ko-KR", "ko-KR"),
        (Language::Russian, "ru", "ru-RU", "ru-RU"),
        (Language::Spanish, "es", "es-ES", "en-US"),
        (Language::Spanish, "es", "es-MX", "en-US"),
    ] {
        for alias in [code, regional] {
            assert_eq!(
                serde_json::from_value::<Language>(serde_json::json!(alias)).unwrap(),
                language
            );
        }
        assert_eq!(language.phira_locale(), native);
    }
}
