//! Bounded reads of the original decoded clip; spectral work stays on Guide's worker.
use super::*;
use phistudio_sdk::*;

impl Workbench {
    pub(super) fn plugin_audio_service(&mut self, call: HostCall) {
        let result = (|| -> Result<Value, String> {
            let runtime = self.plugins.runtime.as_ref().ok_or("Plugin host stopped")?;
            runtime.authorize_audio_service(&call, self.plugin_version())?;
            let assets = ProjectAssetsVersion {
                session: self.session,
                revision: self.asset_revision,
            };
            if !self.audio_guide.current(
                self.session,
                self.asset_revision,
                self.project_options.music.as_deref(),
            ) {
                return Err("Decoded music is unavailable for this project".into());
            }
            let parse = |value: Value| serde_json::from_value(value).map_err(|e| e.to_string());
            match call.request.method.as_str() {
                "info" | "status" | "analyze" | "cancel" => {
                    let query: AudioQuery = parse(call.request.arguments.clone())?;
                    if query.version != self.plugin_version()
                        || query.assets.is_some_and(|a| a != assets)
                    {
                        return Err("Audio query is stale".into());
                    }
                    if matches!(call.request.method.as_str(), "analyze" | "cancel") {
                        self.plugins
                            .audio_requests
                            .retain(|(owner, _)| owner != &call.origin);
                        if call.request.method == "analyze" {
                            self.audio_guide.plugin_analyze();
                            self.plugins
                                .audio_requests
                                .push((call.origin.clone(), assets));
                        }
                    }
                    self.audio_guide.plugin_info().map(|info| json!(info))
                }
                "pcm" => {
                    let query: AudioPcmQuery =
                        serde_json::from_value(call.request.arguments.clone())
                            .map_err(|e| e.to_string())?;
                    if query.version != self.plugin_version() || query.source.assets != assets {
                        return Err("Audio query is stale".into());
                    }
                    self.audio_guide.plugin_pcm(query).map(|page| json!(page))
                }
                "waveform" => {
                    let query: AudioWaveformQuery =
                        serde_json::from_value(call.request.arguments.clone())
                            .map_err(|e| e.to_string())?;
                    if query.version != self.plugin_version() || query.source.assets != assets {
                        return Err("Audio query is stale".into());
                    }
                    self.audio_guide
                        .plugin_waveform(query)
                        .map(|page| json!(page))
                }
                "candidates" => {
                    let query: AudioCandidatesQuery =
                        serde_json::from_value(call.request.arguments.clone())
                            .map_err(|e| e.to_string())?;
                    if query.version != self.plugin_version() || query.source.assets != assets {
                        return Err("Audio query is stale".into());
                    }
                    self.audio_guide
                        .plugin_candidates(query)
                        .map(|page| json!(page))
                }
                _ => Err("Unknown audio service method".into()),
            }
        })();
        self.reply_plugin_service(
            call,
            match result {
                Ok(value) => ServiceOutcome::Success { value },
                Err(error) => ServiceOutcome::error("audio_rejected", error),
            },
        );
    }
}
