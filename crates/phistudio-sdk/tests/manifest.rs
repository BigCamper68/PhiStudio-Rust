use phistudio_sdk::*;
use serde_json::json;

fn manifest() -> Manifest {
    serde_json::from_value(
        json!({"api":1,"id":"example.test","name":"Test","version":"1.0.0",
        "backend":{"kind":"wasm","entry":"plugin.wasm"}}),
    )
    .unwrap()
}

#[test]
fn manifests_reject_unsupported_api_invalid_identifiers_and_self_dependencies() {
    let mut m = manifest();
    m.validate().unwrap();
    for id in [
        "",
        ".a",
        "a.",
        "a..b",
        "A",
        "../../evil",
        "1test",
        "тест",
        "a:b",
        "host",
    ] {
        m.id = id.into();
        assert!(m.validate().is_err(), "{id}");
    }
    m = manifest();
    m.api = 2;
    assert!(m.validate().is_err());
    m = manifest();
    m.dependencies.insert(m.id.clone(), "*".parse().unwrap());
    assert!(m.validate().is_err());
}

#[test]
fn package_paths_and_setting_defaults_are_validated_before_loading() {
    for path in [
        "",
        "/plugin.wasm",
        "../plugin.wasm",
        "a/../b",
        "a//b",
        "a/./b",
        "C:/b",
        "a\\b",
        "a\0b",
    ] {
        assert!(validate_package_path(path).is_err(), "{path}");
    }
    validate_package_path("nested/插件.wasm").unwrap();
    let mut m = manifest();
    m.settings.insert(
        "number".into(),
        Setting::Number {
            title: "Number".into(),
            default: 6.,
            min: 0.,
            max: 5.,
        },
    );
    assert!(m.validate().is_err());
    m.settings.insert(
        "number".into(),
        Setting::Number {
            title: "Number".into(),
            default: 3.,
            min: 0.,
            max: 5.,
        },
    );
    m.validate().unwrap();
    assert!(!m.settings["number"].accepts(&json!("3")));
    assert!(!m.settings["number"].accepts(&json!(-1)));
    assert!(m.settings["number"].accepts(&json!(5)));
}
