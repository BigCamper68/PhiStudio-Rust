use phistudio_core::{ChartDocument, Command, Editor};
use serde_json::{Value, json};

fn chart() -> ChartDocument {
    let mut raw = ChartDocument::empty("Line operations", 120.)
        .unwrap()
        .as_json()
        .clone();
    let line = raw["judgeLineList"][0].clone();
    raw["judgeLineList"] = Value::Array(vec![line; 4]);
    for i in 0..4 {
        raw["judgeLineList"][i]["Name"] = json!(format!("Source {i}"));
        raw["judgeLineList"][i]["custom"] = json!({"kept": [i, null]});
    }
    raw["judgeLineList"][1]["father"] = json!(0);
    raw["judgeLineList"][2]["father"] = json!(1);
    raw["judgeLineList"][1]["anchor"] = json!([-0.25, 1.5]);
    raw["judgeLineList"][1]["notes"] = Value::Null;
    raw["judgeLineList"][1]["numOfNotes"] = json!(77);
    raw["judgeLineList"][3]
        .as_object_mut()
        .unwrap()
        .remove("father");
    raw["effects"] = json!([{"line":1,"shader":"noise","custom":[3]}, {"line":2,"shader":"blur"}]);
    raw["shaderEvents"] = json!([{"line":1,"unknown":true}]);
    raw["META"]["effects"] = json!([{"line":1,"nested":[null]}]);
    ChartDocument::from_value(raw).unwrap()
}

#[test]
fn creating_n_lines_is_one_atomic_undo_and_invalid_counts_preserve_redo() {
    let mut editor = Editor::new(chart());
    let raw = editor.document().as_json().clone();
    assert!(editor.execute(Command::AddLines { count: 12 }).unwrap());
    assert_eq!(editor.document().line_count(), 16);
    assert_eq!(editor.history_cursor(), 1);
    assert_eq!(
        &editor.document().as_json()["judgeLineList"]
            .as_array()
            .unwrap()[..4],
        raw["judgeLineList"].as_array().unwrap()
    );
    assert!(editor.undo());
    assert_eq!(editor.document().as_json(), &raw);
    let revision = editor.revision();
    for count in [0, 10_001, usize::MAX] {
        assert!(editor.execute(Command::AddLines { count }).is_err());
        assert_eq!(editor.revision(), revision);
        assert!(!editor.is_dirty());
        assert!(editor.can_redo());
    }
    assert!(editor.redo());
    assert_eq!(editor.document().line_count(), 16);
    ChartDocument::from_value(editor.document().as_json().clone()).unwrap();
}

#[test]
fn duplicate_retains_entire_raw_line_and_duplicates_its_owned_effects() {
    let mut editor = Editor::new(chart());
    let raw = editor.document().as_json().clone();
    assert!(editor.execute(Command::DuplicateLine(1)).unwrap());
    let mut expected = raw["judgeLineList"][1].clone();
    expected["Name"] = json!("Source 1 (copy)");
    assert_eq!(editor.document().as_json()["judgeLineList"][4], expected);
    for effects in [
        &editor.document().as_json()["effects"],
        &editor.document().as_json()["shaderEvents"],
        &editor.document().as_json()["META"]["effects"],
    ] {
        assert!(effects.as_array().unwrap().iter().any(|v| v["line"] == 4));
        assert!(effects.as_array().unwrap().iter().any(|v| v["line"] == 1));
    }
    let after = editor.document().as_json().clone();
    editor.undo();
    assert_eq!(editor.document().as_json(), &raw);
    editor.redo();
    assert_eq!(editor.document().as_json(), &after);
    editor
        .execute(Command::RenameLine {
            line: 4,
            name: "Independent".into(),
        })
        .unwrap();
    assert_eq!(
        editor.document().as_json()["judgeLineList"][1],
        raw["judgeLineList"][1]
    );
}

#[test]
fn moving_in_both_directions_retains_parent_identity_effect_ownership_and_exact_undo() {
    for (from, to, order) in [
        (0, 3, vec![1, 2, 3, 0]),
        (2, 0, vec![2, 0, 1, 3]),
        (1, 2, vec![0, 2, 1, 3]),
    ] {
        let mut editor = Editor::new(chart());
        let raw = editor.document().as_json().clone();
        editor.execute(Command::MoveLine { from, to }).unwrap();
        let after = editor.document().as_json();
        for (new, old) in order.iter().copied().enumerate() {
            assert_eq!(
                after["judgeLineList"][new]["custom"],
                raw["judgeLineList"][old]["custom"]
            );
            if let Some(parent) = raw["judgeLineList"][old]["father"].as_u64() {
                let expected = order.iter().position(|i| *i == parent as usize).unwrap();
                assert_eq!(after["judgeLineList"][new]["father"], expected);
            }
        }
        let owner = order.iter().position(|i| *i == 1).unwrap();
        assert_eq!(after["effects"][0]["line"], owner);
        assert_eq!(after["shaderEvents"][0]["line"], owner);
        assert_eq!(after["META"]["effects"][0]["line"], owner);
        ChartDocument::from_value(after.clone()).unwrap();
        let after = after.clone();
        editor.undo();
        assert_eq!(editor.document().as_json(), &raw);
        editor.redo();
        assert_eq!(editor.document().as_json(), &after);
    }
}

#[test]
fn deleting_a_line_removes_only_owned_effects_and_restores_children_and_raw_fields() {
    let mut editor = Editor::new(chart());
    let raw = editor.document().as_json().clone();
    editor.execute(Command::DeleteLine(1)).unwrap();
    assert_eq!(
        editor.document().as_json()["judgeLineList"][1]["father"],
        -1
    );
    assert_eq!(
        editor.document().as_json()["effects"],
        json!([{"line":1,"shader":"blur"}])
    );
    assert_eq!(editor.document().as_json()["META"]["effects"], json!([]));
    assert_eq!(editor.document().as_json()["shaderEvents"], json!([]));
    editor.undo();
    assert_eq!(editor.document().as_json(), &raw);
}

#[test]
fn anchors_support_outside_center_and_remain_absent_until_changed() {
    let mut editor = Editor::new(chart());
    let raw = editor.document().as_json().clone();
    let mut p = editor.document().line_properties(0).unwrap();
    assert_eq!(p.anchor, [0.5, 0.5]);
    p.name = "Renamed".into();
    editor
        .execute(Command::EditLine {
            index: 0,
            properties: p.clone(),
        })
        .unwrap();
    assert!(
        editor.document().as_json()["judgeLineList"][0]
            .get("anchor")
            .is_none()
    );
    p.anchor = [-0.25, 1.5];
    editor
        .execute(Command::EditLine {
            index: 0,
            properties: p.clone(),
        })
        .unwrap();
    assert_eq!(
        editor.document().as_json()["judgeLineList"][0]["anchor"],
        json!([-0.25, 1.5])
    );
    editor.undo();
    editor.undo();
    assert_eq!(editor.document().as_json(), &raw);
    p.anchor = [f64::NAN, 0.];
    assert!(
        editor
            .execute(Command::EditLine {
                index: 0,
                properties: p
            })
            .is_err()
    );
    assert!(editor.can_redo());
    assert!(!editor.is_dirty());
}

#[test]
fn legacy_implicit_shader_owners_stay_attached_when_line_count_or_order_changes() {
    let mut raw = chart().as_json().clone();
    raw["effects"] =
        json!([{"shader":"pixel","unknown":[1]}, {"shader":"/circleBlur_pr.glsl","clone":true}]);
    let mut editor = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
    editor
        .execute(Command::AddLine {
            name: "Single".into(),
        })
        .unwrap();
    assert_eq!(editor.document().as_json()["effects"][0]["line"], 2);
    assert_eq!(editor.document().as_json()["effects"][1]["line"], 3);
    editor.undo();
    assert_eq!(editor.document().as_json(), &raw);
    editor.execute(Command::AddLines { count: 4 }).unwrap();
    assert_eq!(editor.document().as_json()["effects"][0]["line"], 2);
    assert_eq!(editor.document().as_json()["effects"][1], raw["effects"][1]);
    editor.undo();
    assert_eq!(editor.document().as_json(), &raw);
    editor
        .execute(Command::MoveLine { from: 2, to: 0 })
        .unwrap();
    assert_eq!(editor.document().as_json()["effects"][0]["line"], 0);
    editor.undo();
    assert_eq!(editor.document().as_json(), &raw);
    editor.execute(Command::DuplicateLine(2)).unwrap();
    assert_eq!(editor.document().as_json()["effects"][1]["line"], 4);
    editor.undo();
    assert_eq!(editor.document().as_json(), &raw);
}
