use phistudio_sdk::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const MAGIC: &[u8] = b"PhiStudio project 1\n";
#[derive(Clone, Copy)]
enum Stage {
    List,
    EncodeStart,
    EncodeHeader,
    Open,
    Close,
    EncodeRead,
    EncodeWrite,
    DecodeStart,
    DecodeHeader,
    DecodeName,
    Create,
    DecodeRead,
    DecodeWrite,
    Pack,
}
struct ReadCheck {
    offset: u64,
    size: u64,
    length: usize,
}
struct Job {
    files: ProviderFiles,
    stage: Stage,
    entries: Vec<PackageEntry>,
    total: Option<usize>,
    index: usize,
    handle: u64,
    offset: u64,
    source: u64,
    root_written: u64,
    expected_write: u64,
    expected_read: Option<ReadCheck>,
    name_length: usize,
    data_total: u64,
}
impl Job {
    fn call(&mut self, stage: Stage, method: &str, arguments: Value) -> Response {
        self.stage = stage;
        Response {
            effects: vec![Effect::Call {
                call: ServiceCall {
                    request_id: "io".into(),
                    service: ServiceRef {
                        provider: "host".into(),
                        id: "files".into(),
                        version: "^1".parse().expect("valid file API version"),
                    },
                    method: method.into(),
                    arguments,
                },
            }],
            ..Default::default()
        }
    }
    fn read(
        &mut self,
        stage: Stage,
        file: u64,
        offset: u64,
        size: u64,
        length: usize,
    ) -> Result<Response, String> {
        if length == 0
            || length > MAX_FILE_CHUNK_BYTES
            || offset > size
            || length as u64 > size - offset
        {
            return Err("Project pack is truncated or has an invalid read range".into());
        }
        self.expected_read = Some(ReadCheck {
            offset,
            size,
            length,
        });
        Ok(self.call(
            stage,
            "read",
            json!(FileRead {
                operation: self.files.operation,
                file,
                offset,
                limit: length
            }),
        ))
    }
    fn write(
        &mut self,
        stage: Stage,
        file: u64,
        bytes: &[u8],
        previous: u64,
    ) -> Result<Response, String> {
        self.expected_write = previous + bytes.len() as u64;
        Ok(self.call(
            stage,
            "write",
            json!(FileWrite {
                operation: self.files.operation,
                file,
                offset: None,
                data: encode_file_chunk(bytes)?
            }),
        ))
    }
    fn list(&mut self) -> Response {
        self.call(
            Stage::List,
            "entries",
            json!(PackageEntries {
                operation: self.files.operation,
                output: false,
                offset: self.entries.len(),
                limit: 256
            }),
        )
    }
    fn encode_entry(&mut self) -> Result<Response, String> {
        if self.index == self.entries.len() {
            return Ok(self.complete("project.pspack"));
        }
        let entry = &self.entries[self.index];
        let mut header = Vec::with_capacity(12 + entry.path.len());
        header.extend((entry.path.len() as u32).to_le_bytes());
        header.extend(entry.size.to_le_bytes());
        header.extend(entry.path.as_bytes());
        self.offset = 0;
        self.write(
            Stage::EncodeHeader,
            self.files.output,
            &header,
            self.root_written,
        )
    }
    fn encode_read(&mut self) -> Result<Response, String> {
        let size = self.entries[self.index].size;
        if self.offset == size {
            return Ok(self.call(
                Stage::Close,
                "close_entry",
                json!(PackageRemove {
                    operation: self.files.operation,
                    file: self.handle
                }),
            ));
        }
        let length = (size - self.offset).min(MAX_FILE_CHUNK_BYTES as u64) as usize;
        self.read(Stage::EncodeRead, self.handle, self.offset, size, length)
    }
    fn decode_entry(&mut self) -> Result<Response, String> {
        if self.index == self.total.ok_or("Missing entry count")? {
            if self.source != self.files.input.size {
                return Err("Project pack has trailing data".into());
            }
            return Ok(self.call(
                Stage::Pack,
                "pack",
                json!(PackagePack {
                    operation: self.files.operation
                }),
            ));
        }
        self.offset = 0;
        self.read(
            Stage::DecodeHeader,
            self.files.input.id,
            self.source,
            self.files.input.size,
            12,
        )
    }
    fn decode_read(&mut self) -> Result<Response, String> {
        let size = self.entries[self.index].size;
        if self.offset == size {
            self.index += 1;
            return self.decode_entry();
        }
        let length = (size - self.offset).min(MAX_FILE_CHUNK_BYTES as u64) as usize;
        self.read(
            Stage::DecodeRead,
            self.files.input.id,
            self.source,
            self.files.input.size,
            length,
        )
    }
    fn complete(&self, name: &str) -> Response {
        Response {
            effects: vec![Effect::Complete {
                operation: self.files.operation,
                file: self.files.output,
                name: name.into(),
            }],
            ..Default::default()
        }
    }
    fn reply(&mut self, reply: &ServiceReply) -> Result<Response, String> {
        if reply.request_id != "io"
            || reply.service.provider != "host"
            || reply.service.id != "files"
        {
            return Err("Unexpected project file reply".into());
        }
        let value = match &reply.outcome {
            ServiceOutcome::Success { value } => value,
            ServiceOutcome::Error { message, .. } => return Err(message.clone()),
        };
        let bytes = if let Some(expected) = self.expected_read.take() {
            let chunk: FileChunk =
                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            let bytes = decode_file_chunk(&chunk.data)?;
            let end = expected.offset + expected.length as u64;
            if chunk.offset != expected.offset
                || chunk.size != expected.size
                || bytes.len() != expected.length
                || chunk.next_offset != (end < expected.size).then_some(end)
            {
                return Err("Invalid project file chunk".into());
            }
            bytes
        } else {
            vec![]
        };
        match self.stage {
            Stage::List => {
                let page: PackagePage =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                if page.offset != self.entries.len()
                    || page.total == 0
                    || page.total > MAX_PACKAGE_ENTRIES
                    || self.total.is_some_and(|n| n != page.total)
                    || page.entries.is_empty()
                    || page.offset + page.entries.len() > page.total
                    || page.next_offset
                        != (page.offset + page.entries.len() < page.total)
                            .then_some(page.offset + page.entries.len())
                {
                    return Err("Invalid project entry page".into());
                }
                self.total = Some(page.total);
                self.entries.extend(page.entries);
                if page.next_offset.is_some() {
                    return Ok(self.list());
                }
                let mut names = BTreeSet::new();
                for entry in &self.entries {
                    if entry.path.is_empty()
                        || entry.path.len() > MAX_PACKAGE_PATH_BYTES
                        || entry.size > MAX_PROVIDER_FILE_BYTES
                        || !names.insert(&entry.path)
                    {
                        return Err("Invalid project entry metadata".into());
                    }
                    self.data_total += entry.size;
                }
                if self.data_total > MAX_PACKAGE_DATA_BYTES {
                    return Err("Project resources exceed their total limit".into());
                }
                let mut header = MAGIC.to_vec();
                header.extend((self.entries.len() as u32).to_le_bytes());
                self.write(Stage::EncodeStart, self.files.output, &header, 0)
            }
            Stage::EncodeStart | Stage::EncodeHeader | Stage::EncodeWrite => {
                if value["size"].as_u64() != Some(self.expected_write) {
                    return Err("Invalid project output acknowledgement".into());
                }
                self.root_written = self.expected_write;
                match self.stage {
                    Stage::EncodeStart => self.encode_entry(),
                    Stage::EncodeHeader => {
                        let path = self.entries[self.index].path.clone();
                        Ok(self.call(
                            Stage::Open,
                            "open_entry",
                            json!(PackagePath {
                                operation: self.files.operation,
                                path
                            }),
                        ))
                    }
                    _ => self.encode_read(),
                }
            }
            Stage::Open => {
                let file: ProviderFile =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                let entry = &self.entries[self.index];
                if file.id < 3 || file.name != entry.path || file.size != entry.size {
                    return Err("Invalid project input handle".into());
                }
                self.handle = file.id;
                self.encode_read()
            }
            Stage::Close => {
                if !value.is_null() {
                    return Err("Invalid closed entry acknowledgement".into());
                }
                self.index += 1;
                self.encode_entry()
            }
            Stage::EncodeRead => {
                self.offset += bytes.len() as u64;
                self.write(
                    Stage::EncodeWrite,
                    self.files.output,
                    &bytes,
                    self.root_written,
                )
            }
            Stage::DecodeStart => {
                if !bytes.starts_with(MAGIC) {
                    return Err("This is not a Project pack file".into());
                }
                let count = u32::from_le_bytes(
                    bytes[MAGIC.len()..]
                        .try_into()
                        .map_err(|_| "Invalid project header")?,
                ) as usize;
                if count == 0 || count > MAX_PACKAGE_ENTRIES {
                    return Err("Invalid project entry count".into());
                }
                self.total = Some(count);
                self.source = bytes.len() as u64;
                self.decode_entry()
            }
            Stage::DecodeHeader => {
                self.name_length =
                    u32::from_le_bytes(bytes[..4].try_into().map_err(|_| "Invalid entry header")?)
                        as usize;
                let size =
                    u64::from_le_bytes(bytes[4..].try_into().map_err(|_| "Invalid entry header")?);
                self.data_total = self
                    .data_total
                    .checked_add(size)
                    .ok_or("Project size overflow")?;
                if self.name_length == 0
                    || self.name_length > MAX_PACKAGE_PATH_BYTES
                    || size > MAX_PROVIDER_FILE_BYTES
                    || self.data_total > MAX_PACKAGE_DATA_BYTES
                {
                    return Err("Project entry exceeds its limits".into());
                }
                self.entries.push(PackageEntry {
                    path: String::new(),
                    size,
                });
                self.source += 12;
                self.read(
                    Stage::DecodeName,
                    self.files.input.id,
                    self.source,
                    self.files.input.size,
                    self.name_length,
                )
            }
            Stage::DecodeName => {
                let path = String::from_utf8(bytes).map_err(|e| e.to_string())?;
                self.source += self.name_length as u64;
                self.entries[self.index].path = path.clone();
                Ok(self.call(
                    Stage::Create,
                    "create_entry",
                    json!(PackagePath {
                        operation: self.files.operation,
                        path
                    }),
                ))
            }
            Stage::Create => {
                let file: ProviderFile =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                if file.id < 3 || file.name != self.entries[self.index].path || file.size != 0 {
                    return Err("Invalid project output handle".into());
                }
                self.handle = file.id;
                self.decode_read()
            }
            Stage::DecodeRead => {
                let previous = self.offset;
                self.source += bytes.len() as u64;
                self.offset += bytes.len() as u64;
                self.write(Stage::DecodeWrite, self.handle, &bytes, previous)
            }
            Stage::DecodeWrite => {
                if value["size"].as_u64() != Some(self.expected_write) {
                    return Err("Invalid resource output acknowledgement".into());
                }
                self.decode_read()
            }
            Stage::Pack => {
                if value["size"]
                    .as_u64()
                    .is_none_or(|size| size == 0 || size > MAX_PROVIDER_FILE_BYTES)
                {
                    return Err("Invalid packed project size".into());
                }
                Ok(self.complete("project.pez"))
            }
        }
    }
}
#[derive(Default)]
struct ProjectPack {
    job: Option<Job>,
}
impl Plugin for ProjectPack {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        match request {
            Request::Activate { .. } => Ok(Response {
                contributions: vec![Contribution::Format {
                    id: "pack".into(),
                    title: "PhiStudio project pack".into(),
                    extensions: vec!["pspack".into()],
                    bridge: FormatBridge::Project,
                    decode: true,
                    encode: true,
                }],
                ..Default::default()
            }),
            Request::Format {
                id,
                operation,
                bridge,
                files,
                ..
            } if id == "pack" && *bridge == FormatBridge::Project => {
                let mut job = Job {
                    files: files.clone(),
                    stage: Stage::List,
                    entries: vec![],
                    total: None,
                    index: 0,
                    handle: 0,
                    offset: 0,
                    source: 0,
                    root_written: 0,
                    expected_write: 0,
                    expected_read: None,
                    name_length: 0,
                    data_total: 0,
                };
                let result = if *operation == FormatOperation::Encode {
                    Ok(job.list())
                } else {
                    job.read(
                        Stage::DecodeStart,
                        files.input.id,
                        0,
                        files.input.size,
                        MAGIC.len() + 4,
                    )
                };
                self.job = Some(job);
                Ok(self.finish_result(result))
            }
            Request::ServiceResult { reply, .. } => {
                let result = self
                    .job
                    .as_mut()
                    .ok_or("No active project operation")?
                    .reply(reply);
                Ok(self.finish_result(result))
            }
            _ => Ok(Response::default()),
        }
    }
}
impl ProjectPack {
    fn finish_result(&mut self, result: Result<Response, String>) -> Response {
        match result {
            Ok(response) => response,
            Err(message) => {
                let operation = self.job.as_ref().map_or(0, |job| job.files.operation);
                self.job = None;
                Response {
                    effects: vec![Effect::Fail { operation, message }],
                    ..Default::default()
                }
            }
        }
    }
}
phistudio_sdk::export_plugin!(ProjectPack::default);
