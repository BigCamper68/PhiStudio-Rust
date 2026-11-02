use phistudio_core::{ChartDocument, decode_chart, import_project, load_chart};
use std::{fs::File, io::Write, path::Path};
use zip::{ZipWriter, write::SimpleFileOptions};

#[test]
fn chart_json_accepts_exactly_256_mib_and_rejects_one_extra_byte() {
    let doc = ChartDocument::empty("Large chart", 120.).unwrap();
    let mut source = serde_json::to_string(doc.as_json()).unwrap();
    let limit = 256 * 1024 * 1024;
    assert_eq!(ChartDocument::MAX_JSON_BYTES, limit);
    source.extend(std::iter::repeat_n(' ', limit + 1 - source.len()));
    assert!(
        ChartDocument::parse(&source)
            .unwrap_err()
            .to_string()
            .contains("256 MiB")
    );
    assert!(decode_chart(&source).is_err());
    source.truncate(limit);
    assert_eq!(
        ChartDocument::parse(&source).unwrap().as_json(),
        doc.as_json()
    );
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("large.json");
    std::fs::write(&path, &source).unwrap();
    drop(source);
    assert_eq!(load_chart(&path).unwrap().as_json(), doc.as_json());
    let project =
        import_project(&path, &root.path().join("projects"), "large.json", false).unwrap();
    assert_eq!(project.charts.len(), 1);
}
fn padded_archive(path: &Path, bytes: usize) {
    let json = serde_json::to_vec(
        ChartDocument::empty("Archive chart", 120.)
            .unwrap()
            .as_json(),
    )
    .unwrap();
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    zip.start_file(
        "chart.json",
        SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(1)),
    )
    .unwrap();
    zip.write_all(&json).unwrap();
    let spaces = [b' '; 64 * 1024];
    let mut left = bytes - json.len();
    while left > 0 {
        let count = left.min(spaces.len());
        zip.write_all(&spaces[..count]).unwrap();
        left -= count;
    }
    zip.finish().unwrap();
}
#[test]
fn zip_chart_above_old_limit_imports_and_oversized_entry_is_rejected_without_publication() {
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("chart.zip");
    let projects = root.path().join("projects");
    padded_archive(&archive, 128 * 1024 * 1024 + 1);
    let project = import_project(&archive, &projects, "chart.zip", true).unwrap();
    assert_eq!(project.charts.len(), 1);
    let count = std::fs::read_dir(&projects).unwrap().count();
    padded_archive(&archive, 256 * 1024 * 1024 + 1);
    let error = import_project(&archive, &projects, "chart.zip", true).unwrap_err();
    assert!(error.to_string().contains("ZIP entry exceeds 256 MiB"));
    assert_eq!(std::fs::read_dir(&projects).unwrap().count(), count);
}
