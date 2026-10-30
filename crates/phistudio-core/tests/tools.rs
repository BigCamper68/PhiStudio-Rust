#[path = "tools/events.rs"]
mod events;
#[path = "tools/generation.rs"]
mod generation;
#[path = "tools/selection.rs"]
mod selection;
use phistudio_core::{
    Beat, ChartDocument, Command, Editor, EventId, EventType, LineEvent, Note, NoteId, NoteKind,
    tools::{self, ChartClipboard, Selection, Transform, ValueField, ValueMode},
};

use serde_json::json;

fn beat(n: i32) -> Beat {
    Beat::new(n, 0, 1).unwrap()
}

fn fixture() -> Editor {
    let mut root = ChartDocument::empty("Tools", 120.)
        .unwrap()
        .as_json()
        .clone();
    root["judgeLineList"][0]["notes"] = json!([
        {"type":1,"startTime":[1,0,1],"endTime":[1,0,1],"positionX":-135.,"hitsound":"custom.wav","controls":{"future":[1,2,3]},"above":2,"isFake":7,"alpha":500},
        {"type":2,"startTime":[3,0,1],"endTime":[4,0,1],"positionX":135.,"future":"hold"},
        {"type":4,"startTime":[5,0,1],"endTime":[5,0,1],"positionX":270.}
    ]);
    root["judgeLineList"][0]["numOfNotes"] = json!(47);
    let ev = |a, b, v| json!({"startTime":[a,0,1],"endTime":[b,0,1],"start":v,"end":v,"easingType":1,"custom":"keep"});
    root["judgeLineList"][0]["eventLayers"] = json!([null,{"rotateEvents":[ev(6,7,60.),ev(2,3,20.),ev(4,5,40.)],"future":{"keep":true}},null]);
    Editor::new(ChartDocument::from_value(root).unwrap())
}

fn notes(indices: &[usize]) -> Selection {
    Selection {
        notes: indices
            .iter()
            .map(|index| NoteId {
                line: 0,
                index: *index,
            })
            .collect(),
        events: vec![],
    }
}

fn events(indices: &[usize]) -> Selection {
    Selection {
        notes: vec![],
        events: indices
            .iter()
            .map(|index| EventId {
                line: 0,
                layer: 1,
                kind: EventType::Rotate,
                index: *index,
            })
            .collect(),
    }
}

fn apply(editor: &mut Editor, edit: tools::ToolEdit) -> Selection {
    edit.apply(editor).unwrap().unwrap()
}

fn pattern() -> tools::NotePattern {
    tools::NotePattern {
        start: beat(2),
        end: beat(4),
        step: Beat::new(0, 1, 3).unwrap(),
        hold_length: Beat::new(0, 1, 6).unwrap(),
        kind: NoteKind::Hold,
        from_x: -450.,
        to_x: 450.,
        pattern: tools::Pattern::Line,
        cycles: 1.,
        control: [-200., 200.],
        seed: 42,
        lanes: 11,
        include_endpoints: true,
    }
}
