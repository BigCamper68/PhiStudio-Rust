use phistudio_plugins::*;
use serde_json::json;
use std::io::Write;
fn manifest() -> Vec<u8> {
    serde_json::to_vec(&json!({"api":1,"id":"test.install","name":"Install test","version":"1.0.0","backend":{"kind":"wasm","entry":"plugin.wasm"}})).unwrap()
}
fn archive(path: &std::path::Path, extra: Option<&str>, link: bool) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for (name, data) in [
        ("manifest.json", manifest()),
        ("plugin.wasm", b"fixture".to_vec()),
    ] {
        zip.start_file(name, options).unwrap();
        zip.write_all(&data).unwrap();
    }
    if let Some(path) = extra {
        zip.start_file(path, options).unwrap();
        zip.write_all(b"outside").unwrap();
    }
    if link {
        zip.add_symlink("link", "../outside", options).unwrap();
    }
    zip.finish().unwrap();
}
#[test]
fn validated_archives_publish_atomically_and_uninstall_retains_a_recoverable_copy() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("plugin.zip");
    let library = root.path().join("library");
    archive(&source, None, false);
    let (package, ()) = install_package(&source, &library, |p| {
        assert_eq!(p.asset("plugin.wasm", 100).unwrap(), b"fixture");
        Ok(())
    })
    .unwrap();
    assert_eq!(discover(&library).unwrap().len(), 1);
    assert!(install_package(&source, &library, |_| Ok(())).is_err());
    assert_eq!(package.asset("plugin.wasm", 100).unwrap(), b"fixture");
    let retired = retire_package(&library, "test.install").unwrap();
    assert!(retired.join("plugin.wasm").exists());
    assert!(discover(&library).unwrap().is_empty());
}
#[test]
fn invalid_packages_links_and_traversal_never_become_installed() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("plugin.zip");
    let library = root.path().join("library");
    archive(&source, None, false);
    assert!(install_package(&source, &library, |_| Err::<(), _>("Bad module".into())).is_err());
    assert!(discover(&library).unwrap().is_empty());
    for path in ["../outside", "/outside", "folder/../../outside"] {
        archive(&source, Some(path), false);
        assert!(install_package(&source, &library, |_| Ok(())).is_err());
        assert!(discover(&library).unwrap().is_empty());
        assert!(!root.path().join("outside").exists());
    }
    archive(&source, None, true);
    assert!(install_package(&source, &library, |_| Ok(())).is_err());
    assert!(discover(&library).unwrap().is_empty());
}
