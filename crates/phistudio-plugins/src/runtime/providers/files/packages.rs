use super::*;
use phistudio_core::{MAX_PROJECT_BYTES, MAX_PROJECT_ENTRIES, ProjectArchive, project_entry_path};
use std::collections::BTreeMap;

pub(super) struct Entry {
    path: String,
    pub(super) file: tempfile::TempPath,
    pub(super) size: u64,
    writable: bool,
}
pub(super) struct PackageFiles {
    archive: Option<ProjectArchive>,
    pub(super) files: BTreeMap<u64, Entry>,
    next: u64,
    written: u64,
}
impl Default for PackageFiles {
    fn default() -> Self {
        Self {
            archive: None,
            files: BTreeMap::new(),
            next: 3,
            written: 0,
        }
    }
}
impl Files {
    pub(super) fn package_call(&mut self, request: &ServiceCall) -> Result<Value, String> {
        if self.operation.format_bridge() != Some(FormatBridge::Project) {
            return Err("Package services require a project format provider".into());
        }
        match request.method.as_str() {
            "entries" => {
                let args: PackageEntries =
                    serde_json::from_value(request.arguments.clone()).map_err(|e| e.to_string())?;
                if args.operation != self.operation.id() || args.limit == 0 || args.limit > 256 {
                    return Err("Invalid package page or operation".into());
                }
                let page = if args.output {
                    package_page(
                        &args,
                        self.package.files.values().filter(|e| e.writable).count(),
                        self.package
                            .files
                            .values()
                            .filter(|e| e.writable)
                            .map(|e| (e.path.as_str(), e.size)),
                    )
                } else {
                    let archive = self.input_archive()?;
                    package_page(
                        &args,
                        archive.entries().len(),
                        archive.entries().iter().map(|e| (e.path.as_str(), e.size)),
                    )
                };
                Ok(json!(page))
            }
            "open_entry" => {
                let args: PackagePath =
                    serde_json::from_value(request.arguments.clone()).map_err(|e| e.to_string())?;
                self.check_package_operation(args.operation)?;
                let path = project_entry_path(&args.path).map_err(|e| e.to_string())?;
                if let Some((&id, entry)) = self
                    .package
                    .files
                    .iter()
                    .find(|(_, e)| !e.writable && e.path == path)
                {
                    return Ok(json!(ProviderFile {
                        id,
                        name: entry.path.clone(),
                        size: entry.size
                    }));
                }
                let index = self
                    .input_archive()?
                    .entries()
                    .iter()
                    .position(|e| e.path == path)
                    .ok_or("Project entry does not exist")?;
                let mut file = tempfile::NamedTempFile::new_in(self.directory.path())
                    .map_err(|e| e.to_string())?;
                let operation = self.operation.clone();
                let size = self
                    .input_archive()?
                    .copy_entry(index, file.as_file_mut(), || operation.is_running())
                    .map_err(|e| e.to_string())?;
                let id = self.package.next;
                self.package.next = id.checked_add(1).ok_or("File handle sequence exhausted")?;
                self.package.files.insert(
                    id,
                    Entry {
                        path: path.clone(),
                        file: file.into_temp_path(),
                        size,
                        writable: false,
                    },
                );
                Ok(json!(ProviderFile {
                    id,
                    name: path,
                    size
                }))
            }
            "create_entry" => {
                let args: PackagePath =
                    serde_json::from_value(request.arguments.clone()).map_err(|e| e.to_string())?;
                self.check_package_operation(args.operation)?;
                let path = project_entry_path(&args.path).map_err(|e| e.to_string())?;
                self.check_entry_name(&path, None)?;
                if self.package.files.values().filter(|e| e.writable).count() >= MAX_PROJECT_ENTRIES
                {
                    return Err("Project contains too many output entries".into());
                }
                let file = tempfile::NamedTempFile::new_in(self.directory.path())
                    .map_err(|e| e.to_string())?;
                let id = self.package.next;
                self.package.next = id.checked_add(1).ok_or("File handle sequence exhausted")?;
                self.package.files.insert(
                    id,
                    Entry {
                        path: path.clone(),
                        file: file.into_temp_path(),
                        size: 0,
                        writable: true,
                    },
                );
                Ok(json!(ProviderFile {
                    id,
                    name: path,
                    size: 0
                }))
            }
            "remove_entry" => {
                let args: PackageRemove =
                    serde_json::from_value(request.arguments.clone()).map_err(|e| e.to_string())?;
                self.check_package_operation(args.operation)?;
                let entry = self
                    .package
                    .files
                    .get(&args.file)
                    .filter(|e| e.writable)
                    .ok_or("Only an output entry can be removed")?;
                self.package.written -= entry.size;
                self.package.files.remove(&args.file);
                Ok(Value::Null)
            }
            "close_entry" => {
                let args: PackageRemove =
                    serde_json::from_value(request.arguments.clone()).map_err(|e| e.to_string())?;
                self.check_package_operation(args.operation)?;
                self.package
                    .files
                    .get(&args.file)
                    .filter(|e| !e.writable)
                    .ok_or("Only an opened input entry can be closed")?;
                self.package.files.remove(&args.file);
                Ok(Value::Null)
            }
            "rename_entry" => {
                let args: PackageRename =
                    serde_json::from_value(request.arguments.clone()).map_err(|e| e.to_string())?;
                self.check_package_operation(args.operation)?;
                let path = project_entry_path(&args.path).map_err(|e| e.to_string())?;
                self.check_entry_name(&path, Some(args.file))?;
                let entry = self
                    .package
                    .files
                    .get_mut(&args.file)
                    .filter(|e| e.writable)
                    .ok_or("Only an output entry can be renamed")?;
                entry.path = path.clone();
                Ok(json!(ProviderFile {
                    id: args.file,
                    name: path,
                    size: entry.size
                }))
            }
            "pack" => {
                let args: PackagePack =
                    serde_json::from_value(request.arguments.clone()).map_err(|e| e.to_string())?;
                self.check_package_operation(args.operation)?;
                let entries: Vec<_> = self
                    .package
                    .files
                    .values()
                    .filter(|e| e.writable)
                    .map(|e| (e.path.clone(), e.file.to_path_buf()))
                    .collect();
                let mut file = tempfile::NamedTempFile::new_in(self.directory.path())
                    .map_err(|e| e.to_string())?;
                let size =
                    phistudio_core::write_project_archive(&entries, file.as_file_mut(), || {
                        self.operation.is_running()
                    })
                    .map_err(|e| e.to_string())?;
                let output = file.as_file().try_clone().map_err(|e| e.to_string())?;
                if !self.operation.is_running() {
                    return Err("Provider operation was cancelled".into());
                }
                file.persist(self.directory.path().join("output"))
                    .map_err(|e| e.error.to_string())?;
                self.output = output;
                self.output_size = size;
                Ok(json!({"size":size}))
            }
            _ => Err("Unknown project file method".into()),
        }
    }
    fn input_archive(&mut self) -> Result<&mut ProjectArchive, String> {
        if self.package.archive.is_none() {
            self.package.archive = Some(
                ProjectArchive::open(&self.directory.path().join("input"))
                    .map_err(|e| e.to_string())?,
            );
        }
        self.package
            .archive
            .as_mut()
            .ok_or_else(|| "Project archive is unavailable".into())
    }
    fn check_package_operation(&self, id: u64) -> Result<(), String> {
        if id != self.operation.id() {
            return Err("Invalid project operation ID".into());
        }
        Ok(())
    }
    fn check_entry_name(&self, path: &str, skip: Option<u64>) -> Result<(), String> {
        let path = Path::new(path);
        if self.package.files.iter().any(|(id, e)| {
            e.writable
                && Some(*id) != skip
                && (Path::new(&e.path).starts_with(path) || path.starts_with(&e.path))
        }) {
            return Err("Project entry duplicates or conflicts with an existing path".into());
        }
        Ok(())
    }
    pub(super) fn write_entry(
        &mut self,
        id: u64,
        offset: Option<u64>,
        bytes: &[u8],
    ) -> Result<Value, String> {
        let entry = self
            .package
            .files
            .get_mut(&id)
            .filter(|e| e.writable)
            .ok_or("Only an output entry is writable")?;
        let offset = offset.unwrap_or(entry.size);
        if offset > entry.size {
            return Err("File write offset is past the end".into());
        }
        let size = entry.size.max(offset + bytes.len() as u64);
        let total = self.package.written - entry.size + size;
        if size > MAX_PROVIDER_FILE_BYTES || total > MAX_PROJECT_BYTES {
            return Err("Project output entries exceed their size limit".into());
        }
        let mut file = OpenOptions::new()
            .write(true)
            .read(true)
            .open(&entry.file)
            .map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
        self.package.written = total;
        entry.size = size;
        Ok(json!({"size":size}))
    }
    pub(super) fn resize_entry(&mut self, id: u64, size: u64) -> Result<Value, String> {
        let entry = self
            .package
            .files
            .get_mut(&id)
            .filter(|e| e.writable)
            .ok_or("Only an output entry is writable")?;
        let total = self.package.written - entry.size + size;
        if size > MAX_PROVIDER_FILE_BYTES || total > MAX_PROJECT_BYTES {
            return Err("Project output entries exceed their size limit".into());
        }
        OpenOptions::new()
            .write(true)
            .open(&entry.file)
            .map_err(|e| e.to_string())?
            .set_len(size)
            .map_err(|e| e.to_string())?;
        self.package.written = total;
        entry.size = size;
        Ok(json!({"size":size}))
    }
}

fn package_page<'a>(
    args: &PackageEntries,
    total: usize,
    entries: impl Iterator<Item = (&'a str, u64)>,
) -> PackagePage {
    let mut page = PackagePage {
        offset: args.offset,
        total,
        next_offset: None,
        entries: vec![],
    };
    let mut bytes = 0;
    for (path, size) in entries.skip(args.offset).take(args.limit) {
        let entry = PackageEntry {
            path: path.to_owned(),
            size,
        };
        let Some(size) = crate::limits::serialized_size(
            &entry,
            (256 * 1024 - 512usize)
                .saturating_sub(bytes)
                .saturating_sub(1),
        ) else {
            break;
        };
        bytes += size + 1;
        page.entries.push(entry);
    }
    let next = args.offset.saturating_add(page.entries.len());
    page.next_offset = (next < page.total).then_some(next);
    page
}
