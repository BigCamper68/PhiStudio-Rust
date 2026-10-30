use phistudio_core::{
    ChartFormat, Command, Editor, Note, ProjectLocation, import_project, list_projects, load_chart,
    load_project_options, open_project, remember_project, save_chart,
};
use serde_json::json;
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};
use zip::{ZipWriter, write::SimpleFileOptions};
const DEMO: &str = include_str!("../../../assets/charts/phistudio-demo.rpe.json");
const PEC: &str = include_str!("fixtures/conversion/pec-overlap.pec");
const OFFICIAL: &str = include_str!("fixtures/conversion/official-v3.json");
fn archive(path: &Path, entries: &[(&str, &[u8])]) {
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}
#[test]
fn content_detection_imports_all_formats_and_txt_can_select_pec_over_rpe() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("mixed.pez");
    archive(
        &path,
        &[
            ("chart.data", DEMO.as_bytes()),
            ("official.bin", OFFICIAL.as_bytes()),
            ("legacy.json", PEC.as_bytes()),
            (
                "info.txt",
                b"#\nName: Legacy title\nChart: legacy.json\nSong: song.ogg\nPicture: art.png\n",
            ),
            ("song.ogg", b"audio"),
            ("hit.ogg", b"hit"),
            ("art.png", b"image"),
        ],
    );
    let project = import_project(&path, &root.path().join("projects"), "mixed.pez", true).unwrap();
    assert_eq!(project.charts.len(), 3);
    let choice = &project.charts[project.suggested.unwrap()];
    assert_eq!(choice.format, ChartFormat::LegacyPec);
    assert_eq!(choice.name, "Legacy title");
    let document = load_chart(&choice.path).unwrap();
    assert_eq!(document.name(), "Legacy title");
    let options = load_project_options(&choice.path).unwrap();
    assert_eq!(options.music.as_deref(), Some("song.ogg"));
    assert_eq!(options.illustration.as_deref(), Some("art.png"));
    assert_eq!(fs::read(&choice.source).unwrap(), PEC.as_bytes());
    let official = project
        .charts
        .iter()
        .find(|c| c.format == ChartFormat::OfficialPhigros)
        .unwrap();
    assert_eq!(fs::read(&official.source).unwrap(), OFFICIAL.as_bytes());
    let rpe = project
        .charts
        .iter()
        .find(|c| c.format == ChartFormat::Rpe)
        .unwrap();
    assert_eq!(fs::read(&rpe.source).unwrap(), DEMO.as_bytes());
    assert_eq!(
        load_chart(&rpe.path).unwrap().as_json(),
        &serde_json::from_str::<serde_json::Value>(DEMO).unwrap()
    );
}
#[test]
fn manifest_display_and_playback_settings_do_not_replace_authored_rpe_meta() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("pack.zip");
    let mut raw: serde_json::Value = serde_json::from_str(DEMO).unwrap();
    raw["META"]["name"] = json!("Authored title");
    raw["META"]["id"] = json!("rpe-id");
    raw["META"]["offset"] = json!(125);
    raw["META"]["song"] = json!("song.ogg");
    raw["META"]["futureMeta"] = json!({"untouched":true});
    raw["META"].as_object_mut().unwrap().remove("backgroundDim");
    let bytes = serde_json::to_vec(&raw).unwrap();
    let info=b"  name: \"Manifest # title\" # comment\n  chart: chart.data\n  level: null\n  music: song.ogg\n  illustration: cover.png\n  backgroundDim: 0.1\n  offset: -0.0015\n  useRpe170Speed: true\n  noteUniformScale: true\n  aspectRatio: 1.5\n  id: 53239\n  intro: |-\n    charter: nested must not override\n";
    archive(
        &path,
        &[
            ("chart.data", &bytes),
            ("info.yml", info),
            ("info.txt", b"Level: AT 17\nCharter: TXT author\n"),
            ("song.ogg", b"audio"),
            ("hit.ogg", b"sound"),
            ("cover.png", b"image"),
        ],
    );
    let project = import_project(&path, &root.path().join("projects"), "pack.zip", true).unwrap();
    let choice = &project.charts[project.suggested.unwrap()];
    let document = load_chart(&choice.path).unwrap();
    let options = load_project_options(&choice.path).unwrap();
    assert_eq!(document.as_json(), &raw);
    assert_eq!(options.display_field(&document, "name"), "Manifest # title");
    assert_eq!(options.display_field(&document, "level"), "AT 17");
    assert_eq!(options.display_field(&document, "charter"), "TXT author");
    assert_eq!(options.music_offset_seconds(&document), 0.123);
    assert_eq!(options.background_dim(&document), 0.1_f32);
    assert!(options.use_rpe_170_speed && options.note_uniform_scale);
    assert_eq!(options.aspect_ratio, Some(1.5));
    let mut changed = Editor::new(document);
    changed
        .execute(Command::RenameChart("Edited title".into()))
        .unwrap();
    assert_eq!(
        options.display_field(changed.document(), "name"),
        "Edited title"
    );
    save_chart(changed.document(), &choice.path).unwrap();
    assert_eq!(fs::read(project.assets.join("info.yml")).unwrap(), info);
    assert_eq!(fs::read(&choice.source).unwrap(), bytes);
    remember_project(
        root.path(),
        ProjectLocation {
            chart: choice.path.clone(),
            assets: Some(project.assets.clone()),
        },
    )
    .unwrap();
    let library = list_projects(root.path()).unwrap();
    assert_eq!(library.projects[0].name, "Edited title");
    assert_eq!(library.projects[0].source_name, "pack.zip");
    let reopened = open_project(root.path(), &project.workspace).unwrap();
    assert_eq!(reopened.charts.len(), 1);
    assert_eq!(reopened.charts[0].name, "Edited title");
}
#[test]
fn editing_and_reopening_a_converted_chart_does_not_reconvert_or_duplicate_it() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("chart.json");
    fs::write(&source, PEC).unwrap();
    let project =
        import_project(&source, &root.path().join("projects"), "chart.json", false).unwrap();
    let chart = &project.charts[0];
    assert_eq!(chart.format, ChartFormat::LegacyPec);
    let mut editor = Editor::new(load_chart(&chart.path).unwrap());
    let original = editor.document().note_count();
    editor
        .execute(Command::AddNote {
            line: 0,
            note: Note::default(),
        })
        .unwrap();
    save_chart(editor.document(), &chart.path).unwrap();
    remember_project(
        root.path(),
        ProjectLocation {
            chart: chart.path.clone(),
            assets: Some(project.assets.clone()),
        },
    )
    .unwrap();
    let reopened = open_project(root.path(), &project.workspace).unwrap();
    assert_eq!(reopened.charts.len(), 1);
    assert_eq!(reopened.charts[0].notes, original + 1);
    assert_eq!(reopened.charts[0].path, chart.path);
    assert_eq!(fs::read(&chart.source).unwrap(), PEC.as_bytes());
    assert_eq!(fs::read(&source).unwrap(), PEC.as_bytes());
}
#[test]
fn rpe_has_priority_without_a_manifest_and_bad_manifest_paths_never_publish() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("mixed.zip");
    let projects = root.path().join("projects");
    archive(
        &path,
        &[("a.bin", OFFICIAL.as_bytes()), ("b.bin", DEMO.as_bytes())],
    );
    let project = import_project(&path, &projects, "mixed.zip", true).unwrap();
    assert_eq!(
        project.charts[project.suggested.unwrap()].format,
        ChartFormat::Rpe
    );
    let count = fs::read_dir(&projects).unwrap().count();
    for info in [
        "chart: ../outside.json\n",
        "chart: chart.data\nmusic: ../outside.ogg\n",
    ] {
        archive(
            &path,
            &[
                ("info.yml", info.as_bytes()),
                ("chart.data", DEMO.as_bytes()),
            ],
        );
        assert!(import_project(&path, &projects, "bad.zip", true).is_err());
        assert_eq!(fs::read_dir(&projects).unwrap().count(), count);
    }
    let invalid = OFFICIAL.replace("\"formatVersion\": 3", "\"formatVersion\": 2");
    archive(
        &path,
        &[
            ("info.yml", b"chart: official.json\n"),
            ("official.json", invalid.as_bytes()),
        ],
    );
    let error = import_project(&path, &projects, "unsupported.zip", true)
        .unwrap_err()
        .to_string();
    assert!(error.contains("formatVersion"), "{error}");
}
#[test]
fn private_description_paths_are_validated_and_do_not_collide_with_archive_files() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("pack.zip");
    archive(
        &path,
        &[
            (".phistudio-workspace.json", DEMO.as_bytes()),
            ("info.yml", b"chart: .phistudio-workspace.json\n"),
        ],
    );
    let project = import_project(&path, &root.path().join("projects"), "pack.zip", true).unwrap();
    assert_eq!(
        fs::read(project.assets.join(".phistudio-workspace.json")).unwrap(),
        DEMO.as_bytes()
    );
    let description_path = project.workspace.join(".phistudio-workspace.json");
    let mut description: serde_json::Value =
        serde_json::from_slice(&fs::read(&description_path).unwrap()).unwrap();
    description["charts"][0]["working"] = json!("../../external.json");
    fs::write(&description_path, description.to_string()).unwrap();
    assert!(open_project(root.path(), &project.workspace).is_err());
    assert!(load_project_options(&project.charts[0].path).is_err());
}

#[test]
fn earlier_flat_projects_get_manifest_resources_without_moving_saved_files() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("projects/legacy");
    fs::create_dir_all(&workspace).unwrap();
    let path = workspace.join("chart.json");
    fs::write(&path, DEMO).unwrap();
    fs::write(workspace.join("info.yml"),"name: Legacy display\nmusic: song.ogg\nillustration: cover.png\nbackgroundDim: 0.1\nholdPartialCover: true\nforceAspectRatio: true\nuseAttachUiFix: true\n").unwrap();
    fs::write(workspace.join("song.ogg"), b"audio").unwrap();
    fs::write(workspace.join("cover.png"), b"image").unwrap();
    let options =
        phistudio_core::load_project_options_with_assets(&path, Some(&workspace)).unwrap();
    let mut document = load_chart(&path).unwrap();
    assert_eq!(options.display_field(&document, "name"), "Legacy display");
    assert_eq!(options.music.as_deref(), Some("song.ogg"));
    assert!(options.hold_partial_cover && options.force_aspect_ratio);
    assert_eq!(options.use_attach_ui_fix, Some(true));
    assert_eq!(options.background_dim(&document), 0.1_f32);
    let mut value = document.as_json().clone();
    value["META"]["backgroundDim"] = json!(0.8);
    document = phistudio_core::ChartDocument::from_value(value).unwrap();
    assert_eq!(options.background_dim(&document), 0.8_f32);
    assert_eq!(fs::read(path).unwrap(), DEMO.as_bytes());
}
