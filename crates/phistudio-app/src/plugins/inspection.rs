//! Generic project metadata and queued original-renderer inspection services.
use super::*;
use phistudio_sdk::*;

impl Workbench {
    pub(super) fn plugin_project_service(&mut self, call: HostCall) {
        let result = (|| -> Result<ProjectInfo, String> {
            if call.request.method != "info" {
                return Err("Unknown project inspection method".into());
            }
            let query: ProjectInfoQuery = serde_json::from_value(call.request.arguments.clone())
                .map_err(|e| e.to_string())?;
            if query.version != self.plugin_version() {
                return Err("Project inspection document changed".into());
            }
            self.plugins
                .runtime
                .as_ref()
                .ok_or("Plugin host stopped")?
                .authorize_inspection(&call, query.version, "project", Capability::ProjectRead)?;
            Ok(ProjectInfo {
                version: query.version,
                assets: ProjectAssetsVersion {
                    session: self.session,
                    revision: self.asset_revision,
                },
                has_assets: self.asset_root().is_some(),
                chart_name: self
                    .file
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|p| p.to_string_lossy().into_owned()),
                music: self.project_options.music.clone(),
                illustration: self.project_options.illustration.clone(),
                music_duration: self
                    .transport
                    .music_duration
                    .filter(|v| v.is_finite() && *v > 0.),
                music_offset: self
                    .project_options
                    .music_offset_seconds(self.editor.document()),
                locale: self.options.language.code().into(),
                plugins: self
                    .plugins
                    .registry
                    .plugins
                    .iter()
                    .filter(|p| p.origin.is_some())
                    .map(|p| ProjectPluginInfo {
                        id: p.manifest.id.clone(),
                        version: p.manifest.version.clone(),
                    })
                    .collect(),
            })
        })();
        let outcome = match result {
            Ok(value) => ServiceOutcome::Success {
                value: json!(value),
            },
            Err(error) => ServiceOutcome::error("inspection_rejected", error),
        };
        self.reply_plugin_service(call, outcome);
    }

    pub(crate) fn inspect_plugin_preview(&mut self, preview: &mut phistudio_prpr::Preview) {
        // Only two bounded requests per rendered frame. prpr remains on its
        // graphics thread; linting, indexing and report generation run in Wasm.
        for _ in 0..2 {
            let Some(call) = self.plugins.inspections.pop_front() else {
                break;
            };
            let result = (|| -> Result<PreviewInspectionResult, String> {
                let request: PreviewInspection =
                    serde_json::from_value(call.request.arguments.clone())
                        .map_err(|e| e.to_string())?;
                request.validate()?;
                if request.version != self.plugin_version() || self.render_error.is_some() {
                    return Err("Native preview is unavailable for this document".into());
                }
                self.plugins
                    .runtime
                    .as_ref()
                    .ok_or("Plugin host stopped")?
                    .authorize_inspection(&call, request.version, "preview", Capability::Preview)?;
                preview.inspect_note(&request).map_err(|e| e.to_string())
            })();
            let outcome = match result {
                Ok(value) => ServiceOutcome::Success {
                    value: json!(value),
                },
                Err(error) => ServiceOutcome::error("inspection_rejected", error),
            };
            self.reply_plugin_service(call, outcome);
        }
    }
}
