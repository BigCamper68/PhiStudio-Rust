use phistudio_core::tool_presets::Library;
use serde_json::json;
#[test]
fn named_parameters_history_survive_restart_and_failed_save_preserves_library() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path();
    let mut library = Library::load(path).unwrap();
    library
        .save(path, "Плавная дуга 中文", json!({"scale":2}))
        .unwrap();
    let id = library.presets[0].id;
    library
        .save(path, "Плавная дуга 中文", json!({"scale":3}))
        .unwrap();
    assert_eq!(library.presets.len(), 1);
    assert_eq!(library.presets[0].id, id);
    for i in 0..21 {
        library.remember(path, json!({"value":i})).unwrap();
    }
    library.remember(path, json!({"value":9})).unwrap();
    let library = Library::load(path).unwrap();
    assert_eq!(library.presets[0].parameters["scale"], 3);
    assert_eq!(library.recent.len(), 16);
    assert_eq!(library.recent[0]["value"], 9);
    let before = std::fs::read(path.join("tool-configurations.json")).unwrap();
    let mut library = library;
    assert!(library.save(path, "", json!({"value":1})).is_err());
    assert!(library.remember(path, json!([1, 2])).is_err());
    assert_eq!(
        before,
        std::fs::read(path.join("tool-configurations.json")).unwrap()
    );
    library.remove(path, id).unwrap();
    assert!(Library::load(path).unwrap().presets.is_empty());
}
