//! Public SDK only. Network work starts only from an explicit user action.
use phistudio_sdk::*;
use serde_json::{Value, json};

#[derive(Default)]
struct Lab {
    features: Option<HostFeatures>,
    page: String,
    rows: Vec<TableRow>,
    audio: Option<AudioInfo>,
    ready: bool,
}
fn call(id: &str, service: &str, method: &str, args: Value) -> Effect {
    Effect::Call {
        call: ServiceCall {
            request_id: id.into(),
            service: ServiceRef {
                provider: "host".into(),
                id: service.into(),
                version: "^1".parse().unwrap(),
            },
            method: method.into(),
            arguments: args,
        },
    }
}
impl Lab {
    fn permitted(&self, cap: Capability) -> bool {
        self.features
            .as_ref()
            .is_some_and(|f| f.granted.contains(&cap))
    }
    fn panel(&self) -> Effect {
        Effect::Panel {
            id: "lab".into(),
            widgets: vec![Widget::Tabs {
                id: "pages".into(),
                selected: if self.page.is_empty() {
                    "report"
                } else {
                    &self.page
                }
                .into(),
                tabs: vec![
                    UiTab {
                        id: "report".into(),
                        title: "Report".into(),
                        children: vec![
                            Widget::Button {
                                id: "features".into(),
                                text: "Refresh host features".into(),
                                enabled: true,
                            },
                            Widget::Button {
                                id: "audio".into(),
                                text: "Inspect music".into(),
                                enabled: self.permitted(Capability::Audio),
                            },
                            Widget::Button {
                                id: "network".into(),
                                text: "Fetch https://example.com".into(),
                                enabled: self.permitted(Capability::Network),
                            },
                            Widget::Button {
                                id: "dialog".into(),
                                text: "Open demo dialog".into(),
                                enabled: true,
                            },
                            Widget::Table {
                                id: "report".into(),
                                columns: vec!["Field".into(), "Value".into()],
                                rows: self.rows.clone(),
                                selected: None,
                                height: 240.,
                            },
                        ],
                    },
                    UiTab {
                        id: "tree".into(),
                        title: "Feature tree".into(),
                        children: vec![Widget::Tree {
                            id: "features-tree".into(),
                            nodes: vec![TreeNode {
                                id: "services".into(),
                                text: "Host services".into(),
                                expanded: true,
                                children: self
                                    .features
                                    .as_ref()
                                    .map(|f| {
                                        f.services
                                            .iter()
                                            .map(|(id, version)| TreeNode {
                                                id: id.clone(),
                                                text: format!("{id} {version}"),
                                                children: vec![],
                                                expanded: false,
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                            }],
                            selected: None,
                        }],
                    },
                ],
            }],
        }
    }
    fn row(&mut self, id: &str, value: impl ToString) {
        self.rows.retain(|r| r.id != id);
        self.rows.push(TableRow {
            id: id.into(),
            cells: vec![id.into(), value.to_string().chars().take(1024).collect()],
        });
        if self.rows.len() > 64 {
            self.rows.remove(0);
        }
    }
}
impl Plugin for Lab {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        let mut response = Response::default();
        match request {
            Request::Activate { .. } => {}
            Request::ServiceResult { reply, .. } => {
                match &reply.outcome {
                    ServiceOutcome::Success { value } => match reply.request_id.as_str() {
                        "features" => {
                            let features: HostFeatures =
                                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                            self.row("editor", &features.editor_version);
                            let supported = ["table", "tree", "tabs"]
                                .iter()
                                .all(|w| features.widgets.contains(*w));
                            self.features = Some(features);
                            if !self.ready {
                                if !supported || !self.permitted(Capability::Ui) {
                                    return Ok(response);
                                }
                                // Contributions must be registered in Activate, so
                                // the feature reply only populates the registered UI.
                                self.ready = true;
                            }
                        }
                        "audio" => {
                            let info: AudioInfo =
                                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                            self.row("sample-rate", info.sample_rate);
                            self.row("frames", info.frames);
                            self.audio = Some(info);
                        }
                        "network" => {
                            let result: NetworkResponse =
                                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                            self.row("http-status", result.status);
                            self.row("http-bytes", result.bytes);
                            response.effects.push(call(
                                "close",
                                "network",
                                "close",
                                json!({"id":"demo"}),
                            ));
                        }
                        _ => {}
                    },
                    ServiceOutcome::Error { code, .. } => self.row("service-error", code),
                }
            }
            Request::Ui { event, context } => match event.widget.as_str() {
                "pages" => self.page = event.value.as_str().unwrap_or("report").into(),
                "features" => {
                    response
                        .effects
                        .push(call("features", "features", "get", Value::Null))
                }
                "audio" => response.effects.push(call(
                    "audio",
                    "audio",
                    "info",
                    json!(AudioQuery {
                        version: context.version,
                        assets: None
                    }),
                )),
                "network" => response.effects.push(call(
                    "network",
                    "network",
                    "request",
                    json!(NetworkRequest::get("demo", "https://example.com")),
                )),
                "dialog" => response.effects.push(Effect::Dialog {
                    panel: "dialog".into(),
                    open: true,
                    version: context.version,
                }),
                "close" => response.effects.push(Effect::Dialog {
                    panel: "dialog".into(),
                    open: false,
                    version: context.version,
                }),
                _ => {}
            },
            Request::Command { arguments, .. } => self.row("source-context", arguments),
            Request::Render { frame, .. } => {
                let mut layers = vec![];
                if let Some(clock) = &frame.viewport.timeline {
                    layers.push(DrawLayer {
                        space: DrawSpace::Timeline,
                        primitives: vec![DrawPrimitive::Circle {
                            center: [0., clock.beat + 1.],
                            radius: 5.,
                            color: DrawColor::Theme(ThemeColor::Accent),
                        }],
                    });
                }
                response.effects.push(Effect::Draw {
                    frame: frame.token,
                    layers,
                });
                return Ok(response);
            }
            _ => {}
        }
        if matches!(request, Request::Activate { .. }) {
            response.contributions = vec![
                Contribution::Panel {
                    id: "lab".into(),
                    title: "SDK Lab".into(),
                    location: PanelLocation::Window,
                    snapshot: false,
                },
                Contribution::Menu {
                    id: "menu".into(),
                    title: "SDK Lab".into(),
                    location: EditorMenu::File,
                    panel: "lab".into(),
                },
                Contribution::Panel {
                    id: "dialog".into(),
                    title: "SDK demo dialog".into(),
                    location: PanelLocation::Window,
                    snapshot: false,
                },
                Contribution::TimelineTrack {
                    id: "clock".into(),
                    title: "SDK timeline column".into(),
                    width: 180.,
                },
                Contribution::Command {
                    id: "inspect".into(),
                    title: "Inspect source context".into(),
                    shortcut: None,
                    snapshot: false,
                },
                Contribution::ContextMenu {
                    id: "note".into(),
                    title: "Inspect in SDK Lab".into(),
                    target: ContextTarget::Note,
                    command: "inspect".into(),
                },
                Contribution::ContextMenu {
                    id: "line".into(),
                    title: "Inspect in SDK Lab".into(),
                    target: ContextTarget::Line,
                    command: "inspect".into(),
                },
            ];
            // A baseline label remains valid before the feature discovery reply.
            response.effects.push(Effect::Panel {
                id: "lab".into(),
                widgets: vec![Widget::Label {
                    text: "Discovering host features…".into(),
                }],
            });
            response.effects.push(Effect::Panel {
                id: "dialog".into(),
                widgets: vec![
                    Widget::Label {
                        text: "This dialog uses the editor's modal and Back handling.".into(),
                    },
                    Widget::Button {
                        id: "close".into(),
                        text: "Close demo dialog".into(),
                        enabled: true,
                    },
                ],
            });
            response
                .effects
                .push(call("features", "features", "get", Value::Null));
        } else if self.ready {
            response.effects.push(self.panel());
        }
        Ok(response)
    }
}
phistudio_sdk::export_plugin!(Lab::default);
