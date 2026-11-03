use phistudio_sdk::*;
use serde_json::json;

fn bitmap(source: &[u8]) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(source).map_err(|e| e.to_string())?;
    let mut words = text.split_whitespace();
    if words.next() != Some("PSTILE") {
        return Err("Expected a PSTILE pixel grid".into());
    }
    let width: usize = words
        .next()
        .ok_or("Missing width")?
        .parse()
        .map_err(|_| "Invalid width")?;
    let height: usize = words
        .next()
        .ok_or("Missing height")?
        .parse()
        .map_err(|_| "Invalid height")?;
    if !(1..=64).contains(&width) || !(1..=64).contains(&height) {
        return Err("Use dimensions from 1 to 64".into());
    }
    let mut pixels = Vec::with_capacity(width * height);
    for _ in 0..width * height {
        let color = words
            .next()
            .ok_or("Not enough pixels")?
            .trim_start_matches('#');
        if color.len() != 6 {
            return Err("Use six-digit RGB colors".into());
        }
        pixels.push(u32::from_str_radix(color, 16).map_err(|_| "Invalid RGB color")?);
    }
    if words.next().is_some() {
        return Err("Too many pixels".into());
    }
    let stride = (width * 3 + 3) & !3;
    let size = stride * height;
    let mut bmp = Vec::with_capacity(54 + size);
    bmp.extend(b"BM");
    bmp.extend(((54 + size) as u32).to_le_bytes());
    bmp.extend([0; 4]);
    bmp.extend(54u32.to_le_bytes());
    bmp.extend(40u32.to_le_bytes());
    bmp.extend((width as i32).to_le_bytes());
    bmp.extend((height as i32).to_le_bytes());
    bmp.extend(1u16.to_le_bytes());
    bmp.extend(24u16.to_le_bytes());
    bmp.extend([0; 4]);
    bmp.extend((size as u32).to_le_bytes());
    bmp.extend([0; 16]);
    for row in (0..height).rev() {
        for color in &pixels[row * width..(row + 1) * width] {
            bmp.extend([*color as u8, (*color >> 8) as u8, (*color >> 16) as u8]);
        }
        bmp.resize(bmp.len() + (stride - width * 3), 0);
    }
    Ok(bmp)
}

#[derive(Default)]
struct PixelGrid {
    files: Option<ProviderFiles>,
}
impl PixelGrid {
    fn service(&self, method: &str, arguments: serde_json::Value) -> Response {
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
    fn reply(&self, reply: &ServiceReply) -> Result<Response, String> {
        let files = self
            .files
            .as_ref()
            .ok_or("Resource operation is unavailable")?;
        let value = match &reply.outcome {
            ServiceOutcome::Success { value } => value,
            ServiceOutcome::Error { message, .. } => return Err(message.clone()),
        };
        match reply.request_id.as_str() {
            "read" => {
                let chunk: FileChunk =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                if chunk.offset != 0 || chunk.next_offset.is_some() {
                    return Err("Pixel grid source exceeds one file chunk".into());
                }
                let image = bitmap(&decode_file_chunk(&chunk.data)?)?;
                Ok(self.service(
                    "write",
                    json!(FileWrite {
                        operation: files.operation,
                        file: files.output,
                        offset: None,
                        data: encode_file_chunk(&image)?
                    }),
                ))
            }
            "write" => Ok(Response {
                effects: vec![Effect::Complete {
                    operation: files.operation,
                    file: files.output,
                    name: "pixel-grid.bmp".into(),
                }],
                ..Default::default()
            }),
            _ => Err("Unexpected resource file reply".into()),
        }
    }
}
impl Plugin for PixelGrid {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![Contribution::Resource {
                    id: "pixel-grid".into(),
                    title: "Pixel grid texture".into(),
                    extensions: vec!["pgrid".into()],
                    resource: ResourceKind::Image,
                }],
                ..Default::default()
            },
            Request::Resource { files, .. } => {
                self.files = Some(files.clone());
                self.service(
                    "read",
                    json!(FileRead {
                        operation: files.operation,
                        file: files.input.id,
                        offset: 0,
                        limit: MAX_FILE_CHUNK_BYTES
                    }),
                )
            }
            Request::ServiceResult { reply, .. } => match self.reply(reply) {
                Ok(response) => response,
                Err(message) => Response {
                    effects: self
                        .files
                        .as_ref()
                        .map(|f| Effect::Fail {
                            operation: f.operation,
                            message,
                        })
                        .into_iter()
                        .collect(),
                    ..Default::default()
                },
            },
            _ => Response::default(),
        })
    }
}
phistudio_sdk::export_plugin!(PixelGrid::default);
