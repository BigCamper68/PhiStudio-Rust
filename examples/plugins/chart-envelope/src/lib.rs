use phistudio_sdk::*;
use serde_json::{Value, json};

const HEADER: &[u8] = b"PhiStudio chart 1\n";
struct Transfer {
    files: ProviderFiles,
    direction: FormatOperation,
    offset: u64,
    next: Option<u64>,
    header: bool,
    written: u64,
}
impl Transfer {
    fn service(&self, method: &str, arguments: Value) -> Response {
        Response {
            effects: vec![Effect::Call {
                call: ServiceCall {
                    request_id: method.into(),
                    service: ServiceRef {
                        provider: "host".into(),
                        id: "files".into(),
                        version: "^1".parse().expect("valid file service requirement"),
                    },
                    method: method.into(),
                    arguments,
                },
            }],
            ..Default::default()
        }
    }
    fn read(&self) -> Response {
        self.service(
            "read",
            json!(FileRead {
                operation: self.files.operation,
                file: self.files.input.id,
                offset: self.offset,
                limit: MAX_FILE_CHUNK_BYTES
            }),
        )
    }
    fn write(&mut self, bytes: &[u8]) -> Result<Response, String> {
        self.written += bytes.len() as u64;
        Ok(self.service(
            "write",
            json!(FileWrite {
                operation: self.files.operation,
                file: self.files.output,
                offset: None,
                data: encode_file_chunk(bytes)?
            }),
        ))
    }
    fn reply(&mut self, reply: &ServiceReply) -> Result<Response, String> {
        if reply.service.provider != "host" || reply.service.id != "files" {
            return Err("Unexpected file service reply".into());
        }
        let value = match &reply.outcome {
            ServiceOutcome::Success { value } => value,
            ServiceOutcome::Error { message, .. } => return Err(message.clone()),
        };
        match reply.request_id.as_str() {
            "read" => {
                let chunk: FileChunk =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                let bytes = decode_file_chunk(&chunk.data)?;
                let end = self.offset + bytes.len() as u64;
                if chunk.offset != self.offset
                    || chunk.size != self.files.input.size
                    || end > chunk.size
                    || (end == self.offset && end < chunk.size)
                    || chunk.next_offset != (end < chunk.size).then_some(end)
                {
                    return Err("Invalid source file chunk".into());
                }
                self.next = chunk.next_offset;
                let bytes = if self.direction == FormatOperation::Decode && self.offset == 0 {
                    bytes
                        .strip_prefix(HEADER)
                        .ok_or("This is not a Chart envelope file")?
                } else {
                    &bytes
                };
                self.write(bytes)
            }
            "write" => {
                if value["size"].as_u64() != Some(self.written) {
                    return Err("Invalid output file size".into());
                }
                if self.header {
                    self.header = false;
                    Ok(self.read())
                } else if let Some(offset) = self.next {
                    self.offset = offset;
                    Ok(self.read())
                } else {
                    Ok(Response {
                        effects: vec![Effect::Complete {
                            operation: self.files.operation,
                            file: self.files.output,
                            name: if self.direction == FormatOperation::Decode {
                                "chart.rpe.json"
                            } else {
                                "chart.pschart"
                            }
                            .into(),
                        }],
                        ..Default::default()
                    })
                }
            }
            _ => Err("Unexpected file request correlation".into()),
        }
    }
}
#[derive(Default)]
struct ChartEnvelope {
    transfer: Option<Transfer>,
}
impl Plugin for ChartEnvelope {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![Contribution::Format {
                    id: "envelope".into(),
                    title: "PhiStudio chart envelope".into(),
                    extensions: vec!["pschart".into()],
                    bridge: FormatBridge::Chart,
                    decode: true,
                    encode: true,
                }],
                ..Default::default()
            },
            Request::Format {
                id,
                operation,
                files,
                ..
            } if id == "envelope" => {
                let mut transfer = Transfer {
                    files: files.clone(),
                    direction: *operation,
                    offset: 0,
                    next: None,
                    header: *operation == FormatOperation::Encode,
                    written: 0,
                };
                let response = if transfer.header {
                    transfer.write(HEADER)?
                } else {
                    transfer.read()
                };
                self.transfer = Some(transfer);
                response
            }
            Request::ServiceResult { reply, .. } => {
                let Some(transfer) = &mut self.transfer else {
                    return Ok(Response::default());
                };
                match transfer.reply(reply) {
                    Ok(response) => response,
                    Err(message) => Response {
                        effects: vec![Effect::Fail {
                            operation: transfer.files.operation,
                            message,
                        }],
                        ..Default::default()
                    },
                }
            }
            Request::Deactivate => {
                self.transfer = None;
                Response::default()
            }
            _ => Response::default(),
        })
    }
}
phistudio_sdk::export_plugin!(ChartEnvelope::default);
