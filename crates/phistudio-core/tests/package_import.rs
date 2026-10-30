use phistudio_core::{import_project, load_chart};
use std::{
    fs::{self, File},
    io::Write,
};
use zip::{ZipWriter, write::SimpleFileOptions};
const DEMO: &str = include_str!("../../../assets/charts/phistudio-demo.rpe.json");
fn archive(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}
#[test]
fn manifest_selects_a_chart_and_resources_stay_in_the_private_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("pack.zip");
    archive(
        &source,
        &[
            ("chart.json", DEMO.as_bytes()),
            ("other.json", DEMO.as_bytes()),
            ("info.yml", b"chart: chart.json\nmusic: song.ogg\n"),
            ("textures/line.png", b"asset"),
        ],
    );
    let imported = import_project(&source, &tmp.path().join("projects"), "Example", true).unwrap();
    assert_eq!(imported.charts.len(), 2);
    assert_eq!(imported.suggested, Some(0));
    assert_eq!(
        fs::read(imported.assets.join("textures/line.png")).unwrap(),
        b"asset"
    );
    assert_eq!(
        load_chart(&imported.charts[0].path).unwrap().as_json(),
        load_chart(
            {
                let p = tmp.path().join("original.json");
                fs::write(&p, DEMO).unwrap();
                p
            }
            .as_path()
        )
        .unwrap()
        .as_json()
    );
}
#[test]
fn several_charts_require_selection_without_a_unique_manifest_hint() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("pack.zip");
    archive(
        &source,
        &[
            ("a.json", DEMO.as_bytes()),
            ("folder/b.json", DEMO.as_bytes()),
        ],
    );
    assert_eq!(
        import_project(&source, &tmp.path().join("projects"), "Example", true)
            .unwrap()
            .suggested,
        None
    );
}

#[test]
fn a_wrapping_directory_is_used_as_the_resource_root() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("pack.zip");
    archive(
        &source,
        &[
            ("My chart/chart.json", DEMO.as_bytes()),
            ("My chart/music.ogg", b"audio"),
            ("My chart/textures/line.png", b"texture"),
        ],
    );
    let project = import_project(&source, &tmp.path().join("projects"), "Wrapped", true).unwrap();
    assert_eq!(project.assets, project.workspace.join("source/My chart"));
    assert!(project.assets.join("textures/line.png").is_file());
}
#[test]
fn unsafe_or_invalid_import_never_publishes_a_partial_project() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("pack.zip");
    let projects = tmp.path().join("projects");
    for name in [
        "../escape.json",
        "/absolute.json",
        "folder/../../escape.json",
        "C:/drive.json",
        "..\\escape.json",
    ] {
        archive(&source, &[("chart.json", DEMO.as_bytes()), (name, b"bad")]);
        assert!(
            import_project(&source, &projects, "Example", true).is_err(),
            "{name}"
        );
        assert_eq!(fs::read_dir(&projects).unwrap().count(), 0);
    }
    fs::write(&source, "invalid").unwrap();
    assert!(import_project(&source, &projects, "Example", false).is_err());
    assert_eq!(fs::read_dir(projects).unwrap().count(), 0);
}
