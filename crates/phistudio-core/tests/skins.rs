use phistudio_core::{ChartError, skins};
use std::{fs, io::Write};
fn archive(path: &std::path::Path, files: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, bytes) in files {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}
#[test]
fn skin_library_keeps_exact_files_supports_wrappers_and_deletes_only_the_selected_import() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("skins");
    let source = temp.path().join("user.zip");
    archive(
        &source,
        &[
            ("My Pack/info.yml", b"name: Example\nauthor: A"),
            ("My Pack/click.png", b"exact bytes"),
        ],
    );
    let original = fs::read(&source).unwrap();
    let load = |path: &std::path::Path| {
        assert_eq!(fs::read(path.join("click.png")).unwrap(), b"exact bytes");
        Ok(("Example".into(), "A".into(), "Description".into()))
    };
    let a = skins::import(&source, &root, load).unwrap();
    let b = skins::import(&source, &root, load).unwrap();
    assert_ne!(a.id, b.id);
    assert_eq!(skins::list(&root).unwrap().len(), 2);
    assert_eq!(fs::read(&source).unwrap(), original);
    skins::delete(&root, &a.id).unwrap();
    let entries = skins::list(&root).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, b.id);
    assert!(skins::delete(&root, "../user.zip").is_err());
    assert_eq!(fs::read(&source).unwrap(), original);
}
#[test]
fn failed_or_escaping_skin_imports_are_not_published() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("skins");
    let source = temp.path().join("bad.zip");
    archive(&source, &[("../outside", b"bad"), ("info.yml", b"bad")]);
    assert!(skins::import(&source, &root, |_| Ok(("Bad".into(), "".into(), "".into()))).is_err());
    archive(&source, &[("info.yml", b"bad")]);
    assert!(
        skins::import(&source, &root, |_| Err(ChartError::Invalid(
            "Invalid atlas".into()
        )))
        .is_err()
    );
    assert_eq!(fs::read_dir(root).unwrap().count(), 0);
    assert!(!temp.path().join("outside").exists());
}
