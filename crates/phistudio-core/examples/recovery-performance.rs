//! Measure real recovery reads in a private temporary workspace; verify exact JSON.
use phistudio_core::{ChartDocument, Note, RecoverySnapshot, load_recovery, save_recovery};
use std::time::Instant;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let notes: usize = std::env::args()
        .nth(1)
        .map(|n| n.parse())
        .transpose()?
        .unwrap_or(500);
    let root = tempfile::tempdir()?;
    let mut value = ChartDocument::empty("Recovery benchmark", 120.)?
        .as_json()
        .clone();
    value["judgeLineList"][0]["notes"] = serde_json::Value::Array(
        (0..notes)
            .map(|_| serde_json::to_value(Note::default()).unwrap())
            .collect(),
    );
    value["judgeLineList"][0]["numOfNotes"] = serde_json::json!(notes);
    value["privateExtension"] = serde_json::json!({"values":[null,true,"中文 😀",1.2345]});
    let document = ChartDocument::from_value(value)?;
    let snapshot = RecoverySnapshot {
        document,
        location: None,
        beat: 12.25,
        line: 0,
    };
    save_recovery(root.path(), &snapshot)?;
    let bytes = std::fs::metadata(root.path().join("recovery/session.json"))?.len();
    let begin = Instant::now();
    let restored = load_recovery(root.path())?.expect("Existing recovery");
    let ms = begin.elapsed().as_secs_f64() * 1000.;
    assert_eq!(restored.document.as_json(), snapshot.document.as_json());
    assert_eq!(
        (restored.beat, restored.line),
        (snapshot.beat, snapshot.line)
    );
    println!(
        "{}",
        serde_json::json!({"notes":notes,"bytes":bytes,"read_ms":ms,"json_and_position_equal":true})
    );
    Ok(())
}
