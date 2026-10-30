use phistudio_core::{ChartFormat, decode_chart, pec_height_signature};
use serde_json::Value;
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/conversion");
fn equivalent(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!(
                (a - b).abs() <= 1e-10 * a.abs().max(b.abs()).max(1.),
                "{path}: {a} != {b}"
            );
        }
        (Value::Object(a), Value::Object(b)) => {
            let keys_a: std::collections::BTreeSet<_> = a.keys().collect();
            let keys_b: std::collections::BTreeSet<_> = b.keys().collect();
            assert_eq!(keys_a, keys_b, "{path}");
            for (key, value) in a {
                equivalent(value, &b[key], &format!("{path}.{key}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                equivalent(a, b, &format!("{path}[{i}]"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn complete_conversion_matches_the_original_java_converter_outputs() {
    for (name, extension, format) in [
        ("official-v1", "json", ChartFormat::OfficialPhigros),
        ("official-v3", "json", ChartFormat::OfficialPhigros),
        ("pec-minimum", "pec", ChartFormat::LegacyPec),
        ("pec-overlap", "pec", ChartFormat::LegacyPec),
    ] {
        let source = std::fs::read_to_string(format!("{FIXTURES}/{name}.{extension}")).unwrap();
        let expected: Value = serde_json::from_str(
            &std::fs::read_to_string(format!("{FIXTURES}/{name}.expected.rpe.json")).unwrap(),
        )
        .unwrap();
        let converted = decode_chart(&source).unwrap();
        assert_eq!(converted.format, format);
        equivalent(converted.document.as_json(), &expected, name);
        let reopened = decode_chart(&converted.document.to_json().unwrap()).unwrap();
        assert_eq!(reopened.format, ChartFormat::Rpe);
        assert_eq!(reopened.document.as_json(), converted.document.as_json());
    }
}
#[test]
fn rpe_detection_preserves_the_authored_tree_and_accepts_a_utf8_bom() {
    let mut root: Value = serde_json::from_str(include_str!(
        "../../../assets/charts/phistudio-demo.rpe.json"
    ))
    .unwrap();
    root["futureExtension"] = serde_json::json!({"keep":[17,null,{"raw":"untouched"}]});
    let converted = decode_chart(&format!(
        "\u{feff}{}",
        serde_json::to_string(&root).unwrap()
    ))
    .unwrap();
    assert_eq!(converted.format, ChartFormat::Rpe);
    assert_eq!(converted.document.as_json(), &root);
}
#[test]
fn malformed_conversions_return_actionable_errors_without_panicking() {
    for text in [
        "",
        "not a chart",
        "NaN\nbp 0 120",
        "150\nbp 0 0",
        "150\n# 1",
        "150\nn1 0 0 0 1 0 &",
        "150\nn1 0 0 0 1 2",
        "150\ncp 2147483647 0 0 0",
        "150\nunknown 0",
    ] {
        assert!(decode_chart(text).is_err(), "{text}");
    }
    for version in [0, 2, 4] {
        let value = serde_json::json!({"formatVersion":version,"judgeLineList":[{"bpm":120}]});
        let error = decode_chart(&value.to_string()).err().unwrap().to_string();
        assert!(error.contains("formatVersion"), "{error}");
    }
    assert!(decode_chart("{\"formatVersion\":3,\"judgeLineList\":[]}").is_err());
}
#[test]
fn pec_floor_provenance_survives_position_edits_and_expires_after_timing_edits() {
    let source = std::fs::read_to_string(format!("{FIXTURES}/pec-overlap.pec")).unwrap();
    let mut root = decode_chart(&source).unwrap().document.as_json().clone();
    let expected = root["META"]["phiPecHeights"]["signature"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        pec_height_signature(&root).as_deref(),
        Some(expected.as_str())
    );
    root["judgeLineList"][0]["notes"][0]["positionX"] = serde_json::json!(120.);
    assert_eq!(
        pec_height_signature(&root).as_deref(),
        Some(expected.as_str())
    );
    root["judgeLineList"][0]["notes"][0]["startTime"] = serde_json::json!([1, 0, 1]);
    assert_ne!(
        pec_height_signature(&root).as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn rpe_flags_and_alpha_match_prpr_ranges_and_survive_unrelated_edits() {
    let mut root: Value = serde_json::from_str(include_str!(
        "../../../assets/charts/phistudio-demo.rpe.json"
    ))
    .unwrap();
    root["judgeLineList"][0]["notes"][0]["above"] = serde_json::json!(2);
    root["judgeLineList"][0]["notes"][0]["isFake"] = serde_json::json!(7);
    root["judgeLineList"][0]["notes"][0]["alpha"] = serde_json::json!(256);
    let document = decode_chart(&root.to_string()).unwrap().document;
    assert_eq!(document.as_json(), &root);
    let mut note = document.note(0, 0).unwrap();
    assert_eq!((note.above, note.is_fake, note.alpha), (2, 7, 256));
    note.x += 10.;
    let mut editor = phistudio_core::Editor::new(document);
    editor
        .execute(phistudio_core::Command::UpdateNote {
            id: phistudio_core::NoteId { line: 0, index: 0 },
            note,
        })
        .unwrap();
    let edited = &editor.document().as_json()["judgeLineList"][0]["notes"][0];
    assert_eq!(edited["above"], 2);
    assert_eq!(edited["isFake"], 7);
    assert_eq!(edited["alpha"], 256);
    editor.undo();
    assert_eq!(editor.document().as_json(), &root);
}
