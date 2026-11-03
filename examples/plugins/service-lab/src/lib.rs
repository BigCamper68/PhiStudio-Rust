use phistudio_sdk::*;
use serde_json::json;

#[derive(Default)]
struct ServiceLab {
    blocked: bool,
    message: String,
}
impl ServiceLab {
    fn panel(&self) -> Effect {
        Effect::Panel {
            id: "lab".into(),
            widgets: vec![
                Widget::Button {
                    id: "call".into(),
                    text: "Call virtual greeting".into(),
                    enabled: true,
                },
                Widget::Bool {
                    id: "block".into(),
                    text: "Block the virtual service".into(),
                    value: self.blocked,
                },
                Widget::Label {
                    text: self.message.clone(),
                },
            ],
        }
    }
    fn call_service(&self) -> Response {
        Response {
            effects: vec![Effect::Call {
                call: ServiceCall {
                    request_id: "greeting".into(),
                    service: ServiceRef {
                        provider: "host".into(),
                        id: "greeting".into(),
                        version: "^1".parse().expect("valid service requirement"),
                    },
                    method: "hello".into(),
                    arguments: json!({"name":"PhiStudio"}),
                },
            }],
            ..Default::default()
        }
    }
    fn intercept(&self, payload: &serde_json::Value) -> Result<Response, String> {
        let mut request: ServiceIntercept =
            serde_json::from_value(payload.clone()).map_err(|e| e.to_string())?;
        if request.call.service.provider != "host" || request.call.service.id != "greeting" {
            return Ok(Response::default());
        }
        let hook = if self.blocked {
            HookDecision::Cancel {
                reason: "The virtual service is blocked".into(),
            }
        } else {
            request.call.service.provider = "example.service-lab".into();
            HookDecision::Replace {
                payload: json!(request),
            }
        };
        Ok(Response {
            hook: Some(hook),
            ..Default::default()
        })
    }
}
impl Plugin for ServiceLab {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => {
                self.message = "Ready to intercept host/greeting".into();
                Response {
                    contributions: vec![
                        Contribution::Panel {
                            id: "lab".into(),
                            title: "Service interception".into(),
                            location: PanelLocation::Window,
                            snapshot: false,
                        },
                        Contribution::Command {
                            id: "call".into(),
                            title: "Call virtual greeting".into(),
                            shortcut: None,
                            snapshot: false,
                        },
                        Contribution::Hook {
                            point: SERVICE_BEFORE_HOOK.into(),
                            priority: 0,
                        },
                        Contribution::Service {
                            id: "greeting".into(),
                            version: Version::new(1, 0, 0),
                        },
                    ],
                    effects: vec![self.panel()],
                    ..Default::default()
                }
            }
            Request::Command { id, .. } if id == "call" => self.call_service(),
            Request::Ui { event, .. } if event.widget == "call" => self.call_service(),
            Request::Ui { event, .. } if event.widget == "block" => {
                if let Some(value) = event.value.as_bool() {
                    self.blocked = value;
                }
                Response {
                    effects: vec![self.panel()],
                    ..Default::default()
                }
            }
            Request::Hook { point, payload, .. } if point == SERVICE_BEFORE_HOOK => {
                return self.intercept(payload);
            }
            Request::Service {
                id,
                method,
                arguments,
                ..
            } if id == "greeting" && method == "hello" => {
                let name = arguments["name"].as_str().unwrap_or("PhiStudio");
                Response {
                    result: json!({"message":format!("Hello, {name}")}),
                    ..Default::default()
                }
            }
            Request::ServiceResult { reply, .. } => {
                self.message = match &reply.outcome {
                    ServiceOutcome::Success { value } => format!(
                        "{}/{}: {}",
                        reply.service.provider,
                        reply.service.id,
                        value["message"]
                            .as_str()
                            .unwrap_or("Invalid provider reply")
                    ),
                    ServiceOutcome::Error { code, message } => format!("{code}: {message}"),
                };
                Response {
                    effects: vec![
                        self.panel(),
                        Effect::Notify {
                            message: self.message.clone(),
                        },
                    ],
                    ..Default::default()
                }
            }
            _ => Response::default(),
        })
    }
}
phistudio_sdk::export_plugin!(ServiceLab::default);
