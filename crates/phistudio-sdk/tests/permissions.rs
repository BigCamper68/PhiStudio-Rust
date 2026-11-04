use phistudio_sdk::*;
use serde_json::json;

fn manifest() -> Manifest {
    serde_json::from_value(
        json!({"api":1,"id":"example.permissions","name":"Permissions",
        "version":"1.0.0","backend":{"kind":"wasm","entry":"plugin.wasm"},
        "capabilities":["ui","network"],"network":{"origins":["https://api.example.com"]}}),
    )
    .unwrap()
}

#[test]
fn old_manifests_keep_the_original_wire_shape() {
    let mut m = manifest();
    m.network = Default::default();
    let value = serde_json::to_value(&m).unwrap();
    assert!(value.get("network").is_none());
    assert!(value.get("minimum_editor").is_none());
    m.minimum_editor = ">=9999.0.0".parse().unwrap();
    assert!(m.validate().unwrap_err().contains("requires editor"));
}

#[test]
fn origins_reject_credentials_paths_ambiguous_addresses_and_implicit_cleartext() {
    for origin in [
        "https://example.com/secret",
        "https://example.com/",
        "https://user@example.com",
        "https://example.com?token=secret",
        "https://example.com#secret",
        "file:///tmp/x",
        "https://example.com:443",
        "https://EXAMPLE.com",
        "https://127.1",
        "http://localhost",
    ] {
        let mut m = manifest();
        m.network.origins = [origin.into()].into();
        assert!(m.validate().is_err(), "{origin}");
    }
    let mut m = manifest();
    m.network.local_network = true;
    m.network.origins = ["http://localhost:8080".into(), "wss://example.com".into()].into();
    m.validate().unwrap();
    m.capabilities.remove(&Capability::Network);
    assert!(m.validate().is_err());
}

#[test]
fn decisions_preserve_denials_and_require_review_for_expanded_access() {
    let m = manifest();
    let requested = PermissionRequest::from_manifest(&m);
    let decision = PermissionDecision {
        requested: requested.clone(),
        granted: requested,
    };
    decision.validate(&m).unwrap();
    let mut changed = m.clone();
    changed
        .network
        .origins
        .insert("https://other.example.com".into());
    assert!(decision.validate(&changed).is_err());
    changed = m.clone();
    changed.network.local_network = true;
    assert!(decision.validate(&changed).is_err());
    changed = m.clone();
    changed.capabilities.insert(Capability::DocumentEdit);
    assert!(decision.validate(&changed).is_err());
    changed = m.clone();
    changed.backend = Backend::Native {
        entry: "plugin.so".into(),
    };
    assert!(decision.validate(&changed).is_err());
    changed = m.clone();
    changed.network = Default::default();
    decision.validate(&changed).unwrap();
    assert!(decision.effective_network(&changed).is_empty());
    let denied = PermissionDecision {
        requested: PermissionRequest::from_manifest(&m),
        granted: Default::default(),
    };
    denied.validate(&m).unwrap();
    assert!(denied.effective(&m).is_empty());
    assert!(denied.effective_network(&m).is_empty());
}
