use phistudio_plugins::*;
use serde_json::json;

fn setup(root: &std::path::Path) {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(
        root.join(MANIFEST_NAME),
        serde_json::to_vec(
            &json!({"api":1,"id":"package.test","name":"Test","version":"1.0.0",
        "backend":{"kind":"wasm","entry":"plugin.wasm"}}),
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn discovery_reports_a_broken_package_without_hiding_the_others() {
    let root = tempfile::tempdir().unwrap();
    setup(&root.path().join("valid"));
    std::fs::create_dir(root.path().join("broken")).unwrap();
    let packages = discover(root.path()).unwrap();
    assert_eq!(packages.len(), 2);
    assert!(packages[0].package.is_err());
    assert!(packages[1].package.is_ok());
    assert!(discover(&root.path().join("missing")).unwrap().is_empty());
}

#[test]
fn assets_enforce_limits_and_prevent_traversal_and_symlink_escape() {
    let root = tempfile::tempdir().unwrap();
    setup(root.path());
    std::fs::write(root.path().join("asset.txt"), b"12345").unwrap();
    let package = Package::open(root.path()).unwrap();
    assert_eq!(package.asset("asset.txt", 5).unwrap(), b"12345");
    assert!(package.asset("asset.txt", 4).is_err());
    assert!(package.asset("../asset.txt", 100).is_err());
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("outside.txt"), b"outside").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("outside.txt"),
            root.path().join("escape.txt"),
        )
        .unwrap();
        assert!(package.asset("escape.txt", 100).is_err());
    }
}
