use phistudio_core::*;
use std::{
    fs::File,
    io::{Read, Write},
    sync::atomic::{AtomicUsize, Ordering},
};
use zip::{ZipWriter, write::SimpleFileOptions};

fn archive(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}
#[test]
fn project_entries_roundtrip_binary_bytes_nested_paths_and_unknown_chart_data() {
    let root = tempfile::tempdir().unwrap();
    let mut value = ChartDocument::empty("Bundle", 120.)
        .unwrap()
        .as_json()
        .clone();
    value["future"] = serde_json::json!({"keep":[null,"unknown"]});
    let chart = root.path().join("chart.json");
    std::fs::write(&chart, serde_json::to_vec(&value).unwrap()).unwrap();
    let binary = root.path().join("binary");
    let bytes: Vec<_> = (0..300_000).map(|i| (i % 251) as u8).collect();
    std::fs::write(&binary, &bytes).unwrap();
    let target = root.path().join("project.zip");
    let mut file = File::create(&target).unwrap();
    write_project_archive(
        &[
            ("charts/main.json".into(), chart),
            ("textures/雪/data.bin".into(), binary),
        ],
        &mut file,
        || true,
    )
    .unwrap();
    let mut archive = ProjectArchive::open(&target).unwrap();
    assert_eq!(archive.entries().len(), 2);
    let mut decoded = vec![];
    archive.copy_entry(1, &mut decoded, || true).unwrap();
    assert_eq!(decoded, bytes);
    let imported = import_project(&target, &root.path().join("projects"), "Bundle", true).unwrap();
    assert_eq!(
        std::fs::read(imported.assets.join("textures/雪/data.bin")).unwrap(),
        bytes
    );
    assert_eq!(
        load_chart(&imported.charts[imported.suggested.unwrap()].path)
            .unwrap()
            .as_json()["future"],
        value["future"]
    );
}

#[test]
fn archive_validation_rejects_escape_aliases_links_prefix_collisions_and_oversized_metadata() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("bad.zip");
    for entries in [
        vec![("../escape", b"x".as_slice())],
        vec![("a/b", b"x".as_slice()), ("a\\b", b"y".as_slice())],
        vec![("a", b"x".as_slice()), ("a/b", b"y".as_slice())],
    ] {
        archive(&path, &entries);
        assert!(ProjectArchive::open(&path).is_err());
    }
    let mut zip = ZipWriter::new(File::create(&path).unwrap());
    zip.add_symlink("link", "../escape", SimpleFileOptions::default())
        .unwrap();
    zip.finish().unwrap();
    assert!(ProjectArchive::open(&path).is_err());
    archive(&path, &[("large", b"x")]);
    let mut bytes = std::fs::read(&path).unwrap();
    let central = bytes.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
    bytes[central + 24..central + 28]
        .copy_from_slice(&((MAX_PROJECT_FILE_BYTES + 1) as u32).to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    assert!(ProjectArchive::open(&path).is_err());
    assert!(!root.path().join("escape").exists());
}

#[test]
fn cancellation_and_crc_failures_stop_entry_copy_without_allocating_whole_resources() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.zip");
    let bytes = vec![123; 500_000];
    archive(&path, &[("data.bin", &bytes)]);
    let mut reader = ProjectArchive::open(&path).unwrap();
    let calls = AtomicUsize::new(0);
    let mut output = vec![];
    assert!(
        reader
            .copy_entry(0, &mut output, || calls.fetch_add(1, Ordering::SeqCst) < 2)
            .is_err()
    );
    assert!(output.len() < bytes.len());
    let mut zip = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
    let offset = zip.by_index(0).unwrap().data_start().unwrap() as usize;
    drop(zip);
    let mut data = std::fs::read(&path).unwrap();
    data[offset] ^= 1;
    std::fs::write(&path, data).unwrap();
    let mut reader = ProjectArchive::open(&path).unwrap();
    assert!(reader.copy_entry(0, &mut vec![], || true).is_err());
}

#[test]
fn packing_rejects_bad_names_and_size_totals_and_native_conversion_honors_cancellation() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("data");
    std::fs::write(&source, b"data").unwrap();
    let mut target = tempfile::tempfile().unwrap();
    for files in [
        vec![("../outside".into(), source.clone())],
        vec![("a".into(), source.clone()), ("a/b".into(), source.clone())],
    ] {
        assert!(write_project_archive(&files, &mut target, || true).is_err());
    }
    File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_len(MAX_PROJECT_FILE_BYTES + 1)
        .unwrap();
    assert!(
        write_project_archive(&[("data".into(), source.clone())], &mut target, || true).is_err()
    );
    File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_len(MAX_PROJECT_FILE_BYTES)
        .unwrap();
    assert!(
        write_project_archive(
            &[
                ("a".into(), source.clone()),
                ("b".into(), source.clone()),
                ("c".into(), source.clone())
            ],
            &mut target,
            || true
        )
        .is_err()
    );
    let document = ChartDocument::empty("Cancel", 120.).unwrap();
    assert!(
        prepare_project_export_checked(&document, &Default::default(), None, None, || false)
            .is_err()
    );
    let package = prepare_project_export(&document, &Default::default(), None, None).unwrap();
    let projects = root.path().join("projects");
    assert!(
        import_project_checked(package.path(), &projects, "Cancelled", true, || false).is_err()
    );
    assert!(!projects.exists());
    let calls = AtomicUsize::new(0);
    assert!(
        import_project_checked(package.path(), &projects, "Cancelled", true, || calls
            .fetch_add(1, Ordering::SeqCst)
            < 3)
        .is_err()
    );
    assert_eq!(std::fs::read_dir(projects).unwrap().count(), 0);
    let mut magic = [0; 4];
    let mut input = File::open(package.path()).unwrap();
    input.read_exact(&mut magic).unwrap();
    assert_eq!(&magic, b"PK\x03\x04");
}
