use super::*;
use phistudio_core::{ChartDocument, ProjectArchive, import_project, load_chart};
use std::io::Write;
use zip::{ZipWriter, write::SimpleFileOptions};

struct Checks {
    files: Option<ProviderFiles>,
    step: usize,
    input: u64,
    output: u64,
    scratch: u64,
    chart: u64,
}
impl Checks {
    fn next(&self) -> Response {
        let files = self.files.as_ref().unwrap();
        let op = files.operation;
        let write = |file, data: &[u8]| {
            json!(FileWrite {
                operation: op,
                file,
                offset: None,
                data: encode_file_chunk(data).unwrap()
            })
        };
        let resize = |file, size| {
            json!(FileResize {
                operation: op,
                file,
                size
            })
        };
        let path = |path: &str| {
            json!(PackagePath {
                operation: op,
                path: path.into()
            })
        };
        let remove = |file| {
            json!(PackageRemove {
                operation: op,
                file
            })
        };
        let (method, args) = match self.step {
            0 => (
                "entries",
                json!(PackageEntries {
                    operation: op,
                    output: false,
                    offset: 0,
                    limit: 1
                }),
            ),
            1 => ("open_entry", path("data/value.bin")),
            2 => (
                "read",
                json!(FileRead {
                    operation: op,
                    file: self.input,
                    offset: 0,
                    limit: 3
                }),
            ),
            3 => ("write", write(self.input, b"bad")),
            4 => ("resize", resize(self.input, 0)),
            5 => ("create_entry", path("../escape")),
            6 => ("create_entry", path("output/data.bin")),
            7 => ("write", write(self.output, b"12345")),
            8 => (
                "rename_entry",
                json!(PackageRename {
                    operation: op,
                    file: self.output,
                    path: "changed/data.bin".into()
                }),
            ),
            9 => ("create_entry", path("changed")),
            10 => ("create_entry", path("changed/data.bin/child")),
            11 => ("create_entry", path("changed/./data.bin")),
            12 => ("resize", resize(self.output, 2)),
            13 => (
                "entries",
                json!(PackageEntries {
                    operation: op,
                    output: true,
                    offset: 0,
                    limit: 256
                }),
            ),
            14 => ("remove_entry", remove(self.input)),
            15 => ("close_entry", remove(self.input)),
            16 => (
                "read",
                json!(FileRead {
                    operation: op,
                    file: self.input,
                    offset: 0,
                    limit: 1
                }),
            ),
            17 | 23 => ("create_entry", path("discard")),
            18 | 28 => ("remove_entry", remove(self.scratch)),
            19 => (
                "read",
                json!(FileRead {
                    operation: op,
                    file: self.scratch,
                    offset: 0,
                    limit: 1
                }),
            ),
            20 => ("create_entry", path("chart.json")),
            21 => (
                "write",
                write(
                    self.chart,
                    &serde_json::to_vec(
                        ChartDocument::empty("Package output", 120.)
                            .unwrap()
                            .as_json(),
                    )
                    .unwrap(),
                ),
            ),
            22 => ("resize", resize(self.output, MAX_PROVIDER_FILE_BYTES)),
            24 => ("resize", resize(self.scratch, MAX_PROVIDER_FILE_BYTES)),
            25 => ("resize", resize(self.output, MAX_PROVIDER_FILE_BYTES + 1)),
            26 => ("resize", resize(self.output, 2)),
            27 => ("resize", resize(self.scratch, 0)),
            29 => ("pack", json!(PackagePack { operation: op + 1 })),
            30 => ("pack", json!(PackagePack { operation: op })),
            _ => {
                return Response {
                    effects: vec![Effect::Complete {
                        operation: op,
                        file: files.output,
                        name: "project.pez".into(),
                    }],
                    ..Default::default()
                };
            }
        };
        Response {
            effects: vec![Effect::Call {
                call: ServiceCall {
                    request_id: "step".into(),
                    service: ServiceRef {
                        provider: "host".into(),
                        id: "files".into(),
                        version: "^1".parse().unwrap(),
                    },
                    method: method.into(),
                    arguments: args,
                },
            }],
            ..Default::default()
        }
    }
}
impl Plugin for Checks {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        match request {
            Request::Activate { .. } => {
                return Ok(Response {
                    contributions: vec![Contribution::Format {
                        id: "project".into(),
                        title: "Project test".into(),
                        extensions: vec!["ptest".into()],
                        bridge: FormatBridge::Project,
                        decode: true,
                        encode: true,
                    }],
                    ..Default::default()
                });
            }
            Request::Format { bridge, files, .. } => {
                assert_eq!(*bridge, FormatBridge::Project);
                self.files = Some(files.clone());
                return Ok(self.next());
            }
            Request::ServiceResult { reply, .. } => {
                let expected = match self.step {
                    3 | 4 => Some("writable"),
                    5 => Some("Invalid path"),
                    9..=11 => Some("conflicts"),
                    14 => Some("removed"),
                    16 | 19 => Some("does not belong"),
                    24 => Some("size limit"),
                    25 => Some("resize"),
                    29 => Some("operation"),
                    _ => None,
                };
                match (&reply.outcome, expected) {
                    (ServiceOutcome::Error { message, .. }, Some(text)) => {
                        assert!(message.contains(text), "Step {}: {message}", self.step)
                    }
                    (ServiceOutcome::Success { value }, None) => match self.step {
                        0 => {
                            let page: PackagePage = serde_json::from_value(value.clone()).unwrap();
                            assert_eq!(page.total, 2);
                            assert_eq!(page.next_offset, Some(1));
                            assert_eq!(page.entries[0].path, "chart.json");
                        }
                        1 | 6 | 17 | 20 | 23 => {
                            let file: ProviderFile = serde_json::from_value(value.clone()).unwrap();
                            assert!(file.id >= 3);
                            match self.step {
                                1 => self.input = file.id,
                                6 => self.output = file.id,
                                20 => self.chart = file.id,
                                _ => self.scratch = file.id,
                            }
                        }
                        2 => assert_eq!(
                            decode_file_chunk(value["data"].as_str().unwrap()).unwrap(),
                            b"raw"
                        ),
                        8 => assert_eq!(value["id"], self.output),
                        13 => assert_eq!(
                            value["entries"],
                            json!([{"path":"changed/data.bin","size":2}])
                        ),
                        30 => assert!(value["size"].as_u64().unwrap() > 0),
                        _ => {}
                    },
                    _ => panic!(
                        "Unexpected package result at {}: {:?}",
                        self.step, reply.outcome
                    ),
                }
                self.step += 1;
                return Ok(self.next());
            }
            _ => {}
        }
        Ok(Response::default())
    }
}
fn builtin() -> Builtin {
    Builtin {manifest:serde_json::from_value(json!({"api":1,"id":"test.package","name":"Package","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["formats","services","document_read"]})).unwrap(),enabled:true,create:Arc::new(||Box::new(Checks {files:None,step:0,input:0,output:0,scratch:0,chart:0}))}
}

#[test]
fn project_services_bound_paths_handles_and_sizes_and_pack_native_importable_data() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.zip");
    let mut zip = ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = SimpleFileOptions::default();
    zip.start_file("chart.json", options).unwrap();
    zip.write_all(
        &serde_json::to_vec(ChartDocument::empty("Input", 120.).unwrap().as_json()).unwrap(),
    )
    .unwrap();
    zip.start_file("data/value.bin", options).unwrap();
    zip.write_all(b"raw").unwrap();
    zip.finish().unwrap();
    let mut runtime = Runtime::spawn(Some(root.path().into()), vec![builtin()]).unwrap();
    wait(&runtime, 0).unwrap();
    let origin = runtime.registry().unwrap().plugins[0]
        .origin
        .clone()
        .unwrap();
    let scope = runtime
        .begin_provider(
            &origin,
            Provider::Format {
                id: "project".into(),
                operation: FormatOperation::Decode,
            },
            context().version,
        )
        .unwrap();
    assert_eq!(scope.format_bridge(), Some(FormatBridge::Project));
    let id = start(&mut runtime, &scope, &path);
    let output = wait(&runtime, id).unwrap();
    let artifact = output.artifact.unwrap();
    assert!(ProjectArchive::open(artifact.path()).is_ok());
    let project = import_project(
        artifact.path(),
        &root.path().join("projects"),
        "Package",
        true,
    )
    .unwrap();
    assert_eq!(
        load_chart(&project.charts[project.suggested.unwrap()].path)
            .unwrap()
            .name(),
        "Package output"
    );
    assert_eq!(
        std::fs::read(project.assets.join("changed/data.bin")).unwrap(),
        b"12"
    );
    assert_eq!(
        runtime.registry().unwrap().plugins[0].status,
        Status::Enabled
    );
    let private = artifact.path().to_owned();
    drop(artifact);
    assert!(!private.exists());
    runtime.shutdown().unwrap();
}

#[test]
fn project_export_requires_project_read_before_a_source_can_be_submitted() {
    let mut runtime = Runtime::spawn(None, vec![builtin()]).unwrap();
    wait(&runtime, 0).unwrap();
    let origin = runtime.registry().unwrap().plugins[0]
        .origin
        .clone()
        .unwrap();
    assert!(
        runtime
            .begin_provider(
                &origin,
                Provider::Format {
                    id: "project".into(),
                    operation: FormatOperation::Encode
                },
                context().version
            )
            .unwrap_err()
            .contains("ProjectRead")
    );
    let mut host = Host::default();
    let registration = builtin();
    host.install(
        registration.manifest.clone(),
        (registration.create)(),
        registration.manifest.capabilities,
    )
    .unwrap();
    host.enable("test.package").unwrap();
    assert!(
        host.provider(
            "test.package",
            &Provider::Format {
                id: "project".into(),
                operation: FormatOperation::Encode
            },
            ProviderFiles {
                operation: 1,
                input: ProviderFile {
                    id: 1,
                    name: "project.pez".into(),
                    size: 0
                },
                output: 2
            },
            context()
        )
        .unwrap_err()
        .to_string()
        .contains("ProjectRead")
    );
    assert_eq!(
        runtime.registry().unwrap().plugins[0].status,
        Status::Enabled
    );
    runtime.shutdown().unwrap();
}

struct ManyEntries {
    files: Option<ProviderFiles>,
    step: usize,
}
impl Plugin for ManyEntries {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        match request {
            Request::Activate { .. } => {
                return Checks {
                    files: None,
                    step: 0,
                    input: 0,
                    output: 0,
                    scratch: 0,
                    chart: 0,
                }
                .call(request);
            }
            Request::Format { files, .. } => self.files = Some(files.clone()),
            Request::ServiceResult { reply, .. } => {
                if self.step == MAX_PACKAGE_ENTRIES {
                    assert!(
                        matches!(&reply.outcome,ServiceOutcome::Error {message,..} if message.contains("too many"))
                    );
                } else {
                    assert!(
                        matches!(&reply.outcome, ServiceOutcome::Success { .. }),
                        "{:?}",
                        reply.outcome
                    );
                }
                self.step += 1;
            }
            _ => return Ok(Response::default()),
        }
        let files = self.files.as_ref().unwrap();
        if self.step > MAX_PACKAGE_ENTRIES + 1 {
            return Ok(Response {
                effects: vec![Effect::Complete {
                    operation: files.operation,
                    file: files.output,
                    name: "many.pez".into(),
                }],
                ..Default::default()
            });
        }
        let (method, arguments) = if self.step <= MAX_PACKAGE_ENTRIES {
            (
                "create_entry",
                json!(PackagePath {
                    operation: files.operation,
                    path: format!("files/{:04}.bin", self.step)
                }),
            )
        } else {
            (
                "pack",
                json!(PackagePack {
                    operation: files.operation
                }),
            )
        };
        Ok(Response {
            effects: vec![Effect::Call {
                call: ServiceCall {
                    request_id: "entry".into(),
                    service: ServiceRef {
                        provider: "host".into(),
                        id: "files".into(),
                        version: "^1".parse().unwrap(),
                    },
                    method: method.into(),
                    arguments,
                },
            }],
            ..Default::default()
        })
    }
}

#[test]
fn all_1024_output_entries_fit_the_operation_and_completed_artifacts_release_entry_snapshots() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    std::fs::write(&source, b"").unwrap();
    let registration = Builtin {
        create: Arc::new(|| {
            Box::new(ManyEntries {
                files: None,
                step: 0,
            })
        }),
        ..builtin()
    };
    let mut runtime = Runtime::spawn(Some(root.path().into()), vec![registration]).unwrap();
    wait(&runtime, 0).unwrap();
    let origin = runtime.registry().unwrap().plugins[0]
        .origin
        .clone()
        .unwrap();
    let scope = runtime
        .begin_provider(
            &origin,
            Provider::Format {
                id: "project".into(),
                operation: FormatOperation::Decode,
            },
            context().version,
        )
        .unwrap();
    let id = start(&mut runtime, &scope, &source);
    let output = wait(&runtime, id).unwrap();
    let artifact = output.artifact.unwrap();
    assert_eq!(
        ProjectArchive::open(artifact.path())
            .unwrap()
            .entries()
            .len(),
        MAX_PACKAGE_ENTRIES
    );
    assert_eq!(
        std::fs::read_dir(artifact.path().parent().unwrap())
            .unwrap()
            .count(),
        2
    );
    assert_eq!(
        runtime.registry().unwrap().plugins[0].status,
        Status::Enabled
    );
    runtime.shutdown().unwrap();
}
