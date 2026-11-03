use phistudio_sdk::*;
use serde_json::json;
use std::collections::BTreeMap;

enum Pending {
    Open(AssetSource),
    Image(u64, AssetSource),
    List,
    Close,
}
struct AssetView {
    next: u64,
    pending: BTreeMap<String, Pending>,
    bundled: Option<u64>,
    project: Option<u64>,
    use_project: bool,
    path: String,
    status: String,
    entries: Vec<AssetEntry>,
    next_page: Option<usize>,
}
impl Default for AssetView {
    fn default() -> Self {
        Self {
            next: 0,
            pending: BTreeMap::new(),
            bundled: None,
            project: None,
            use_project: false,
            path: "cover.png".into(),
            status: "Loading bundled image…".into(),
            entries: vec![],
            next_page: None,
        }
    }
}
impl AssetView {
    fn request(&mut self, method: &str, arguments: serde_json::Value, pending: Pending) -> Effect {
        self.next += 1;
        let request_id = format!("asset-{}", self.next);
        self.pending.insert(request_id.clone(), pending);
        Effect::Call {
            call: ServiceCall {
                request_id,
                service: ServiceRef {
                    provider: "host".into(),
                    id: "assets".into(),
                    version: VersionReq::STAR,
                },
                method: method.into(),
                arguments,
            },
        }
    }
    fn layers(&self, min: [f64; 2], max: [f64; 2], space: DrawSpace) -> Vec<DrawLayer> {
        let image = if self.use_project {
            self.project
        } else {
            self.bundled
        };
        image
            .map(|file| {
                vec![DrawLayer {
                    space,
                    primitives: vec![DrawPrimitive::Image {
                        file,
                        min,
                        max,
                        tint: DrawColor::Rgba([255; 4]),
                    }],
                }]
            })
            .unwrap_or_default()
    }
    fn panel(&self) -> Effect {
        let mut widgets = vec![
            Widget::Label {
                text: self.status.clone(),
            },
            Widget::Canvas {
                height: 88.,
                layers: self.layers([0.25, 0.], [0.75, 1.], DrawSpace::Normalized),
            },
            Widget::Text {
                id: "path".into(),
                text: "Project image path".into(),
                value: self.path.clone(),
            },
            Widget::Button {
                id: "image".into(),
                text: "Show project image".into(),
                enabled: true,
            },
            Widget::Button {
                id: "bundled".into(),
                text: "Use bundled image".into(),
                enabled: self.bundled.is_some(),
            },
            Widget::Button {
                id: "list".into(),
                text: "Read project resources".into(),
                enabled: true,
            },
        ];
        for entry in &self.entries {
            widgets.push(Widget::Label {
                text: if entry.directory {
                    format!("{}/", entry.path)
                } else {
                    format!("{} · {} bytes", entry.path, entry.size)
                },
            });
        }
        if self.next_page.is_some() {
            widgets.push(Widget::Button {
                id: "more".into(),
                text: "More project resources".into(),
                enabled: true,
            });
        }
        Effect::Panel {
            id: "assets".into(),
            widgets,
        }
    }
    fn reply(&mut self, reply: &ServiceReply) -> Result<Vec<Effect>, String> {
        let Some(pending) = self.pending.remove(&reply.request_id) else {
            return Ok(vec![]);
        };
        if matches!(pending, Pending::Close) {
            return Ok(vec![]);
        }
        let value = match &reply.outcome {
            ServiceOutcome::Success { value } => value,
            ServiceOutcome::Error { message, .. } => {
                self.status = format!("Resource unavailable: {message}");
                let mut effects = vec![];
                if let Pending::Image(file, _) = pending {
                    effects.push(self.request(
                        "close",
                        json!(AssetHandle { file }),
                        Pending::Close,
                    ));
                }
                effects.push(self.panel());
                return Ok(effects);
            }
        };
        Ok(match pending {
            Pending::Open(source) => {
                let file: AssetFile =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                vec![self.request(
                    "image",
                    json!(AssetHandle { file: file.id }),
                    Pending::Image(file.id, source),
                )]
            }
            Pending::Image(file, source) => {
                let image: AssetImage =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                match source {
                    AssetSource::Plugin => self.bundled = Some(file),
                    AssetSource::Project => {
                        self.project = Some(file);
                        self.use_project = true;
                    }
                }
                self.status = format!(
                    "{} image · {} × {}",
                    if source == AssetSource::Plugin {
                        "Bundled"
                    } else {
                        "Project"
                    },
                    image.width,
                    image.height
                );
                vec![self.panel()]
            }
            Pending::List => {
                let page: AssetPage =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                self.status = format!(
                    "Project resources · {} total · page from {}",
                    page.total, page.offset
                );
                self.entries = page.entries;
                self.next_page = page.next_offset;
                vec![self.panel()]
            }
            Pending::Close => vec![],
        })
    }
}
impl Plugin for AssetView {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        let mut response = Response::default();
        match request {
            Request::Activate { .. } => {
                response.contributions = vec![
                    Contribution::Panel {
                        id: "assets".into(),
                        title: "Asset view".into(),
                        location: PanelLocation::Window,
                        snapshot: false,
                    },
                    Contribution::Preview { id: "image".into() },
                    Contribution::Track {
                        id: "images".into(),
                        title: "Asset image track".into(),
                    },
                    Contribution::Event {
                        topic: "host.document.opened".into(),
                    },
                    Contribution::Event {
                        topic: "host.project.assets.changed".into(),
                    },
                ];
                response.effects = vec![
                    self.panel(),
                    self.request(
                        "open",
                        json!(AssetOpen {
                            source: AssetSource::Plugin,
                            path: "mark.bmp".into()
                        }),
                        Pending::Open(AssetSource::Plugin),
                    ),
                ];
            }
            Request::Ui { event, .. } => {
                match event.widget.as_str() {
                    "path" => {
                        if let Some(path) = event.value.as_str() {
                            self.path = path.into();
                        }
                    }
                    "bundled" => {
                        self.use_project = false;
                        self.status = "Using bundled image".into();
                    }
                    "image" => {
                        if let Some(file) = self.project.take() {
                            response.effects.push(self.request(
                                "close",
                                json!(AssetHandle { file }),
                                Pending::Close,
                            ));
                        }
                        response.effects.push(self.request(
                            "open",
                            json!(AssetOpen {
                                source: AssetSource::Project,
                                path: self.path.clone()
                            }),
                            Pending::Open(AssetSource::Project),
                        ));
                    }
                    "list" | "more" => {
                        let offset = if event.widget == "more" {
                            self.next_page.unwrap_or(0)
                        } else {
                            0
                        };
                        response.effects.push(self.request(
                            "list",
                            json!(AssetList {
                                source: AssetSource::Project,
                                directory: String::new(),
                                offset,
                                limit: 32
                            }),
                            Pending::List,
                        ));
                    }
                    _ => {}
                }
                response.effects.insert(0, self.panel());
            }
            Request::ServiceResult { reply, .. } => response.effects = self.reply(reply)?,
            Request::Event { topic, .. }
                if topic == "host.document.opened" || topic == "host.project.assets.changed" =>
            {
                let had_entries = !self.entries.is_empty();
                self.entries.clear();
                self.next_page = None;
                if self.project.take().is_some() {
                    self.use_project = false;
                    self.status = "Project resources changed; reopen the project image".into();
                    response.effects.push(self.panel());
                } else if had_entries {
                    response.effects.push(self.panel());
                }
            }
            Request::Render { frame, context } => {
                response.effects.push(Effect::Draw {
                    frame: frame.token,
                    layers: match frame.target.kind {
                        RenderKind::Preview => {
                            self.layers([0.02, 0.02], [0.18, 0.18], DrawSpace::Normalized)
                        }
                        RenderKind::Track => self.layers(
                            [-40., context.beat],
                            [40., context.beat + 0.5],
                            DrawSpace::Timeline,
                        ),
                        RenderKind::PreviewEffect => vec![],
                    },
                });
            }
            _ => {}
        }
        Ok(response)
    }
}
phistudio_sdk::export_plugin!(AssetView::default);
