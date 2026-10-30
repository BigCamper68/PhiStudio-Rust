use phistudio_core::*;
use serde_json::json;
use std::{
    fs,
    io::{Read, Write},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};
const DEMO: &str = include_str!("../../../assets/charts/phistudio-demo.rpe.json");
#[test]
fn exported_snapshot_round_trips_edits_assets_unknown_fields_and_signed_offset() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("original.pez");
    let mut zip = ZipWriter::new(fs::File::create(&archive).unwrap());
    for (name, bytes) in [
        ("chart.json", DEMO.as_bytes()),
        ("info.yml", b"chart: chart.json\nmusic: song.ogg\nillustration: cover.png\noffset: -0.125\nname: Package title\nuseRpe170Speed: true\nholdPartialCover: true\nforceAspectRatio: true\naspectRatio: 1.5\nuseAttachUIFix: false\nintro: custom metadata\n".as_slice()),
        ("song.ogg", b"music\0bytes"), ("cover.png", b"image bytes"), ("custom/hit.ogg", b"custom hitsound"), ("custom/shader.glsl", b"original shader"),
    ] { zip.start_file(name, SimpleFileOptions::default()).unwrap(); zip.write_all(bytes).unwrap(); }
    zip.finish().unwrap();
    let original = fs::read(&archive).unwrap();
    let project = import_project(
        &archive,
        &temp.path().join("projects"),
        "original.pez",
        true,
    )
    .unwrap();
    let path = &project.charts[project.suggested.unwrap()].path;
    let saved = fs::read(path).unwrap();
    let mut raw = load_chart(path).unwrap().as_json().clone();
    raw["META"]["future"] = json!({"preserve": [1,2,3]});
    raw["META"]["offset"] = json!(250);
    raw["judgeLineList"][0]["notes"][0]["positionX"] = json!(123.);
    raw["judgeLineList"][0]["notes"][0]["hitsound"] = json!("custom/hit.ogg");
    let document = ChartDocument::from_value(raw.clone()).unwrap();
    let options = load_project_options(path).unwrap();
    let export =
        prepare_project_export(&document, &options, Some(&project.assets), Some(path)).unwrap();
    let destination = temp.path().join("result.pez");
    fs::write(&destination, b"previous export").unwrap();
    export.write_to(&destination).unwrap();
    let mut zip = ZipArchive::new(fs::File::open(&destination).unwrap()).unwrap();
    for (path, bytes) in [
        ("song.ogg", b"music\0bytes".as_slice()),
        ("cover.png", b"image bytes"),
        ("custom/hit.ogg", b"custom hitsound"),
        ("custom/shader.glsl", b"original shader"),
    ] {
        let mut actual = Vec::new();
        zip.by_name(path).unwrap().read_to_end(&mut actual).unwrap();
        assert_eq!(actual, bytes);
    }
    let mut manifest = String::new();
    zip.by_name("info.yml")
        .unwrap()
        .read_to_string(&mut manifest)
        .unwrap();
    assert!(manifest.contains("custom metadata"));
    let imported = import_project(
        &destination,
        &temp.path().join("roundtrip"),
        "result.pez",
        true,
    )
    .unwrap();
    let chart = &imported.charts[imported.suggested.unwrap()].path;
    let actual = load_chart(chart).unwrap();
    assert_eq!(actual.as_json(), &raw);
    let actual_options = load_project_options(chart).unwrap();
    assert_eq!(
        actual_options.display_field(&actual, "name"),
        "Package title"
    );
    assert_eq!(actual_options.music_offset_seconds(&actual), 0.125);
    assert!(
        actual_options.use_rpe_170_speed
            && actual_options.hold_partial_cover
            && actual_options.force_aspect_ratio
    );
    assert_eq!(actual_options.use_attach_ui_fix, Some(false));
    assert_eq!(actual_options.aspect_ratio, Some(1.5));
    assert_eq!(fs::read(path).unwrap(), saved);
    assert_eq!(fs::read(archive).unwrap(), original);
}
#[test]
fn blank_projects_export_and_failed_destinations_keep_previous_files() {
    let root = tempfile::tempdir().unwrap();
    let document = ChartDocument::empty("Fresh / chart", 173.).unwrap();
    let export = prepare_project_export(&document, &ProjectOptions::default(), None, None).unwrap();
    assert_eq!(export.name, "Fresh _ chart.pez");
    let destination = root.path().join("existing");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("keep"), b"unchanged").unwrap();
    assert!(export.write_to(&destination).is_err());
    assert_eq!(fs::read(destination.join("keep")).unwrap(), b"unchanged");
    let imported = import_project(
        export.path(),
        &root.path().join("projects"),
        "Fresh.pez",
        true,
    )
    .unwrap();
    assert_eq!(
        load_chart(&imported.charts[imported.suggested.unwrap()].path)
            .unwrap()
            .as_json(),
        document.as_json()
    );
}
#[test]
fn missing_or_escaping_resources_and_symlinks_cannot_be_exported() {
    let root = tempfile::tempdir().unwrap();
    let document = ChartDocument::empty("Check", 120.).unwrap();
    for music in ["missing.ogg", "../outside.ogg", "/outside.ogg"] {
        let options = ProjectOptions {
            music: Some(music.into()),
            ..Default::default()
        };
        assert!(prepare_project_export(&document, &options, Some(root.path()), None).is_err());
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc/passwd", root.path().join("link")).unwrap();
        assert!(
            prepare_project_export(
                &document,
                &ProjectOptions::default(),
                Some(root.path()),
                None
            )
            .is_err()
        );
    }
}
