//! Opt-in, reproducible read-path measurements; correctness is tested separately.
use phistudio_core::{ChartDocument, Note};
use serde_json::json;
use std::{hint::black_box, time::Instant};

#[test]
#[ignore = "manual read-path timing"]
fn read_performance() {
    let mut value = ChartDocument::empty("Read benchmark", 120.)
        .unwrap()
        .as_json()
        .clone();
    let mut note = json!(Note::default());
    note["futureExtension"] = json!({"text":"x".repeat(512),"curve":[{"x":1,"y":2},{"x":3,"y":4}]});
    value["judgeLineList"][0]["notes"] = json!((0..4000).map(|_| note.clone()).collect::<Vec<_>>());
    let doc = ChartDocument::from_value(value).unwrap();
    let start = Instant::now();
    for _ in 0..20 {
        black_box(doc.notes(0));
    }
    eprintln!(
        "PERF note_reads notes=4000 reads=20 ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.
    );
}
