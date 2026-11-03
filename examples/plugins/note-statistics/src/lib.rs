use phistudio_sdk::*;
use serde_json::{Value, json};

struct Scan {
    id: String,
    version: DocumentVersion,
    line: usize,
    offset: usize,
    next: Option<usize>,
    counts: [u64; 5],
}
#[derive(Default)]
struct NoteStatistics {
    sequence: u64,
    scan: Option<Scan>,
    message: String,
}
impl NoteStatistics {
    fn panel(&self) -> Effect {
        Effect::Panel {
            id: "statistics".into(),
            widgets: vec![
                Widget::Button {
                    id: "count".into(),
                    text: "Count notes on active line".into(),
                    enabled: true,
                },
                Widget::Label {
                    text: self.message.clone(),
                },
            ],
        }
    }
    fn request(&self, provider: &str, service: &str, method: &str, arguments: Value) -> Effect {
        Effect::Call {
            call: ServiceCall {
                request_id: self
                    .scan
                    .as_ref()
                    .map(|s| s.id.clone())
                    .unwrap_or_else(|| "unused".into()),
                service: ServiceRef {
                    provider: provider.into(),
                    id: service.into(),
                    version: "^1".parse().expect("valid service requirement"),
                },
                method: method.into(),
                arguments,
            },
        }
    }
    fn page(&self) -> Option<Effect> {
        let scan = self.scan.as_ref()?;
        Some(self.request(
            "host",
            "document",
            "slice",
            json!(DocumentPage {
                version: scan.version,
                path: format!("/judgeLineList/{}/notes", scan.line),
                offset: scan.offset,
                limit: 4096,
            }),
        ))
    }
    fn begin(&mut self, context: &EditorContext) -> Response {
        self.sequence = self.sequence.wrapping_add(1);
        self.scan = Some(Scan {
            id: format!("scan-{}", self.sequence),
            version: context.version,
            line: context.line,
            offset: 0,
            next: None,
            counts: [0; 5],
        });
        self.message = "Reading notes…".into();
        let mut effects = vec![self.panel()];
        effects.extend(self.page());
        Response {
            effects,
            ..Default::default()
        }
    }
    fn reply(&mut self, reply: &ServiceReply, context: &EditorContext) -> Result<Response, String> {
        let Some(scan) = &mut self.scan else {
            return Ok(Response::default());
        };
        if reply.request_id != scan.id || context.version != scan.version {
            return Ok(Response::default());
        }
        let value = match &reply.outcome {
            ServiceOutcome::Success { value } => value,
            ServiceOutcome::Error { code, message } => {
                self.message = format!("{code}: {message}");
                self.scan = None;
                return Ok(Response {
                    effects: vec![self.panel()],
                    ..Default::default()
                });
            }
        };
        let mut effects = vec![];
        if reply.service.provider == "host" {
            let page: DocumentItems =
                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            if page.version != scan.version
                || page.offset != scan.offset
                || page.next_offset.is_some_and(|next| next <= page.offset)
            {
                return Err("Invalid document page".into());
            }
            scan.next = page.next_offset;
            effects.push(self.request(
                "example.note-statistics",
                "histogram",
                "count",
                json!(page.items),
            ));
        } else {
            let counts: [u64; 5] =
                serde_json::from_value(value["counts"].clone()).map_err(|e| e.to_string())?;
            for (total, count) in scan.counts.iter_mut().zip(counts) {
                *total += count;
            }
            if let Some(offset) = scan.next {
                scan.offset = offset;
                self.message = format!("Read {offset} notes…");
                effects.extend(self.page());
            } else {
                self.message = format!(
                    "Line {}: {} notes · Tap {} · Hold {} · Flick {} · Drag {} · Other {}",
                    scan.line,
                    scan.counts.iter().sum::<u64>(),
                    scan.counts[1],
                    scan.counts[2],
                    scan.counts[3],
                    scan.counts[4],
                    scan.counts[0]
                );
                self.scan = None;
            }
            effects.insert(0, self.panel());
        }
        Ok(Response {
            effects,
            ..Default::default()
        })
    }
}
impl Plugin for NoteStatistics {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => {
                self.message = "Ready".into();
                self.scan = None;
                Response {
                    contributions: vec![
                        Contribution::Command {
                            id: "count".into(),
                            title: "Count notes on active line".into(),
                            shortcut: None,
                            snapshot: false,
                        },
                        Contribution::Panel {
                            id: "statistics".into(),
                            title: "Note statistics".into(),
                            location: PanelLocation::Window,
                            snapshot: false,
                        },
                        Contribution::Service {
                            id: "histogram".into(),
                            version: Version::new(1, 0, 0),
                        },
                    ],
                    effects: vec![self.panel()],
                    ..Default::default()
                }
            }
            Request::Command { id, context, .. } if id == "count" => self.begin(context),
            Request::Ui { event, context } if event.widget == "count" => self.begin(context),
            Request::ServiceResult { reply, context } => return self.reply(reply, context),
            Request::Service {
                id,
                method,
                arguments,
                ..
            } if id == "histogram" && method == "count" => {
                let Some(notes) = arguments.as_array() else {
                    return Ok(Response {
                        result: json!({"error":"Expected an array of source note objects"}),
                        ..Default::default()
                    });
                };
                let mut counts = [0_u64; 5];
                for note in notes {
                    let kind = note["type"]
                        .as_u64()
                        .filter(|v| (1..=4).contains(v))
                        .unwrap_or(0) as usize;
                    counts[kind] += 1;
                }
                Response {
                    result: json!({"counts":counts}),
                    ..Default::default()
                }
            }
            _ => Response::default(),
        })
    }
}
phistudio_sdk::export_plugin!(NoteStatistics::default);
