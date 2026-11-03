use super::*;
#[path = "files/packages.rs"]
mod packages;
use serde_json::json;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

pub(in crate::runtime) struct Files {
    pub(super) directory: tempfile::TempDir,
    pub(super) operation: Operation,
    input: File,
    output: File,
    input_size: u64,
    output_size: u64,
    package: packages::PackageFiles,
}
impl Files {
    pub(super) fn new(
        operation: Operation,
        source: Box<dyn PackageSource>,
        name: String,
        root: Option<&Path>,
    ) -> Result<(Self, ProviderFiles), String> {
        if !valid_source_name(&name) || !operation.is_running() {
            return Err("Invalid or cancelled provider input".into());
        }
        let directory = if let Some(root) = root {
            let jobs = root.join("plugins-work");
            std::fs::create_dir_all(&jobs).map_err(|e| e.to_string())?;
            tempfile::Builder::new()
                .prefix("operation-")
                .tempdir_in(jobs)
        } else {
            tempfile::Builder::new()
                .prefix("phistudio-provider-")
                .tempdir()
        }
        .map_err(|e| e.to_string())?;
        // Reject FIFOs and device files before opening; opening a FIFO may block.
        let metadata = std::fs::metadata(source.path()).map_err(|e| e.to_string())?;
        validate_input(&metadata)?;
        let mut input = File::open(source.path()).map_err(|e| e.to_string())?;
        validate_input(&input.metadata().map_err(|e| e.to_string())?)?;
        let mut snapshot = OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open(directory.path().join("input"))
            .map_err(|e| e.to_string())?;
        let mut buffer = vec![0; MAX_FILE_CHUNK_BYTES];
        let mut size = 0u64;
        loop {
            if !operation.is_running() {
                return Err("Provider operation was cancelled".into());
            }
            let read = input.read(&mut buffer).map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            size += read as u64;
            if size > MAX_PROVIDER_FILE_BYTES {
                return Err("Provider input exceeds 256 MiB".into());
            }
            snapshot
                .write_all(&buffer[..read])
                .map_err(|e| e.to_string())?;
        }
        snapshot.flush().map_err(|e| e.to_string())?;
        let output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open(directory.path().join("output"))
            .map_err(|e| e.to_string())?;
        let descriptor = ProviderFiles {
            operation: operation.id(),
            input: ProviderFile { id: 1, name, size },
            output: 2,
        };
        Ok((
            Self {
                directory,
                operation,
                input: snapshot,
                output,
                input_size: size,
                output_size: 0,
                package: packages::PackageFiles::default(),
            },
            descriptor,
        ))
    }
    pub(super) fn call(&mut self, request: &ServiceCall) -> Result<Value, String> {
        if !self.operation.is_running() {
            return Err("Provider operation is not running".into());
        }
        match request.method.as_str() {
            "entries" | "open_entry" | "close_entry" | "create_entry" | "remove_entry"
            | "rename_entry" | "pack" => self.package_call(request),
            "read" => {
                let args: FileRead =
                    serde_json::from_value(request.arguments.clone()).map_err(|e| e.to_string())?;
                if args.operation != self.operation.id()
                    || args.limit == 0
                    || args.limit > MAX_FILE_CHUNK_BYTES
                {
                    return Err("Invalid file read or operation ID".into());
                }
                let mut opened;
                let (file, size) = match args.file {
                    1 => (&mut self.input, self.input_size),
                    2 => (&mut self.output, self.output_size),
                    other => {
                        let entry = self
                            .package
                            .files
                            .get_mut(&other)
                            .ok_or("File does not belong to this operation")?;
                        opened = File::open(&entry.file).map_err(|e| e.to_string())?;
                        (&mut opened, entry.size)
                    }
                };
                if args.offset > size {
                    return Err("File offset is past the end".into());
                }
                file.seek(SeekFrom::Start(args.offset))
                    .map_err(|e| e.to_string())?;
                let length = (size - args.offset).min(args.limit as u64) as usize;
                let mut bytes = vec![0; length];
                file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
                let end = args.offset + length as u64;
                Ok(json!(FileChunk {
                    offset: args.offset,
                    size,
                    next_offset: (end < size).then_some(end),
                    data: encode_file_chunk(&bytes)?
                }))
            }
            "write" => {
                let args: FileWrite =
                    serde_json::from_value(request.arguments.clone()).map_err(|e| e.to_string())?;
                if args.operation != self.operation.id() || args.file == 1 {
                    return Err("Only the operation's output file is writable".into());
                }
                let bytes = decode_file_chunk(&args.data)?;
                if args.file != 2 {
                    return self.write_entry(args.file, args.offset, &bytes);
                }
                let offset = args.offset.unwrap_or(self.output_size);
                if offset > self.output_size {
                    return Err("File write offset is past the end".into());
                }
                let size = self.output_size.max(offset + bytes.len() as u64);
                if size > MAX_PROVIDER_FILE_BYTES {
                    return Err("Provider output exceeds 256 MiB".into());
                }
                self.output
                    .seek(SeekFrom::Start(offset))
                    .map_err(|e| e.to_string())?;
                self.output.write_all(&bytes).map_err(|e| e.to_string())?;
                self.output_size = size;
                Ok(json!({"size":size}))
            }
            "resize" => {
                let args: FileResize =
                    serde_json::from_value(request.arguments.clone()).map_err(|e| e.to_string())?;
                if args.operation != self.operation.id()
                    || args.file == 1
                    || args.size > MAX_PROVIDER_FILE_BYTES
                {
                    return Err("Invalid output file resize".into());
                }
                if args.file != 2 {
                    return self.resize_entry(args.file, args.size);
                }
                self.output.set_len(args.size).map_err(|e| e.to_string())?;
                self.output_size = args.size;
                Ok(json!({"size":args.size}))
            }
            _ => Err("Unknown provider file method".into()),
        }
    }
    pub(super) fn flush(&mut self) -> Result<(), String> {
        self.output.flush().map_err(|e| e.to_string())?;
        if self.output.metadata().map_err(|e| e.to_string())?.len() != self.output_size {
            return Err("Provider output is incomplete".into());
        }
        Ok(())
    }
}

fn validate_input(metadata: &std::fs::Metadata) -> Result<(), String> {
    if !metadata.is_file() {
        return Err("Provider input is not a regular file".into());
    }
    if metadata.len() > MAX_PROVIDER_FILE_BYTES {
        return Err("Provider input exceeds 256 MiB".into());
    }
    Ok(())
}
