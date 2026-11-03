//! Bounded archive access for project converters; chart interpretation stays in import.
use super::*;
use std::{
    collections::BTreeMap,
    io::{Seek, SeekFrom, Write},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

pub const MAX_PROJECT_ENTRIES: usize = 1024;
pub const MAX_PROJECT_BYTES: u64 = 512 * MIB;
pub const MAX_PROJECT_FILE_BYTES: u64 = 256 * MIB;

/// A virtual archive path, never an absolute filesystem path supplied by a guest.
pub fn project_entry_path(name: &str) -> Result<String, ChartError> {
    let path = relative(name)?;
    let text = path
        .to_str()
        .ok_or_else(|| invalid("Project entry path is not UTF-8"))?;
    if text.is_empty() || text.len() > 4096 || path.components().count() > 64 {
        return Err(invalid(
            "Project entry path is empty or exceeds its length/depth limit",
        ));
    }
    Ok(text.replace('\\', "/"))
}

#[derive(Clone, Debug)]
pub struct ProjectMember {
    pub path: String,
    pub size: u64,
    index: usize,
}
pub struct ProjectArchive {
    archive: ZipArchive<File>,
    entries: Vec<ProjectMember>,
}
fn check_parents(names: &BTreeMap<PathBuf, bool>) -> Result<(), ChartError> {
    for path in names.keys() {
        let mut parent = path.parent();
        while let Some(path) = parent {
            if names.get(path) == Some(&false) {
                return Err(invalid("Project entry is nested under a file"));
            }
            parent = path.parent();
        }
    }
    Ok(())
}
impl ProjectArchive {
    pub fn open(source: &Path) -> Result<Self, ChartError> {
        let metadata = fs::metadata(source)?;
        if !metadata.is_file() || metadata.len() > MAX_PROJECT_FILE_BYTES {
            return Err(invalid("Project ZIP must be a regular file within 256 MiB"));
        }
        let input = File::open(source)?;
        if !input.metadata()?.is_file() || input.metadata()?.len() > MAX_PROJECT_FILE_BYTES {
            return Err(invalid("Project ZIP changed before reading"));
        }
        let mut archive = ZipArchive::new(input).map_err(|e| invalid(e.to_string()))?;
        if archive.len() > MAX_PROJECT_ENTRIES {
            return Err(invalid("ZIP contains more than 1024 entries"));
        }
        let mut names = BTreeMap::new();
        let mut entries = vec![];
        let mut total = 0u64;
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|e| invalid(e.to_string()))?;
            let path = project_entry_path(entry.name())?;
            if names.insert(PathBuf::from(&path), entry.is_dir()).is_some() {
                return Err(invalid("Duplicate path in ZIP"));
            }
            if entry
                .unix_mode()
                .is_some_and(|m| ![0, 0o100000, 0o040000].contains(&(m & 0o170000)))
            {
                return Err(invalid("ZIP symlinks and special files are not supported"));
            }
            if entry.is_dir() {
                continue;
            }
            if entry.size() > MAX_PROJECT_FILE_BYTES
                || entry.compressed_size() > MAX_PROJECT_FILE_BYTES
            {
                return Err(invalid("ZIP entry exceeds 256 MiB"));
            }
            total = total
                .checked_add(entry.size())
                .ok_or_else(|| invalid("Project size overflow"))?;
            if total > MAX_PROJECT_BYTES {
                return Err(invalid("Extracted package exceeds 512 MiB"));
            }
            entries.push(ProjectMember {
                path,
                size: entry.size(),
                index,
            });
        }
        check_parents(&names)?;
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Self { archive, entries })
    }
    pub fn entries(&self) -> &[ProjectMember] {
        &self.entries
    }
    /// Decompress one entry on the caller's worker; never allocate its full contents.
    pub fn copy_entry(
        &mut self,
        index: usize,
        destination: &mut impl Write,
        active: impl Fn() -> bool,
    ) -> Result<u64, ChartError> {
        let member = self
            .entries
            .get(index)
            .ok_or_else(|| invalid("Project entry does not exist"))?;
        let mut entry = self
            .archive
            .by_index(member.index)
            .map_err(|e| invalid(e.to_string()))?;
        copy_exact(&mut entry, destination, member.size, active)
    }
}

fn copy_exact(
    input: &mut impl Read,
    output: &mut impl Write,
    size: u64,
    active: impl Fn() -> bool,
) -> Result<u64, ChartError> {
    let mut buffer = vec![0; 64 * 1024];
    let mut copied = 0u64;
    loop {
        if !active() {
            return Err(invalid("Project archive operation was cancelled"));
        }
        let limit = buffer.len().min((size - copied).saturating_add(1) as usize);
        let read = input.read(&mut buffer[..limit])?;
        if read == 0 {
            break;
        }
        copied += read as u64;
        if copied > size {
            return Err(invalid("Project entry exceeds its declared size"));
        }
        output.write_all(&buffer[..read])?;
    }
    if copied != size {
        return Err(invalid("Project entry size changed or is truncated"));
    }
    Ok(copied)
}

struct CappedFile<'a> {
    file: &'a mut File,
    position: u64,
}
impl Write for CappedFile<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > MAX_PROJECT_FILE_BYTES.saturating_sub(self.position) {
            return Err(io::Error::other("Project ZIP exceeds 256 MiB"));
        }
        let written = self.file.write(bytes)?;
        self.position += written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
impl Seek for CappedFile<'_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let position = self.file.seek(position)?;
        if position > MAX_PROJECT_FILE_BYTES {
            return Err(io::Error::other("Project ZIP seek exceeds its limit"));
        }
        self.position = position;
        Ok(position)
    }
}

/// Pack host-owned regular files at validated virtual paths without interpreting charts.
pub fn write_project_archive(
    files: &[(String, PathBuf)],
    destination: &mut File,
    active: impl Fn() -> bool,
) -> Result<u64, ChartError> {
    if files.len() > MAX_PROJECT_ENTRIES {
        return Err(invalid("Project contains more than 1024 files"));
    }
    let mut names = BTreeMap::new();
    let mut sources = vec![];
    let mut total = 0u64;
    for (name, source) in files {
        let path = project_entry_path(name)?;
        if names.insert(PathBuf::from(&path), false).is_some() {
            return Err(invalid("Duplicate project entry"));
        }
        let metadata = fs::symlink_metadata(source)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_PROJECT_FILE_BYTES
        {
            return Err(invalid(
                "Project entries must be regular files within 256 MiB",
            ));
        }
        total += metadata.len();
        if total > MAX_PROJECT_BYTES {
            return Err(invalid("Project files exceed 512 MiB"));
        }
        sources.push((path, source, metadata.len()));
    }
    check_parents(&names)?;
    destination.set_len(0)?;
    destination.seek(SeekFrom::Start(0))?;
    {
        let mut zip = ZipWriter::new(CappedFile {
            file: destination,
            position: 0,
        });
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, source, size) in sources {
            if !active() {
                return Err(invalid("Project archive operation was cancelled"));
            }
            zip.start_file(name, options)
                .map_err(|e| invalid(e.to_string()))?;
            let mut input = File::open(source)?;
            if !input.metadata()?.is_file() {
                return Err(invalid("Project entry changed before reading"));
            }
            copy_exact(&mut input, &mut zip, size, &active)?;
        }
        zip.finish().map_err(|e| invalid(e.to_string()))?;
    }
    if !active() {
        return Err(invalid("Project archive operation was cancelled"));
    }
    destination.sync_all()?;
    Ok(destination.metadata()?.len())
}
