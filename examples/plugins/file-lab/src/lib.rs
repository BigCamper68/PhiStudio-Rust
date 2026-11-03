use phistudio_sdk::*;
use serde_json::json;

#[derive(Default)]
struct FileLab {
    block_save: bool,
    block_open: bool,
    block_export: bool,
    rename_export: bool,
    message: String,
}
impl FileLab {
    fn panel(&self) -> Effect {
        Effect::Panel {
            id: "lab".into(),
            widgets: vec![
                Widget::Bool {
                    id: "save".into(),
                    text: "Block chart saves".into(),
                    value: self.block_save,
                },
                Widget::Bool {
                    id: "open".into(),
                    text: "Block chart opens".into(),
                    value: self.block_open,
                },
                Widget::Bool {
                    id: "export".into(),
                    text: "Block project exports".into(),
                    value: self.block_export,
                },
                Widget::Bool {
                    id: "rename".into(),
                    text: "Suggest a reviewed export name".into(),
                    value: self.rename_export,
                },
                Widget::Label {
                    text: self.message.clone(),
                },
            ],
        }
    }
    fn intercept(&mut self, payload: &serde_json::Value) -> Result<Response, String> {
        let mut file: FileIntercept =
            serde_json::from_value(payload.clone()).map_err(|e| e.to_string())?;
        file.validate()?;
        let blocked = match file.action {
            FileAction::Save => self.block_save,
            FileAction::Open => self.block_open,
            FileAction::Export => self.block_export,
        };
        self.message = format!(
            "{:?}: {}",
            file.action,
            file.name.as_deref().unwrap_or("new private project")
        );
        let hook = if blocked {
            HookDecision::Cancel {
                reason: format!("{:?} blocked by File workflow lab", file.action),
            }
        } else if self.rename_export
            && file.action == FileAction::Export
            && file.content == FileContent::Project
        {
            file.name = Some("reviewed-project.pez".into());
            HookDecision::Replace {
                payload: json!(file),
            }
        } else {
            HookDecision::Continue
        };
        Ok(Response {
            hook: Some(hook),
            effects: vec![self.panel()],
            ..Default::default()
        })
    }
}
impl Plugin for FileLab {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![
                    Contribution::Hook {
                        point: FILE_BEFORE_HOOK.into(),
                        priority: 0,
                    },
                    Contribution::Panel {
                        id: "lab".into(),
                        title: "File workflow controls".into(),
                        location: PanelLocation::Window,
                        snapshot: false,
                    },
                ],
                effects: vec![self.panel()],
                ..Default::default()
            },
            Request::Ui { event, .. } => {
                if let Some(value) = event.value.as_bool() {
                    match event.widget.as_str() {
                        "save" => self.block_save = value,
                        "open" => self.block_open = value,
                        "export" => self.block_export = value,
                        "rename" => self.rename_export = value,
                        _ => {}
                    }
                }
                Response {
                    effects: vec![self.panel()],
                    ..Default::default()
                }
            }
            Request::Hook { point, payload, .. } if point == FILE_BEFORE_HOOK => {
                return self.intercept(payload);
            }
            _ => Response::default(),
        })
    }
}
phistudio_sdk::export_plugin!(FileLab::default);
