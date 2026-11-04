use super::*;

impl Worker {
    pub(super) fn feature_service(&self, origin: &Origin, call: &ServiceCall) -> ServiceOutcome {
        if !self.host.is_current(origin) {
            return ServiceOutcome::error("revoked", "Plugin activation is no longer current");
        }
        if !call.service.version.matches(&Version::new(1, 0, 0)) || call.method != "get" {
            return ServiceOutcome::error("unsupported", "Use host/features v1 get");
        }
        if !call.arguments.is_null() && call.arguments != serde_json::json!({}) {
            return ServiceOutcome::error(
                "invalid_arguments",
                "Feature discovery takes no arguments",
            );
        }
        let Ok(editor_version) = Version::parse(env!("CARGO_PKG_VERSION")) else {
            return ServiceOutcome::error("host_version", "Editor version is invalid");
        };
        let Some(info) = self
            .host
            .snapshot()
            .into_iter()
            .find(|p| p.origin.as_ref() == Some(origin))
        else {
            return ServiceOutcome::error("revoked", "Plugin activation is no longer current");
        };
        if call.service.id == "diagnostics" {
            return match serde_json::to_value(info.diagnostics) {
                Ok(value) => ServiceOutcome::Success { value },
                Err(_) => {
                    ServiceOutcome::error("host_diagnostics", "Diagnostics could not be encoded")
                }
            };
        }
        let mut services: BTreeMap<String, Version> = [
            "features",
            "diagnostics",
            "assets",
            "files",
            "network",
            "websocket",
        ]
        .map(|s| (s.into(), Version::new(1, 0, 0)))
        .into();
        if self.config.credentials.supported() {
            services.insert("secrets".into(), Version::new(1, 0, 0));
        }
        services.extend(
            self.config
                .application_services
                .iter()
                .map(|s| (s.clone(), Version::new(1, 0, 0))),
        );
        let features = HostFeatures {
            api: API_VERSION,
            editor_version,
            platform: std::env::consts::OS.into(),
            granted: info.granted,
            services,
            widgets: [
                "label", "button", "bool", "number", "text", "choice", "row", "column", "canvas",
                "password", "table", "tree", "tabs",
            ]
            .map(String::from)
            .into(),
            extensions: ["ui.dialog", "editor.context_menu", "timeline.embedded"]
                .map(String::from)
                .into(),
            limits: [
                ("message_bytes".into(), MAX_MESSAGE_BYTES as u64),
                ("document_query_items".into(), crate::MAX_QUERY_ITEMS as u64),
                ("file_chunk_bytes".into(), MAX_FILE_CHUNK_BYTES as u64),
                ("network_response_bytes".into(), MAX_NETWORK_RESPONSE_BYTES),
                ("network_upload_bytes".into(), MAX_NETWORK_BODY_BYTES as u64),
                (
                    "network_inline_bytes".into(),
                    MAX_NETWORK_INLINE_BYTES as u64,
                ),
                (
                    "websocket_message_bytes".into(),
                    MAX_WEBSOCKET_MESSAGE_BYTES as u64,
                ),
                ("secret_value_bytes".into(), 16 * 1024),
                ("secret_records_per_owner".into(), 64),
                ("audio_pcm_frames".into(), MAX_PCM_FRAMES as u64),
                ("audio_waveform_buckets".into(), MAX_WAVEFORM_BUCKETS as u64),
                ("audio_candidates".into(), MAX_AUDIO_CANDIDATES as u64),
            ]
            .into(),
        };
        match serde_json::to_value(features) {
            Ok(value) => ServiceOutcome::Success { value },
            Err(_) => {
                ServiceOutcome::error("host_features", "Feature metadata could not be encoded")
            }
        }
    }
}
