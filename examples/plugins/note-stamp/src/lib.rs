use phistudio_sdk::*;
use serde_json::{Value, json};

struct Stamp {
    gesture: u64,
    version: DocumentVersion,
    path: String,
    note: Value,
}
#[derive(Default)]
struct NoteStamp {
    pending: Option<Stamp>,
}
impl NoteStamp {
    fn input(&mut self, input: &InputBatch, context: &EditorContext) -> Response {
        if matches!(input.phase(), Some(InputPhase::Begin | InputPhase::Cancel)) {
            self.pending = None;
        }
        let Some(point) = input
            .events
            .last()
            .filter(|e| e.phase == InputPhase::End && !e.moved)
            .map(|e| &e.position)
        else {
            return Response::default();
        };
        let (Some(beat), Some(x)) = (point.beat, point.chart_x) else {
            return Response::default();
        };
        let path = format!("/judgeLineList/{}/notes", context.line);
        self.pending = Some(Stamp {
            gesture: input.gesture,
            version: context.version,
            path: path.clone(),
            note: json!({
                "type": 1, "startTime": beat, "endTime": beat, "positionX": x,
                "above": 1, "isFake": 0, "alpha": 255, "size": 1.0,
                "speed": 1.0, "visibleTime": 999999.0, "yOffset": 0.0
            }),
        });
        Response {
            effects: vec![Effect::Call {
                call: ServiceCall {
                    request_id: format!("stamp-{}", input.gesture),
                    service: ServiceRef {
                        provider: "host".into(),
                        id: "document".into(),
                        version: "^1".parse().expect("valid host service version"),
                    },
                    method: "inspect".into(),
                    arguments: json!(DocumentQuery {
                        version: context.version,
                        path,
                    }),
                },
            }],
            ..Default::default()
        }
    }
    fn reply(&mut self, reply: &ServiceReply, context: &EditorContext) -> Response {
        let Some(stamp) = &self.pending else {
            return Response::default();
        };
        if reply.request_id != format!("stamp-{}", stamp.gesture)
            || reply.service.provider != "host"
            || reply.service.id != "document"
            || context.version != stamp.version
        {
            return Response::default();
        }
        let append = match &reply.outcome {
            ServiceOutcome::Success { value }
                if value["version"] == json!(stamp.version) && value["kind"] == "array" =>
            {
                true
            }
            ServiceOutcome::Success { value }
                if value["version"] == json!(stamp.version) && value["kind"] == "null" =>
            {
                false
            }
            ServiceOutcome::Error { code, .. } if code == "path_not_found" => false,
            _ => {
                self.pending = None;
                return Response::default();
            }
        };
        let Some(stamp) = self.pending.take() else {
            return Response::default();
        };
        let (path, value) = if append {
            (format!("{}/-", stamp.path), stamp.note)
        } else {
            (stamp.path, json!([stamp.note]))
        };
        Response {
            effects: vec![Effect::Edit {
                transaction: Transaction {
                    version: stamp.version,
                    label: "Stamp note".into(),
                    operations: vec![PatchOperation::Add { path, value }],
                },
            }],
            ..Default::default()
        }
    }
}
impl Plugin for NoteStamp {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![
                    Contribution::Tool {
                        id: "stamp".into(),
                        title: "Stamp note".into(),
                        surfaces: vec![InputSurfaceKind::Notes],
                        buttons: default_input_buttons(),
                        gesture: ToolGesture::Tap,
                    },
                    Contribution::Command {
                        id: "select".into(),
                        title: "Select note stamp".into(),
                        shortcut: Some("Alt+S".into()),
                        snapshot: false,
                    },
                ],
                ..Default::default()
            },
            Request::Command { id, .. } if id == "select" => Response {
                effects: vec![Effect::SelectTool {
                    id: Some("stamp".into()),
                }],
                ..Default::default()
            },
            Request::Input {
                tool,
                input,
                context,
            } if tool == "stamp" => self.input(input, context),
            Request::ServiceResult { reply, context } => self.reply(reply, context),
            _ => Response::default(),
        })
    }
}
phistudio_sdk::export_plugin!(NoteStamp::default);
