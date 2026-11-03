//! Installable Chart Linter. Only public SDK requests cross the host boundary.
mod lint;
mod resources;
#[cfg(test)]
mod tests;
mod ui;
use lint::*;
use phistudio_sdk::*;
use resources::Resources;
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};

mod scan;
use scan::{Scan, Task};

#[derive(Default)]
pub struct ChartLinter {
    scan: Option<Scan>,
    pending: Option<(String, Task)>,
    serial: u64,
    calls: u64,
    report: Report,
    version: Option<DocumentVersion>,
    message: String,
    severity: String,
    category: String,
    search: String,
    page: usize,
    target: String,
    show_info: bool,
    confirm: Option<usize>,
    abandoned: BTreeMap<String, Task>,
    cleanup: std::collections::BTreeSet<u64>,
}
impl ChartLinter {
    fn clear(&mut self, message: &str) {
        if let Some(file) = self.scan.as_ref().and_then(|s| s.open_file) {
            self.cleanup.insert(file);
        }
        self.scan = None;
        if let Some((id, task @ Task::Open(..))) = self.pending.take() {
            self.abandoned.insert(id, task);
        }
        self.serial += 1;
        self.calls = 0;
        self.report = Report::default();
        self.version = None;
        self.confirm = None;
        self.page = 0;
        self.message = message.into()
    }
    fn start(&mut self, version: DocumentVersion, deep: bool) {
        self.clear(if deep {
            "Deep scan in progress…"
        } else {
            "Basic scan in progress…"
        });
        self.version = Some(version);
        self.scan = Some(Scan::new(version, deep))
    }
    fn call_host(
        &mut self,
        service: &str,
        method: &str,
        arguments: Value,
        request_id: String,
    ) -> Effect {
        Effect::Call {
            call: ServiceCall {
                request_id,
                service: ServiceRef {
                    provider: "host".into(),
                    id: service.into(),
                    version: VersionReq::STAR,
                },
                method: method.into(),
                arguments,
            },
        }
    }
    fn advance(&mut self) -> Vec<Effect> {
        if self.pending.is_some() {
            return vec![];
        }
        if self.scan.is_some() && self.calls >= 40_000 {
            let scan = self.scan.take().unwrap();
            self.report.complete = false;
            self.report.push(
                Severity::Error,
                "scan_coverage",
                "",
                None,
                "Scan stopped at its 40,000-query budget; results are incomplete",
                false,
            );
            self.message = "Scan incomplete: query budget reached".into();
            return scan
                .open_file
                .map(|file| {
                    self.call_host(
                        "assets",
                        "close",
                        json!(AssetHandle { file }),
                        format!("cleanup.f{file}"),
                    )
                })
                .into_iter()
                .collect();
        }
        let Some(scan) = &mut self.scan else {
            return vec![];
        };
        if let Some(task) = scan.next(&mut self.report) {
            let (service, method, arguments) = scan.request(&task);
            self.calls += 1;
            let id = format!("scan{}.call{}", self.serial, self.calls);
            self.pending = Some((id.clone(), task));
            vec![self.call_host(service, method, arguments, id)]
        } else {
            let scan = self.scan.take().unwrap();
            self.report.complete = !scan.failed;
            self.report.deep_complete = scan.deep && !scan.failed && !scan.native_failed;
            self.message = format!(
                "{}: {} notes, {} events{}",
                if scan.failed || scan.native_failed {
                    "Scan incomplete"
                } else {
                    "Scan complete"
                },
                self.report.checked_notes,
                self.report.checked_events,
                if scan.deep {
                    format!(", {} native note inspections", scan.deep_checked)
                } else {
                    String::new()
                }
            );
            vec![]
        }
    }
    fn failure(&mut self, task: Task, code: &str, message: &str) {
        let Some(scan) = &mut self.scan else { return };
        let missing = code == "path_not_found";
        match task {
            Task::Events {
                path,
                key,
                layer,
                offset,
                overlap,
                ..
            } if missing && offset == 0 => finish_track(
                Track::default(),
                &path,
                &key,
                layer,
                overlap,
                &mut self.report,
            ),
            Task::Layers(line) if missing => {
                for key in BASE.into_iter().rev() {
                    scan.tasks.push_front(Scan::event_task(line, key, Some(0)))
                }
            }
            Task::Notes(_, 0) | Task::Aux(_) if missing => {}
            Task::Close(_) => {}
            Task::Read { file, .. } => {
                self.cleanup.insert(file);
                scan.open_file = None;
                scan.failed = true;
                self.report
                    .push(Severity::Error, "resource_read", "", None, message, false);
            }
            Task::Assets(..) => {
                scan.failed = true;
                scan.assets_failed = true;
                scan.tasks.retain(|t| !matches!(t, Task::Assets(..)));
                self.report.push(Severity::Error,"scan_coverage","",None,format!("Resource listing is incomplete: {message}. Missing-file checks were skipped."),false);
            }
            Task::Probe { ref offsets, .. }
                if message.contains("sample budget") && scan.retry_inspection(offsets) => {}
            Task::Probe { .. } => {
                scan.native_failed = true;
                scan.failed = true;
                self.report.push(
                    Severity::Warning,
                    "native_inspection",
                    "",
                    None,
                    format!("Native inspection unavailable: {message}. Deep scan is incomplete."),
                    false,
                )
            }
            task => {
                scan.failed = true;
                self.report.push(
                    Severity::Error,
                    "scan_coverage",
                    "",
                    None,
                    format!("Could not inspect {task:?}: {message}"),
                    false,
                )
            }
        }
    }
}
impl Plugin for ChartLinter {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        let mut response = Response::default();
        match request {
            Request::Activate { .. } => {
                self.target = "next".into();
                self.severity = "all".into();
                self.category = "all".into();
                self.show_info = true;
                self.message =
                    "Run a basic scan or an explicit deep scan. Scanning never edits the chart."
                        .into();
                response.contributions = vec![
                    Contribution::Menu {
                        id: "open".into(),
                        title: "Chart Linter".into(),
                        location: EditorMenu::File,
                        panel: "report".into(),
                    },
                    Contribution::Command {
                        id: "scan".into(),
                        title: "Chart Linter: scan chart".into(),
                        shortcut: None,
                        snapshot: false,
                    },
                    Contribution::Panel {
                        id: "report".into(),
                        title: "Chart Linter".into(),
                        location: PanelLocation::Window,
                        snapshot: false,
                    },
                ];
                for topic in [
                    "host.document.opened",
                    "host.document.changed",
                    "host.project.assets.changed",
                    "host.preview.configuration.changed",
                    "host.plugins.changed",
                ] {
                    response.contributions.push(Contribution::Event {
                        topic: topic.into(),
                    })
                }
            }
            Request::Command { context, .. } => self.start(context.version, false),
            Request::Ui { event, context } => {
                if self.version.is_some_and(|v| v != context.version) {
                    self.clear("Chart changed. Run a new scan.")
                }
                match event.widget.as_str() {
                    "scan" => self.start(context.version, false),
                    "deep" => self.start(context.version, true),
                    "cancel" => self.clear("Scan canceled. Late results are ignored."),
                    "target" => {
                        if let Some(value) = event
                            .value
                            .as_str()
                            .filter(|v| ["next", "rpe", "phira"].contains(v))
                        {
                            self.target = value.into();
                            self.clear("Compatibility target changed. Run a new scan.")
                        }
                    }
                    "severity" => {
                        self.severity = event.value.as_str().unwrap_or("all").into();
                        self.page = 0
                    }
                    "category" => {
                        self.category = event.value.as_str().unwrap_or("all").into();
                        self.page = 0
                    }
                    "info" => {
                        self.show_info = event.value.as_bool().unwrap_or(true);
                        self.page = 0
                    }
                    "search" => {
                        self.search = event.value.as_str().unwrap_or("").into();
                        self.page = 0
                    }
                    "previous" => self.page = self.page.saturating_sub(1),
                    "next" => self.page += 1,
                    "keep" => self.confirm = None,
                    "remove_confirm" => {
                        if let Some(index) = self.confirm.take()
                            && let Some(issue) =
                                self.report.issues.get(index).filter(|i| i.removable)
                            && self.version == Some(context.version)
                        {
                            response.effects.push(Effect::Edit {
                                transaction: Transaction {
                                    version: context.version,
                                    label: "Chart Linter: remove object".into(),
                                    operations: vec![PatchOperation::Remove {
                                        path: issue.path.clone(),
                                    }],
                                },
                            });
                            self.clear(
                                "Object removal submitted. Undo restores it; run a new scan.",
                            );
                        }
                    }
                    widget => {
                        if let Some(index) = widget
                            .strip_prefix("locate.r")
                            .and_then(|v| v.parse::<usize>().ok())
                            && let Some(issue) = self.report.issues.get(index)
                        {
                            let path = if issue.code == "missing_baseline" {
                                issue.path.split('/').take(3).collect::<Vec<_>>().join("/")
                            } else {
                                issue.path.clone()
                            };
                            let arguments = json!(EditorReveal {
                                version: context.version,
                                path,
                                beat: issue.beat.map(|b| (b - 0.5).max(0.))
                            });
                            response.effects.push(self.call_host(
                                "editor",
                                "reveal",
                                arguments,
                                "locate".into(),
                            ));
                        } else if let Some(index) = widget
                            .strip_prefix("remove.r")
                            .and_then(|v| v.parse::<usize>().ok())
                            && self.scan.is_none()
                            && self.report.issues.get(index).is_some_and(|i| i.removable)
                        {
                            self.confirm = Some(index)
                        }
                    }
                }
            }
            Request::Event { topic, context, .. } => {
                // The first resource/open notification can follow activation or
                // a scan command for the same revision. Changed versions and
                // later asset/configuration events invalidate all source indices.
                if self.version.is_some()
                    && (topic != "host.document.opened" || self.version != Some(context.version))
                {
                    self.clear(if self.version != Some(context.version) {
                        "Chart changed. Run a new scan."
                    } else {
                        "Project resources or preview configuration changed. Run a new scan."
                    })
                }
            }
            Request::ServiceResult { reply, context } => {
                if self.abandoned.remove(&reply.request_id).is_some()
                    && let ServiceOutcome::Success { value } = &reply.outcome
                    && let Ok(file) = serde_json::from_value::<AssetFile>(value.clone())
                {
                    self.cleanup.insert(file.id);
                }
                if self.version.is_some_and(|v| v != context.version) {
                    self.clear("Chart changed while scanning. Run a new scan.")
                }
                if reply.request_id == "locate" {
                    if let ServiceOutcome::Error { message, .. } = &reply.outcome {
                        self.message = format!("Could not locate object: {message}")
                    }
                } else if self
                    .pending
                    .as_ref()
                    .is_some_and(|(id, _)| id == &reply.request_id)
                {
                    let (_, task) = self.pending.take().unwrap();
                    match &reply.outcome {
                        ServiceOutcome::Success { value } => {
                            if let Some(scan) = &mut self.scan
                                && let Err(error) = scan.accept(
                                    task.clone(),
                                    value.clone(),
                                    &mut self.report,
                                    &self.target,
                                )
                            {
                                self.failure(task, "invalid_reply", &error)
                            }
                        }
                        ServiceOutcome::Error { code, message } => {
                            self.failure(task, code, message)
                        }
                    }
                }
            }
            Request::Deactivate => self.clear("Plugin disabled"),
            _ => {}
        }
        for file in std::mem::take(&mut self.cleanup) {
            response.effects.push(self.call_host(
                "assets",
                "close",
                json!(AssetHandle { file }),
                format!("cleanup.f{file}"),
            ));
        }
        let continuation = self.advance();
        // Worker-owned services can reply within this same cascade. Publish
        // current UI first so an older panel cannot overwrite the reply's UI.
        response.effects.push(self.panel());
        response.effects.extend(continuation);
        Ok(response)
    }
}
phistudio_sdk::export_plugin!(ChartLinter::default);
