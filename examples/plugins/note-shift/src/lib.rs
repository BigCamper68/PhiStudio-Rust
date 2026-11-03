use phistudio_sdk::*;
use serde_json::{Value, json};

struct NoteShift {
    offset: f64,
}
impl NoteShift {
    fn panel(&self) -> Effect {
        Effect::Panel {
            id: "shift".into(),
            widgets: vec![
                Widget::Number {
                    id: "offset".into(),
                    text: "Horizontal offset".into(),
                    value: self.offset,
                    min: -1350.,
                    max: 1350.,
                },
                Widget::Button {
                    id: "apply".into(),
                    text: "Move notes on active line".into(),
                    enabled: true,
                },
            ],
        }
    }
    fn move_notes(&self, context: &EditorContext, arguments: &Value) -> Response {
        let offset = arguments
            .get("offset")
            .and_then(Value::as_f64)
            .unwrap_or(self.offset);
        let Some(notes) = context
            .chart
            .as_ref()
            .and_then(|c| c["judgeLineList"][context.line]["notes"].as_array())
        else {
            return Response {
                effects: vec![Effect::Notify {
                    message: "Open a chart with notes first".into(),
                }],
                ..Default::default()
            };
        };
        if !offset.is_finite() || !(-1350. ..=1350.).contains(&offset) || notes.len() > 20_000 {
            return Response {
                effects: vec![Effect::Notify {
                    message: "Use a finite offset within ±1350 and at most 20,000 notes".into(),
                }],
                ..Default::default()
            };
        }
        let operations = notes
            .iter()
            .enumerate()
            .map(|(index, note)| PatchOperation::Add {
                path: format!("/judgeLineList/{}/notes/{index}/positionX", context.line),
                value: json!(note["positionX"].as_f64().unwrap_or(0.) + offset),
            })
            .collect();
        Response {
            effects: vec![Effect::Edit {
                transaction: Transaction {
                    version: context.version,
                    label: "Shift notes (plugin)".into(),
                    operations,
                },
            }],
            ..Default::default()
        }
    }
}
impl Plugin for NoteShift {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { settings, .. } => {
                self.offset = settings
                    .get("offset")
                    .and_then(Value::as_f64)
                    .unwrap_or(25.);
                Response {
                    contributions: vec![
                        Contribution::Command {
                            id: "shift".into(),
                            title: "Move notes on active line".into(),
                            shortcut: None,
                            snapshot: true,
                        },
                        Contribution::Panel {
                            id: "shift".into(),
                            title: "Note shift".into(),
                            location: PanelLocation::Window,
                            snapshot: true,
                        },
                    ],
                    effects: vec![self.panel()],
                    ..Default::default()
                }
            }
            Request::Command {
                id,
                arguments,
                context,
            } if id == "shift" => self.move_notes(context, arguments),
            Request::Ui { event, context } if event.widget == "apply" => {
                self.move_notes(context, &Value::Null)
            }
            Request::Ui { event, .. } if event.widget == "offset" => {
                if let Some(offset) = event
                    .value
                    .as_f64()
                    .filter(|n| (-1350. ..=1350.).contains(n))
                {
                    self.offset = offset;
                }
                Response {
                    effects: vec![self.panel()],
                    ..Default::default()
                }
            }
            Request::SettingsChanged { key, value } if key == "offset" => {
                self.offset = value.as_f64().unwrap_or(25.);
                Response {
                    effects: vec![self.panel()],
                    ..Default::default()
                }
            }
            _ => Response::default(),
        })
    }
}
phistudio_sdk::export_plugin!(|| NoteShift { offset: 25. });
phistudio_sdk::export_native_plugin!(|| NoteShift { offset: 25. });
