//! Bounded source-JSON queries. A page is checked before any subtree is cloned.
use crate::{HostCall, Runtime, limits::serialized_size};
use phistudio_core::ChartDocument;
use phistudio_sdk::*;
use serde_json::{Value, json};

pub const MAX_QUERY_BYTES: usize = 256 * 1024;
pub const MAX_QUERY_ITEMS: usize = 4096;

impl Runtime {
    /// Resolve on the document-owning thread. Only the bounded result crosses
    /// back to the worker; neither the document nor a live reference escapes.
    pub fn document_service(
        &self,
        call: &HostCall,
        version: DocumentVersion,
        document: &ChartDocument,
    ) -> ServiceOutcome {
        let Ok(registry) = self.registry.lock() else {
            return ServiceOutcome::error("host_unavailable", "Plugin registry interrupted");
        };
        let Some(plugin) = registry.current(&call.origin) else {
            return ServiceOutcome::error("caller_inactive", "Service caller is no longer active");
        };
        if !registry.valid_scope(&call.origin, &call.required)
            || call
                .invocation()
                .is_some_and(|i| !i.belongs_to(&self.registry) || !i.is_active())
            || call.interaction().is_some_and(|i| !i.is_active())
            || call
                .operation()
                .is_some_and(|o| !o.belongs_to(&self.registry) || !o.current_in(&registry, true))
        {
            return ServiceOutcome::error("caller_inactive", "Service caller's activation changed");
        }
        if !plugin.granted.contains(&Capability::DocumentRead)
            || !plugin.granted.contains(&Capability::Services)
        {
            return ServiceOutcome::error(
                "permission_denied",
                "DocumentRead and Services are required",
            );
        }
        let service = &call.request.service;
        if service.provider != "host"
            || service.id != "document"
            || !service.version.matches(&Version::new(1, 0, 0))
        {
            return ServiceOutcome::error(
                "service_unavailable",
                "Requested host service version is unavailable",
            );
        }
        if call.context.version != version {
            return ServiceOutcome::error(
                "stale_document",
                "The document changed before this query ran",
            );
        }
        let result = if call.request.method == "validate" {
            validate_source(document, &call.request.arguments, version)
        } else {
            query(
                &call.request.method,
                &call.request.arguments,
                version,
                document.as_json(),
            )
        };
        match result {
            Ok(value) => ServiceOutcome::Success { value },
            Err((code, message)) => ServiceOutcome::error(code, message),
        }
    }
}

fn validate_source(
    document: &ChartDocument,
    arguments: &Value,
    version: DocumentVersion,
) -> Result<Value, QueryError> {
    use phistudio_core::SourceLocation;
    let request: DocumentQuery =
        serde_json::from_value(arguments.clone()).map_err(|e| invalid(e.to_string()))?;
    if request.version != version {
        return Err(("stale_document", "Validation document changed".into()));
    }
    validate_pointer(&request.path)?;
    let value = document
        .as_json()
        .pointer(&request.path)
        .ok_or(("path_not_found", "Validation path does not exist".into()))?;
    // Native validators may walk/deserialize their object. Keep each invocation
    // bounded by the same size limit as a normal document query.
    serialized_size(value, MAX_QUERY_BYTES - 512).ok_or_else(too_large)?;
    let result = if request.path == "/phiStudioExtra" {
        phistudio_core::shaders::validate_shader_extra(value)
    } else {
        match document
            .source_location(&request.path)
            .map_err(|e| invalid(e.to_string()))?
        {
            SourceLocation::Note(id) if request.path.split('/').count() == 5 => {
                document.note(id.line, id.index).and_then(|n| n.validate())
            }
            SourceLocation::Event(id) if request.path.split('/').count() == 7 => {
                document.event(id).and_then(|e| e.validate(id.kind))
            }
            SourceLocation::Noise(_) if request.path.split('/').count() == 3 => {
                phistudio_core::noise_fields::validate(value)
            }
            _ => {
                return Err(invalid(
                    "No native authoring validator exists for this source location",
                ));
            }
        }
    };
    Ok(json!(DocumentValidation {
        version,
        valid: result.is_ok(),
        message: result.err().map(|e| e.to_string())
    }))
}

type QueryError = (&'static str, String);
fn invalid(message: impl Into<String>) -> QueryError {
    ("invalid_arguments", message.into())
}
fn too_large() -> QueryError {
    (
        "response_too_large",
        "Query value exceeds 256 KiB; query a smaller subtree".into(),
    )
}

fn query(
    method: &str,
    arguments: &Value,
    version: DocumentVersion,
    root: &Value,
) -> Result<Value, QueryError> {
    if matches!(method, "fields" | "rows") {
        return projected_query(method, arguments, version, root);
    }
    if method == "plugin_data" {
        let args: PluginDataQuery =
            serde_json::from_value(arguments.clone()).map_err(|e| invalid(e.to_string()))?;
        if args.version != version {
            return Err((
                "stale_document",
                "Plugin data query does not match the current document".into(),
            ));
        }
        plugin_data_path(&args.plugin).map_err(invalid)?;
        let namespace = root.get(PLUGIN_DATA_KEY);
        if namespace.is_some_and(|v| !v.is_object()) {
            return Err((
                "invalid_document",
                "Plugin data container is not an object".into(),
            ));
        }
        let entry = namespace.and_then(|v| v.get(&args.plugin));
        if let Some(entry) = entry {
            serialized_size(entry, MAX_QUERY_BYTES - 512).ok_or_else(too_large)?;
        }
        return Ok(json!(PluginDataValue {
            version,
            present: entry.is_some(),
            entry: entry.cloned().unwrap_or(Value::Null)
        }));
    }
    let paged = matches!(method, "slice" | "keys");
    let (requested, path, page) = if paged {
        let page: DocumentPage =
            serde_json::from_value(arguments.clone()).map_err(|e| invalid(e.to_string()))?;
        if page.limit == 0 || page.limit > MAX_QUERY_ITEMS {
            return Err(invalid("Query limit must be between 1 and 4096"));
        }
        (page.version, page.path.clone(), Some(page))
    } else if matches!(method, "read" | "inspect") {
        let request: DocumentQuery =
            serde_json::from_value(arguments.clone()).map_err(|e| invalid(e.to_string()))?;
        (request.version, request.path, None)
    } else {
        return Err(("method_not_found", "Unknown document query method".into()));
    };
    if requested != version {
        return Err((
            "stale_document",
            "Query version does not match the current document".into(),
        ));
    }
    validate_pointer(&path)?;
    let value = root
        .pointer(&path)
        .ok_or_else(|| ("path_not_found", "JSON pointer does not exist".into()))?;
    match method {
        "read" => {
            serialized_size(value, MAX_QUERY_BYTES - 512).ok_or_else(too_large)?;
            Ok(json!(DocumentValue {
                version,
                value: value.clone()
            }))
        }
        "inspect" => {
            let (kind, length) = match value {
                Value::Null => ("null", None),
                Value::Bool(_) => ("boolean", None),
                Value::Number(_) => ("number", None),
                Value::String(v) => ("string", Some(v.len())),
                Value::Array(v) => ("array", Some(v.len())),
                Value::Object(v) => ("object", Some(v.len())),
            };
            Ok(json!({"version":version,"kind":kind,"length":length}))
        }
        "slice" => {
            let array = value
                .as_array()
                .ok_or_else(|| invalid("slice requires an array"))?;
            let page = page.ok_or_else(|| invalid("Missing page arguments"))?;
            collect_page(array.iter().skip(page.offset), array.len(), page)
        }
        "keys" => {
            let object = value
                .as_object()
                .ok_or_else(|| invalid("keys requires an object"))?;
            let page = page.ok_or_else(|| invalid("Missing page arguments"))?;
            let mut remaining = MAX_QUERY_BYTES - 512;
            let mut keys = vec![];
            for key in object.keys().skip(page.offset).take(page.limit) {
                let Some(size) = serialized_size(key, remaining.saturating_sub(1)) else {
                    if keys.is_empty() {
                        return Err(too_large());
                    }
                    break;
                };
                remaining -= size + 1;
                keys.push(Value::String(key.clone()));
            }
            page_result(keys, object.len(), page)
        }
        _ => Err(invalid("Unknown query method")),
    }
}

fn projected_query(
    method: &str,
    arguments: &Value,
    version: DocumentVersion,
    root: &Value,
) -> Result<Value, QueryError> {
    let (requested, path, fields, page) = if method == "fields" {
        let request: DocumentFields =
            serde_json::from_value(arguments.clone()).map_err(|e| invalid(e.to_string()))?;
        (request.version, request.path, request.fields, None)
    } else {
        let request: DocumentRows =
            serde_json::from_value(arguments.clone()).map_err(|e| invalid(e.to_string()))?;
        if !(1..=MAX_QUERY_ITEMS).contains(&request.limit) {
            return Err(invalid("Query limit must be between 1 and 4096"));
        }
        let page = DocumentPage {
            version: request.version,
            path: request.path.clone(),
            offset: request.offset,
            limit: request.limit,
        };
        (request.version, request.path, request.fields, Some(page))
    };
    if requested != version {
        return Err((
            "stale_document",
            "Projection does not match the current document".into(),
        ));
    }
    validate_pointer(&path)?;
    if fields.is_empty()
        || fields.len() > 64
        || fields.iter().any(|key| key.len() > 1024)
        || fields
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != fields.len()
    {
        return Err(invalid(
            "Select 1–64 unique literal field names of at most 1024 bytes",
        ));
    }
    let value = root
        .pointer(&path)
        .ok_or_else(|| ("path_not_found", "JSON pointer does not exist".into()))?;
    if let Some(page) = page {
        let array = value
            .as_array()
            .ok_or_else(|| invalid("rows requires an array"))?;
        let mut remaining = MAX_QUERY_BYTES - 512;
        let mut items = vec![];
        for row in array.iter().skip(page.offset).take(page.limit) {
            let projected = match project_fields(row, &fields, remaining.saturating_sub(1)) {
                Ok(value) => value,
                Err(error) if error.0 == "response_too_large" && !items.is_empty() => break,
                Err(error) => return Err(error),
            };
            let size =
                serialized_size(&projected, remaining.saturating_sub(1)).ok_or_else(too_large)?;
            remaining -= size + 1;
            items.push(projected);
        }
        page_result(items, array.len(), page)
    } else {
        if !value.is_object() {
            return Err(invalid("fields requires an object"));
        }
        Ok(json!(DocumentValue {
            version,
            value: project_fields(value, &fields, MAX_QUERY_BYTES - 512)?,
        }))
    }
}

fn project_fields(value: &Value, fields: &[String], limit: usize) -> Result<Value, QueryError> {
    let Some(object) = value.as_object() else {
        serialized_size(value, limit).ok_or_else(too_large)?;
        return Ok(value.clone());
    };
    let selected: std::collections::BTreeMap<_, _> = fields
        .iter()
        .filter_map(|key| object.get(key).map(|value| (key.as_str(), value)))
        .collect();
    // Size the borrowed selection before cloning any selected subtree.
    serialized_size(&selected, limit).ok_or_else(too_large)?;
    Ok(Value::Object(
        selected
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.clone()))
            .collect(),
    ))
}

fn collect_page<'a>(
    values: impl Iterator<Item = &'a Value>,
    total: usize,
    page: DocumentPage,
) -> Result<Value, QueryError> {
    let mut remaining = MAX_QUERY_BYTES - 512;
    let mut items = vec![];
    for value in values.take(page.limit) {
        let Some(size) = serialized_size(value, remaining.saturating_sub(1)) else {
            if items.is_empty() {
                return Err(too_large());
            }
            break;
        };
        remaining -= size + 1;
        items.push(value.clone());
    }
    page_result(items, total, page)
}

fn page_result(items: Vec<Value>, total: usize, page: DocumentPage) -> Result<Value, QueryError> {
    let next = page.offset.saturating_add(items.len());
    Ok(json!(DocumentItems {
        version: page.version,
        offset: page.offset,
        total,
        next_offset: (next < total).then_some(next),
        items
    }))
}

fn validate_pointer(path: &str) -> Result<(), QueryError> {
    if path.len() > 4096 || (!path.is_empty() && !path.starts_with('/')) {
        return Err(invalid("Use an RFC 6901 pointer of at most 4096 bytes"));
    }
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        if c == '~' && !matches!(chars.next(), Some('0' | '1')) {
            return Err(invalid("Invalid JSON pointer escape"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_validation_is_readonly_versioned_and_bounded_to_whole_objects() {
        use phistudio_core::{Beat, noise_fields};
        let version = DocumentVersion {
            session: 1,
            revision: 0,
        };
        let document = ChartDocument::empty("Native validation", 120.).unwrap();
        let mut field = noise_fields::create(
            Beat::ZERO,
            Beat::ONE,
            noise_fields::Format::Native,
            false,
            document.tempo(),
        );
        field["phiStudioColor"] = json!("invalid");
        let mut source = document.as_json().clone();
        source["blockAreaList"] = json!([field]);
        let document = ChartDocument::from_value(source.clone()).unwrap();
        let args = json!(DocumentQuery {
            version,
            path: "/blockAreaList/0".into()
        });
        let result: DocumentValidation =
            serde_json::from_value(validate_source(&document, &args, version).unwrap()).unwrap();
        assert!(!result.valid);
        assert!(result.message.is_some());
        assert_eq!(document.as_json(), &source);
        let nested = json!(DocumentQuery {
            version,
            path: "/blockAreaList/0/phiStudioColor".into()
        });
        assert_eq!(
            validate_source(&document, &nested, version).unwrap_err().0,
            "invalid_arguments"
        );
        let stale = DocumentVersion {
            revision: 1,
            ..version
        };
        assert_eq!(
            validate_source(&document, &args, stale).unwrap_err().0,
            "stale_document"
        );
        source["blockAreaList"][0]["foreignBlob"] = json!("x".repeat(MAX_QUERY_BYTES));
        let document = ChartDocument::from_value(source).unwrap();
        assert_eq!(
            validate_source(&document, &args, version).unwrap_err().0,
            "response_too_large"
        );
    }
    #[test]
    fn projections_skip_large_unknown_fields_and_preserve_missing_and_null() {
        let version = DocumentVersion {
            session: 1,
            revision: 2,
        };
        let root = json!({"rows":[{"type":1,"startTime":[0,0,1],"explicit":null,"unknown":"x".repeat(MAX_QUERY_BYTES * 2)},null],"META":{"offset":0,"future":"x".repeat(MAX_QUERY_BYTES * 2)}});
        let request = json!(DocumentRows {
            version,
            path: "/rows".into(),
            offset: 0,
            limit: 10,
            fields: vec![
                "type".into(),
                "startTime".into(),
                "explicit".into(),
                "missing".into()
            ],
        });
        let result = query("rows", &request, version, &root).unwrap();
        assert_eq!(
            result["items"],
            json!([{"type":1,"startTime":[0,0,1],"explicit":null},null])
        );
        assert!(result["next_offset"].is_null());
        assert_eq!(
            query(
                "fields",
                &json!(DocumentFields {
                    version,
                    path: "/META".into(),
                    fields: vec!["offset".into()],
                }),
                version,
                &root
            )
            .unwrap()["value"],
            json!({"offset":0})
        );
        assert_eq!(
            root["rows"][0]["unknown"].as_str().unwrap().len(),
            MAX_QUERY_BYTES * 2
        );
    }

    #[test]
    fn projection_pages_retain_byte_bounds_and_validate_arguments_and_versions() {
        let version = DocumentVersion::default();
        let root = json!({"rows":[{"v":"x".repeat(180_000)},{"v":"y".repeat(180_000)}]});
        let mut request = DocumentRows {
            version,
            path: "/rows".into(),
            offset: 0,
            limit: 4,
            fields: vec!["v".into()],
        };
        let page = query("rows", &json!(request), version, &root).unwrap();
        assert_eq!(page["items"].as_array().unwrap().len(), 1);
        assert_eq!(page["next_offset"], 1);
        request.offset = 1;
        assert!(query("rows", &json!(request), version, &root).unwrap()["next_offset"].is_null());
        request.version.revision += 1;
        assert_eq!(
            query("rows", &json!(request), version, &root)
                .unwrap_err()
                .0,
            "stale_document"
        );
        request.version = version;
        request.fields = vec!["v".into(); 2];
        assert_eq!(
            query("rows", &json!(request), version, &root)
                .unwrap_err()
                .0,
            "invalid_arguments"
        );
        request.fields.clear();
        assert!(query("rows", &json!(request), version, &root).is_err());
    }
    #[test]
    fn plugin_data_queries_distinguish_missing_null_legacy_and_bounded_entries() {
        let version = DocumentVersion::default();
        let root = json!({PLUGIN_DATA_KEY:{"test.null":null,"test.legacy":{"future":true},"test.huge":"x".repeat(MAX_QUERY_BYTES)}});
        let request = |plugin: &str| {
            json!(PluginDataQuery {
                version,
                plugin: plugin.into()
            })
        };
        let missing = query("plugin_data", &request("test.missing"), version, &root).unwrap();
        assert_eq!(missing["present"], false);
        let null = query("plugin_data", &request("test.null"), version, &root).unwrap();
        assert_eq!(null["present"], true);
        assert_eq!(null["entry"], Value::Null);
        assert_eq!(
            query("plugin_data", &request("test.legacy"), version, &root).unwrap()["entry"],
            json!({"future":true})
        );
        assert_eq!(
            query("plugin_data", &request("test.huge"), version, &root)
                .unwrap_err()
                .0,
            "response_too_large"
        );
        assert_eq!(
            query("plugin_data", &request("../invalid"), version, &root)
                .unwrap_err()
                .0,
            "invalid_arguments"
        );
        assert_eq!(
            query(
                "plugin_data",
                &request("test.null"),
                version,
                &json!({PLUGIN_DATA_KEY:42})
            )
            .unwrap_err()
            .0,
            "invalid_document"
        );
        let stale = DocumentVersion {
            session: 1,
            revision: 0,
        };
        assert_eq!(
            query(
                "plugin_data",
                &json!(PluginDataQuery {
                    version: stale,
                    plugin: "test.null".into()
                }),
                version,
                &root
            )
            .unwrap_err()
            .0,
            "stale_document"
        );
    }
    #[test]
    fn pages_are_bounded_before_cloning_and_keep_source_values_and_keys() {
        let version = DocumentVersion {
            session: 3,
            revision: 7,
        };
        let root = json!({"a/b~": vec![json!({"unknown":true,"text":"x".repeat(90_000)}); 5],
            "object":{"first":{"huge":"z".repeat(MAX_QUERY_BYTES)},"second":null}});
        let mut args = json!({"version":version,"path":"/a~1b~0","offset":0,"limit":5});
        let page = query("slice", &args, version, &root).unwrap();
        assert_eq!(page["items"].as_array().unwrap().len(), 2);
        assert_eq!(page["items"][0]["unknown"], true);
        assert_eq!(page["next_offset"], 2);
        assert!(serde_json::to_vec(&page).unwrap().len() <= MAX_QUERY_BYTES);
        args["offset"] = json!(4);
        let last = query("slice", &args, version, &root).unwrap();
        assert_eq!(last["next_offset"], Value::Null);
        let keys = query(
            "keys",
            &json!({"version":version,"path":"/object","offset":0,"limit":2}),
            version,
            &root,
        )
        .unwrap();
        assert_eq!(keys["items"], json!(["first", "second"]));
    }
    #[test]
    fn invalid_paths_versions_types_and_unbounded_reads_return_errors() {
        let version = DocumentVersion::default();
        let root = json!({"a":["x".repeat(MAX_QUERY_BYTES)],"a~2":1});
        for (method, path, code) in [
            ("read", "/a", "response_too_large"),
            ("slice", "/a", "response_too_large"),
            ("read", "/a~2", "invalid_arguments"),
            ("read", "missing", "invalid_arguments"),
            ("read", "/missing", "path_not_found"),
            ("keys", "/a", "invalid_arguments"),
        ] {
            let mut args = json!({"version":version,"path":path});
            if method == "slice" || method == "keys" {
                args["offset"] = json!(0);
                args["limit"] = json!(1);
            }
            assert_eq!(query(method, &args, version, &root).unwrap_err().0, code);
        }
        assert_eq!(
            query(
                "inspect",
                &json!({"version":{"session":1,"revision":0},"path":""}),
                version,
                &root
            )
            .unwrap_err()
            .0,
            "stale_document"
        );
    }
}
