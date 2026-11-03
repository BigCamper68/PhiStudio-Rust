use phistudio_sdk::*;
use serde_json::{Value, json};

const ID: &str = "example.project-data";
struct Tags {
    tag: String,
    policy: String,
    status: String,
    sequence: u64,
    pending: String,
}
impl Default for Tags {
    fn default() -> Self {
        Self {
            tag: "My tag".into(),
            policy: "editor_only".into(),
            status: "Reading project tag…".into(),
            sequence: 0,
            pending: String::new(),
        }
    }
}
impl Tags {
    fn panel(&self) -> Effect {
        Effect::Panel {
            id: "tag".into(),
            widgets: vec![
                Widget::Label {
                    text: self.status.clone(),
                },
                Widget::Text {
                    id: "text".into(),
                    text: "Project tag".into(),
                    value: self.tag.clone(),
                },
                Widget::Choice {
                    id: "policy".into(),
                    text: "Export behavior".into(),
                    value: self.policy.clone(),
                    options: vec![
                        ("runtime".into(), "Keep during runtime export".into()),
                        ("editor_only".into(), "Editor only".into()),
                        ("requires_conversion".into(), "Requires conversion".into()),
                    ],
                },
                Widget::Button {
                    id: "store".into(),
                    text: "Store tag in project".into(),
                    enabled: true,
                },
                Widget::Button {
                    id: "remove".into(),
                    text: "Remove project tag".into(),
                    enabled: true,
                },
                Widget::Button {
                    id: "read".into(),
                    text: "Reload project tag".into(),
                    enabled: true,
                },
            ],
        }
    }
    fn read(&mut self, version: DocumentVersion) -> Effect {
        self.sequence += 1;
        self.pending = format!("tag-{}", self.sequence);
        Effect::Call {
            call: ServiceCall {
                request_id: self.pending.clone(),
                service: ServiceRef {
                    provider: "host".into(),
                    id: "document".into(),
                    version: VersionReq::STAR,
                },
                method: "plugin_data".into(),
                arguments: json!(PluginDataQuery {
                    version,
                    plugin: ID.into()
                }),
            },
        }
    }
}
impl Plugin for Tags {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        let mut response = Response::default();
        match request {
            Request::Activate { .. } => {
                response.contributions = vec![
                    Contribution::Panel {
                        id: "tag".into(),
                        title: "Project data".into(),
                        location: PanelLocation::Window,
                        snapshot: false,
                    },
                    Contribution::Event {
                        topic: "host.document.opened".into(),
                    },
                    Contribution::Event {
                        topic: "host.document.changed".into(),
                    },
                ];
                response.effects.push(self.panel());
            }
            Request::Event { context, .. } => response.effects.push(self.read(context.version)),
            Request::Ui { event, context } => {
                match event.widget.as_str() {
                    "text" => {
                        if let Some(value) = event.value.as_str() {
                            self.tag = value.into();
                        }
                    }
                    "policy" => {
                        if let Some(value) = event.value.as_str() {
                            self.policy = value.into();
                        }
                    }
                    "read" => response.effects.push(self.read(context.version)),
                    "store" | "remove" => {
                        let entry = (event.widget == "store").then(|| {
                            PluginData::new(
                                1,
                                match self.policy.as_str() {
                                    "runtime" => PluginDataPolicy::Runtime,
                                    "requires_conversion" => PluginDataPolicy::RequiresConversion,
                                    _ => PluginDataPolicy::EditorOnly,
                                },
                                json!({"tag":self.tag}),
                            )
                        });
                        response.effects.push(Effect::Edit {
                            transaction: Transaction {
                                version: context.version,
                                label: if entry.is_some() {
                                    "Store project tag"
                                } else {
                                    "Remove project tag"
                                }
                                .into(),
                                operations: vec![PatchOperation::PluginData {
                                    plugin: ID.into(),
                                    entry,
                                }],
                            },
                        });
                    }
                    _ => {}
                }
                // Loading/current descriptions precede worker-resolved replies.
                response.effects.insert(0, self.panel());
            }
            Request::ServiceResult { reply, .. } if reply.request_id == self.pending => {
                match &reply.outcome {
                    ServiceOutcome::Success { value } => {
                        let result: PluginDataValue =
                            serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                        if !result.present {
                            self.status = "No project tag stored".into();
                        } else if let Ok(entry) = serde_json::from_value::<PluginData>(result.entry)
                        {
                            if entry.version == 1 {
                                self.tag = entry
                                    .data
                                    .get("tag")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .into();
                                self.policy = match entry.export {
                                    PluginDataPolicy::Runtime => "runtime",
                                    PluginDataPolicy::EditorOnly => "editor_only",
                                    PluginDataPolicy::RequiresConversion => "requires_conversion",
                                }
                                .into();
                                self.status = "Project tag loaded".into();
                            } else {
                                self.status =
                                    "Unsupported project tag version; data preserved".into();
                            }
                        } else {
                            self.status = "Unknown project tag format; data preserved".into();
                        }
                    }
                    ServiceOutcome::Error { code, message } if code != "stale_document" => {
                        self.status = format!("Tag unavailable: {message}")
                    }
                    _ => {}
                }
                response.effects.push(self.panel());
            }
            _ => {}
        }
        Ok(response)
    }
}
phistudio_sdk::export_plugin!(Tags::default);
