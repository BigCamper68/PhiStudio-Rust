use super::*;
use phistudio_core::{Command, Note};

fn encoded(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut data = b"PhiStudio project 1\n".to_vec();
    data.extend((entries.len() as u32).to_le_bytes());
    for (path, bytes) in entries {
        data.extend((path.len() as u32).to_le_bytes());
        data.extend((bytes.len() as u64).to_le_bytes());
        data.extend(path.as_bytes());
        data.extend(*bytes);
    }
    data
}

#[test]
#[ignore = "requires the separately built project-pack Wasm example"]
fn compiled_wasm_project_pack_rejects_escaping_truncated_duplicate_and_trailing_data_without_losing_edits()
 {
    let root = tempfile::tempdir().unwrap();
    let mut app = Workbench::new(
        ChartDocument::empty("Keep this document", 120.).unwrap(),
        None,
    );
    let mut files = FileManager::new(Some(root.path().into()));
    let ctx = egui::Context::default();
    app.start_plugins(Some(root.path().into()));
    settle(&mut app, &mut files, &ctx);
    let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/plugins/project-pack");
    app.install_plugin(MediaFile::new(package, "Project pack".into(), false));
    settle(&mut app, &mut files, &ctx);
    app.plugin_action(Action::Enable("example.project-pack".into()));
    settle(&mut app, &mut files, &ctx);
    app.editor
        .execute(Command::AddNote {
            line: 0,
            note: Note::default(),
        })
        .unwrap();
    let original = app.editor.document().as_json().clone();
    app.plugins.open = true;
    let mut truncated = encoded(&[("chart.json", b"missing bytes")]);
    truncated.truncate(truncated.len() - 3);
    let mut trailing = encoded(&[("chart.json", &serde_json::to_vec(&original).unwrap())]);
    trailing.extend(b"unexpected trailing data");
    let mut count = b"PhiStudio project 1\n".to_vec();
    count.extend(1025u32.to_le_bytes());
    for (name, data) in [
        ("escape", encoded(&[("../outside.rpe.json", b"")])),
        (
            "duplicate",
            encoded(&[("chart.json", b""), ("chart.json", b"")]),
        ),
        ("truncated", truncated),
        ("trailing", trailing),
        ("count", count),
    ] {
        app.error = None;
        let source = root.path().join(format!("{name}.pspack"));
        std::fs::write(&source, data).unwrap();
        click(&mut app, &mut files, &ctx, "Import with plugin");
        app.plugins.providers.dialog.as_mut().unwrap().source =
            source.to_string_lossy().into_owned();
        click(&mut app, &mut files, &ctx, "Import with plugin");
        settle(&mut app, &mut files, &ctx);
        assert!(app.error.is_some(), "{name}");
        assert_eq!(app.editor.document().as_json(), &original, "{name}");
        assert!(app.editor.is_dirty() && app.editor.can_undo());
        assert!(app.files.pending_open.is_none());
        assert_eq!(
            app.plugins.registry.plugins[0].status,
            phistudio_plugins::Status::Enabled
        );
    }
    assert!(!root.path().join("outside.rpe.json").exists());
    app.plugins.shutdown().unwrap();
}

#[test]
#[ignore = "requires the separately built project-pack Wasm example"]
fn compiled_wasm_project_pack_streams_large_resources_and_paged_entries_without_losing_unsaved_chart_data()
 {
    let root = tempfile::tempdir().unwrap();
    let mut value = ChartDocument::empty("Packed project", 128.)
        .unwrap()
        .as_json()
        .clone();
    value["phiStudioPlugins"] =
        json!({"future.extension":{"keep":[null,{"text":"Exact extension data"}]}});
    value["judgeLineList"][0]["futureLine"] = json!({"keep":true});
    let document = ChartDocument::from_value(value).unwrap();
    let location = phistudio_core::create_project(root.path(), &document).unwrap();
    let assets = location.assets.unwrap();
    std::fs::create_dir_all(assets.join("audio")).unwrap();
    std::fs::create_dir_all(assets.join("art")).unwrap();
    std::fs::create_dir_all(assets.join("data/files")).unwrap();
    std::fs::create_dir_all(assets.join("textures/雪")).unwrap();
    let audio = crate::media::tests::wav_bytes();
    let image = include_bytes!("../../../../../../assets/skins/phigros/click.png");
    std::fs::write(assets.join("audio/music.wav"), &audio).unwrap();
    std::fs::write(assets.join("art/cover.png"), image).unwrap();
    let binary: Vec<u8> = (0..phistudio_sdk::MAX_MESSAGE_BYTES + 1234)
        .map(|i| (i % 251) as u8)
        .collect();
    std::fs::write(assets.join("textures/雪/raw.bin"), &binary).unwrap();
    for index in 0..300 {
        std::fs::write(
            assets.join(format!("data/files/{index:03}.bin")),
            format!("resource {index}"),
        )
        .unwrap();
    }
    let saved = std::fs::read(&location.chart).unwrap();
    let mut app = Workbench::new(document, Some(location.chart.clone()));
    app.project_options.music = Some("audio/music.wav".into());
    app.project_options.illustration = Some("art/cover.png".into());
    let mut files = FileManager::new(Some(root.path().into()));
    let ctx = egui::Context::default();
    app.start_plugins(Some(root.path().into()));
    settle(&mut app, &mut files, &ctx);
    let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/plugins/project-pack");
    app.install_plugin(MediaFile::new(package, "Project pack".into(), false));
    settle(&mut app, &mut files, &ctx);
    app.plugin_action(Action::Enable("example.project-pack".into()));
    settle(&mut app, &mut files, &ctx);
    assert!(app.error.is_none(), "{:?}", app.error);
    app.editor
        .execute(Command::AddNote {
            line: 0,
            note: Note::default(),
        })
        .unwrap();
    let expected = app.editor.document().as_json().clone();
    app.plugins.open = true;
    let destination = root.path().join("project.pspack");
    click(&mut app, &mut files, &ctx, "Export with plugin");
    app.plugins.providers.dialog.as_mut().unwrap().destination =
        destination.to_string_lossy().into_owned();
    let started = Instant::now();
    click(&mut app, &mut files, &ctx, "Export with plugin");
    settle(&mut app, &mut files, &ctx);
    eprintln!(
        "Project pack export (300 entries, >16 MiB resource): {:?}",
        started.elapsed()
    );
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.editor.is_dirty());
    assert_eq!(app.editor.document().as_json(), &expected);
    assert_eq!(std::fs::read(&location.chart).unwrap(), saved);
    let packed = std::fs::read(&destination).unwrap();
    assert!(packed.starts_with(b"PhiStudio project 1\n"));
    assert!(packed.len() > phistudio_sdk::MAX_MESSAGE_BYTES);
    click(&mut app, &mut files, &ctx, "Import with plugin");
    app.plugins.providers.dialog.as_mut().unwrap().source =
        destination.to_string_lossy().into_owned();
    let started = Instant::now();
    click(&mut app, &mut files, &ctx, "Import with plugin");
    settle(&mut app, &mut files, &ctx);
    eprintln!(
        "Project pack import (300 entries, >16 MiB resource): {:?}",
        started.elapsed()
    );
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.files.pending_open.is_some());
    assert_eq!(app.editor.document().as_json(), &expected);
    assert!(app.editor.is_dirty());
    click(&mut app, &mut files, &ctx, "Discard and open");
    settle(&mut app, &mut files, &ctx);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(!app.editor.is_dirty() && !app.editor.can_undo());
    assert_eq!(
        app.editor.document().as_json()["phiStudioPlugins"],
        expected["phiStudioPlugins"]
    );
    assert_eq!(
        app.editor.document().as_json()["judgeLineList"][0]["futureLine"],
        expected["judgeLineList"][0]["futureLine"]
    );
    assert_eq!(app.editor.document().note_count(), 1);
    assert_eq!(
        app.project_options.music.as_deref(),
        Some("audio/music.wav")
    );
    assert_eq!(
        app.project_options.illustration.as_deref(),
        Some("art/cover.png")
    );
    let imported_assets = app.asset_root().unwrap().to_owned();
    assert_eq!(
        std::fs::read(imported_assets.join("audio/music.wav")).unwrap(),
        audio
    );
    assert_eq!(
        std::fs::read(imported_assets.join("art/cover.png")).unwrap(),
        image
    );
    assert_eq!(
        std::fs::read(imported_assets.join("textures/雪/raw.bin")).unwrap(),
        binary
    );
    for index in 0..300 {
        assert_eq!(
            std::fs::read_to_string(imported_assets.join(format!("data/files/{index:03}.bin")))
                .unwrap(),
            format!("resource {index}")
        );
    }
    app.plugin_action(Action::Disable("example.project-pack".into()));
    settle(&mut app, &mut files, &ctx);
    app.editor
        .execute(Command::AddNote {
            line: 0,
            note: Note::default(),
        })
        .unwrap();
    assert!(app.editor.undo());
    assert_eq!(
        app.editor.document().as_json()["phiStudioPlugins"],
        expected["phiStudioPlugins"]
    );
    let package = phistudio_core::prepare_project_export(
        app.editor.document(),
        &app.project_options,
        Some(&imported_assets),
        app.file.as_deref(),
    )
    .unwrap();
    let restored = phistudio_core::import_project(
        package.path(),
        &root.path().join("roundtrip"),
        "Native export",
        true,
    )
    .unwrap();
    assert_eq!(
        std::fs::read(restored.assets.join("textures/雪/raw.bin")).unwrap(),
        binary
    );
    assert_eq!(std::fs::read(&location.chart).unwrap(), saved);
    app.plugins.shutdown().unwrap();
}
