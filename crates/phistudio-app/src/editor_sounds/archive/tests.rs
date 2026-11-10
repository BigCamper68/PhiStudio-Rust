use super::*;
use phistudio_core::{EditorSoundEvent as Event, EditorSoundOptions, EditorSoundRule};
#[test]
fn portable_pack_round_trip_embeds_audio_and_rewrites_all_references() {
    let root = tempfile::tempdir().unwrap();
    let audio = root.path().join("user.wav");
    let bytes = crate::media::tests::wav_bytes();
    std::fs::write(&audio, &bytes).unwrap();
    let mut pack = EditorSoundPack {
        name: "Portable".into(),
        pitch: 1.2,
        ..Default::default()
    };
    pack.events.insert(
        Event::MoveResize,
        EditorSoundRule {
            files: vec![audio.to_string_lossy().into_owned()],
            volume: 0.7,
            ..Default::default()
        },
    );
    pack.events
        .insert(Event::Place, pack.rule(Event::MoveResize));
    let exported = export(pack.clone()).unwrap();
    let raw = std::fs::read_to_string(exported.path()).unwrap();
    assert!(!raw.contains(&root.path().to_string_lossy().to_string()));
    let archive: Archive = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        archive.sounds.len(),
        1,
        "Shared files are exported only once"
    );
    std::fs::remove_file(&audio).unwrap();
    let library = root.path().join("imported");
    let imported = import(exported.path(), &library).unwrap();
    assert_eq!(imported.pitch, 1.2);
    let rule = imported.rule(Event::MoveResize);
    assert_eq!(rule.volume, 0.7);
    assert_eq!(std::fs::read(&rule.files[0]).unwrap(), bytes);
    assert_eq!(imported.rule(Event::Place).files, rule.files);
    assert!(phistudio_prpr::load_interface_sound(Path::new(&rule.files[0])).is_ok());
}
#[test]
fn invalid_or_external_references_and_corrupt_audio_never_commit_imported_files() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("pack.json");
    let library = root.path().join("library");
    let mut archive = Archive {
        format: "phistudio.soundpack".into(),
        version: 1,
        pack: EditorSoundPack::default(),
        sounds: BTreeMap::new(),
    };
    archive.pack.events.insert(
        Event::Click,
        EditorSoundRule {
            files: vec!["/private/secret".into()],
            ..Default::default()
        },
    );
    std::fs::write(&source, serde_json::to_vec(&archive).unwrap()).unwrap();
    assert!(import(&source, &library).is_err());
    assert!(!library.exists());
    // The ID is data, never a filesystem path; invalid audio rolls staging back.
    archive
        .sounds
        .insert("/private/secret".into(), STANDARD.encode(b"broken audio"));
    std::fs::write(&source, serde_json::to_vec(&archive).unwrap()).unwrap();
    assert!(import(&source, &library).is_err());
    assert_eq!(std::fs::read_dir(&library).unwrap().count(), 0);
    archive.version = 2;
    std::fs::write(&source, serde_json::to_vec(&archive).unwrap()).unwrap();
    assert!(import(&source, &library).is_err());
}

#[test]
fn all_actions_can_have_four_portable_variants() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("complete.phisounds.json");
    let bytes = crate::media::tests::wav_bytes();
    let mut archive = Archive {
        format: "phistudio.soundpack".into(),
        version: 1,
        pack: EditorSoundPack::default(),
        sounds: BTreeMap::new(),
    };
    for (action, event) in Event::ALL.into_iter().enumerate() {
        let paths = (0..4)
            .map(|variant| {
                let id = format!("{action}-{variant}");
                archive.sounds.insert(id.clone(), STANDARD.encode(&bytes));
                id
            })
            .collect();
        archive.pack.events.insert(
            event,
            EditorSoundRule {
                files: paths,
                ..Default::default()
            },
        );
    }
    assert_eq!(archive.sounds.len(), MAX_PACK_FILES);
    std::fs::write(&source, serde_json::to_vec(&archive).unwrap()).unwrap();
    let imported = import(&source, &root.path().join("library")).unwrap();
    let mut options = EditorSoundOptions::default();
    options.add_pack(imported.clone());
    let prepared = crate::editor_sounds::prepare(crate::editor_sounds::Sources::new(&options));
    assert!(prepared.warning.is_none(), "{:?}", prepared.warning);
    assert_eq!(prepared.files.len(), MAX_PACK_FILES);
    assert!(
        Event::ALL
            .into_iter()
            .all(|event| imported.rule(event).files.len() == 4)
    );
    let again = export(imported).unwrap();
    assert_eq!(
        serde_json::from_slice::<Archive>(&std::fs::read(again.path()).unwrap())
            .unwrap()
            .sounds
            .len(),
        MAX_PACK_FILES
    );
}
