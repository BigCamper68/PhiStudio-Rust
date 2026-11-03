use phistudio_sdk::*;
use serde_json::json;

#[derive(Default)]
struct ActionLab {
    block_delete: bool,
    block_undo: bool,
    quantize_seek: bool,
    message: String,
}
impl ActionLab {
    fn panel(&self) -> Effect {
        let mut widgets = vec![
            Widget::Bool {
                id: "delete".into(),
                text: "Block native deletion".into(),
                value: self.block_delete,
            },
            Widget::Bool {
                id: "undo".into(),
                text: "Block Undo".into(),
                value: self.block_undo,
            },
            Widget::Bool {
                id: "quantize".into(),
                text: "Snap API seeks to bars".into(),
                value: self.quantize_seek,
            },
        ];
        for (id, text) in [
            ("play", "Play / pause through API"),
            ("seek", "Seek forward through API"),
            ("history", "Undo through API"),
            ("status", "Read editor status"),
        ] {
            widgets.push(Widget::Button {
                id: id.into(),
                text: text.into(),
                enabled: true,
            });
        }
        widgets.push(Widget::Label {
            text: self.message.clone(),
        });
        Effect::Panel {
            id: "lab".into(),
            widgets,
        }
    }
    fn call_editor(&self, method: &str, arguments: serde_json::Value) -> Effect {
        Effect::Call {
            call: ServiceCall {
                request_id: "control".into(),
                service: ServiceRef {
                    provider: "host".into(),
                    id: "editor".into(),
                    version: VersionReq::STAR,
                },
                method: method.into(),
                arguments,
            },
        }
    }
}
impl Plugin for ActionLab {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![
                    Contribution::Hook {
                        point: ACTION_BEFORE_HOOK.into(),
                        priority: 0,
                    },
                    Contribution::Panel {
                        id: "lab".into(),
                        title: "Editor action controls".into(),
                        location: PanelLocation::Window,
                        snapshot: false,
                    },
                ],
                effects: vec![self.panel()],
                ..Default::default()
            },
            Request::Ui { event, context } => {
                if let Some(value) = event.value.as_bool() {
                    match event.widget.as_str() {
                        "delete" => self.block_delete = value,
                        "undo" => self.block_undo = value,
                        "quantize" => self.quantize_seek = value,
                        _ => {}
                    }
                }
                let action = match event.widget.as_str() {
                    "play" => Some(EditorAction::SetPlayback {
                        playing: !context.playing,
                    }),
                    "seek" => Some(EditorAction::Seek {
                        beat: (context.beat + 1.5).min(MAX_EDITOR_SEEK_BEAT),
                    }),
                    "history" => Some(EditorAction::Undo),
                    _ => None,
                };
                let mut effects = vec![self.panel()];
                if let Some(action) = action {
                    effects.push(self.call_editor(
                        "action",
                        json!(EditorActionRequest {
                            version: context.version,
                            action
                        }),
                    ));
                } else if event.widget == "status" {
                    effects.push(self.call_editor(
                        "status",
                        json!(EditorStatusQuery {
                            version: context.version
                        }),
                    ));
                }
                Response {
                    effects,
                    ..Default::default()
                }
            }
            Request::Hook { point, payload, .. } if point == ACTION_BEFORE_HOOK => {
                let mut payload: ActionIntercept =
                    serde_json::from_value(payload.clone()).map_err(|e| e.to_string())?;
                let blocked = (self.block_delete
                    && payload.action == EditorAction::DeleteSelection)
                    || (self.block_undo && payload.action == EditorAction::Undo);
                self.message = format!(
                    "{:?}, source: {}",
                    payload.action,
                    payload.source.as_deref().unwrap_or("editor")
                );
                let hook = if blocked {
                    HookDecision::Cancel {
                        reason: "Blocked by Editor action lab".into(),
                    }
                } else if let EditorAction::Seek { beat } = &mut payload.action
                    && self.quantize_seek
                {
                    *beat = (*beat / 4.).round() * 4.;
                    HookDecision::Replace {
                        payload: json!(payload),
                    }
                } else {
                    HookDecision::Continue
                };
                Response {
                    hook: Some(hook),
                    effects: vec![self.panel()],
                    ..Default::default()
                }
            }
            Request::ServiceResult { reply, .. } => {
                self.message = match &reply.outcome {
                    ServiceOutcome::Success { value } => format!("Editor: {value}"),
                    ServiceOutcome::Error { message, .. } => {
                        format!("Editor action declined: {message}")
                    }
                };
                Response {
                    effects: vec![self.panel()],
                    ..Default::default()
                }
            }
            _ => Response::default(),
        })
    }
}
phistudio_sdk::export_plugin!(ActionLab::default);
