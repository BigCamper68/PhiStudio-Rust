use super::*;
use phistudio_core::ChartDocument;

struct DocumentConverter(Option<ProviderFiles>);
impl Plugin for DocumentConverter {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        let effect = match request {
            Request::Activate { .. } => {
                return Ok(Response {
                    contributions: vec![Contribution::Format {
                        id: "copy".into(),
                        title: "Document converter".into(),
                        extensions: vec!["copy".into()],
                        bridge: FormatBridge::Chart,
                        decode: true,
                        encode: true,
                    }],
                    ..Default::default()
                });
            }
            Request::Format { files, context, .. } => {
                self.0 = Some(files.clone());
                assert!(context.chart.is_none());
                Effect::Call {
                    call: ServiceCall {
                        request_id: "document".into(),
                        service: ServiceRef {
                            provider: "host".into(),
                            id: "document".into(),
                            version: "^1".parse().unwrap(),
                        },
                        method: "read".into(),
                        arguments: json!(DocumentQuery {
                            version: context.version,
                            path: "/META/name".into()
                        }),
                    },
                }
            }
            Request::ServiceResult { reply, .. } if reply.request_id == "document" => {
                let ServiceOutcome::Success { value } = &reply.outcome else {
                    panic!("Query failed")
                };
                let files = self.0.as_ref().unwrap();
                Effect::Call {
                    call: ServiceCall {
                        request_id: "write".into(),
                        service: ServiceRef {
                            provider: "host".into(),
                            id: "files".into(),
                            version: "^1".parse().unwrap(),
                        },
                        method: "write".into(),
                        arguments: json!(FileWrite {
                            operation: files.operation,
                            file: files.output,
                            offset: None,
                            data: encode_file_chunk(value["value"].as_str().unwrap().as_bytes())
                                .unwrap()
                        }),
                    },
                }
            }
            Request::ServiceResult { reply, .. } => {
                assert!(matches!(reply.outcome, ServiceOutcome::Success { .. }));
                let files = self.0.as_ref().unwrap();
                Effect::Complete {
                    operation: files.operation,
                    file: files.output,
                    name: "document.copy".into(),
                }
            }
            _ => return Ok(Response::default()),
        };
        Ok(Response {
            effects: vec![effect],
            ..Default::default()
        })
    }
}
fn document_runtime() -> Runtime {
    let manifest = serde_json::from_value(json!({"api":1,"id":"test.document-converter","name":"Document converter","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["formats","services","document_read"]})).unwrap();
    let runtime = Runtime::spawn(
        None,
        vec![Builtin {
            manifest,
            enabled: true,
            create: Arc::new(|| Box::new(DocumentConverter(None))),
        }],
    )
    .unwrap();
    wait(&runtime, 0).unwrap();
    runtime
}

#[test]
fn provider_scope_survives_document_roundtrips_and_revokes_pending_queries_on_cancel() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    std::fs::write(&source, b"source").unwrap();
    let document = ChartDocument::empty("Document continuation", 120.).unwrap();
    for scenario in ["complete", "cancel", "foreign"] {
        let mut runtime = document_runtime();
        let scope = operation(&mut runtime, false);
        let id = start(&mut runtime, &scope, &source);
        let mut output = wait(&runtime, id).unwrap();
        assert!(output.artifact.is_none());
        assert!(scope.is_running());
        assert_eq!(output.calls.len(), 1);
        let call = output.calls.pop().unwrap();
        assert_eq!(call.operation(), Some(&scope));
        let mut other = document_runtime();
        assert!(!other.is_output_current(&output));
        assert!(!other.is_call_current(&call));
        assert!(matches!(
            other.document_service(&call, context().version, &document),
            ServiceOutcome::Error { .. }
        ));
        if scenario == "foreign" {
            assert!(
                other
                    .submit(Action::ServiceReply {
                        call,
                        outcome: ServiceOutcome::Success { value: Value::Null }
                    })
                    .is_err()
            );
            other.shutdown().unwrap();
            runtime.shutdown().unwrap();
            continue;
        }
        other.shutdown().unwrap();
        if scenario == "cancel" {
            scope.cancel();
            assert!(!runtime.is_call_current(&call));
            assert!(!runtime.is_output_current(&output));
            assert!(
                matches!(runtime.document_service(&call,context().version,&document), ServiceOutcome::Error {code,..} if code == "caller_inactive")
            );
            assert!(
                runtime
                    .submit(Action::ServiceReply {
                        call,
                        outcome: ServiceOutcome::Success { value: Value::Null }
                    })
                    .is_err()
            );
        } else {
            assert!(
                matches!(runtime.document_service(&call,DocumentVersion::default(),&document), ServiceOutcome::Error {code,..} if code == "stale_document")
            );
            let outcome = runtime.document_service(&call, context().version, &document);
            let id = runtime
                .submit(Action::ServiceReply { call, outcome })
                .unwrap();
            let output = wait(&runtime, id).unwrap();
            assert!(runtime.is_output_current(&output));
            assert_eq!(
                std::fs::read(output.artifact.unwrap().path()).unwrap(),
                b"Document continuation"
            );
            assert!(!scope.is_running());
        }
        runtime.shutdown().unwrap();
    }
}
