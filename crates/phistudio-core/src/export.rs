//! Package export is a snapshot: saved projects, source archives and recovery stay untouched.
use crate::{ChartDocument, ChartError, ProjectOptions};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use zip::{ZipWriter, write::SimpleFileOptions};

#[derive(Debug)]
pub struct ProjectExport {
    pub(crate) file: tempfile::NamedTempFile,
    pub name: String,
}
impl ProjectExport {
    /// Prepare a non-chart export for the existing platform save workflow.
    pub fn from_bytes(name: String, bytes: &[u8]) -> Result<Self, ChartError> {
        let mut file = tempfile::NamedTempFile::new()?;
        file.write_all(bytes)?;
        file.as_file().sync_all()?;
        Ok(Self { file, name })
    }
    pub fn path(&self) -> &Path {
        self.file.path()
    }
    /// Stage next to the destination so a failed write cannot truncate an existing export.
    pub fn write_to(&self, destination: &Path) -> Result<(), ChartError> {
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut staged = tempfile::NamedTempFile::new_in(parent)?;
        io::copy(&mut File::open(self.path())?, &mut staged)?;
        staged.as_file().sync_all()?;
        staged.persist(destination).map_err(|e| e.error)?;
        Ok(())
    }
}
fn invalid(message: impl Into<String>) -> ChartError {
    ChartError::Invalid(message.into())
}

pub fn prepare_project_export(
    document: &ChartDocument,
    options: &ProjectOptions,
    assets: Option<&Path>,
    chart: Option<&Path>,
) -> Result<ProjectExport, ChartError> {
    prepare_project_export_checked(document, options, assets, chart, || true)
}

pub fn prepare_project_export_checked(
    document: &ChartDocument,
    options: &ProjectOptions,
    assets: Option<&Path>,
    chart: Option<&Path>,
    active: impl Fn() -> bool,
) -> Result<ProjectExport, ChartError> {
    prepare_project_export_with_plugin_data_checked(
        document,
        options,
        assets,
        chart,
        phistudio_sdk::PluginDataExport::Preserve,
        active,
    )
}

pub fn prepare_project_export_with_plugin_data(
    document: &ChartDocument,
    options: &ProjectOptions,
    assets: Option<&Path>,
    chart: Option<&Path>,
    policy: phistudio_sdk::PluginDataExport,
) -> Result<ProjectExport, ChartError> {
    prepare_project_export_with_plugin_data_checked(
        document,
        options,
        assets,
        chart,
        policy,
        || true,
    )
}

pub fn prepare_project_export_with_plugin_data_checked(
    document: &ChartDocument,
    options: &ProjectOptions,
    assets: Option<&Path>,
    chart: Option<&Path>,
    policy: phistudio_sdk::PluginDataExport,
    active: impl Fn() -> bool,
) -> Result<ProjectExport, ChartError> {
    let check = || {
        if active() {
            Ok(())
        } else {
            Err(invalid("Project export was cancelled"))
        }
    };
    check()?;
    let exported = document.with_plugin_data_export(policy)?;
    let document = exported.as_ref();
    let root = assets.map(Path::canonicalize).transpose()?;
    let mut files = root
        .as_deref()
        .map(crate::package::inventory)
        .transpose()?
        .unwrap_or_default();
    files.sort();
    check()?;
    if files
        .iter()
        .any(|path| path != Path::new("info.yml") && path.starts_with("info.yml"))
    {
        return Err(invalid("Package has a directory named info.yml"));
    }
    let mut chart_path = chart
        .zip(root.as_deref())
        .map(|(chart, root)| crate::package::export_chart_path(chart, root))
        .transpose()?
        .flatten();
    if chart_path.as_ref().is_none_or(|p| {
        p.extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("json"))
    }) {
        let mut suffix = 0;
        loop {
            let path = PathBuf::from(if suffix == 0 {
                "chart.rpe.json".into()
            } else {
                format!("chart-{suffix}.rpe.json")
            });
            if !files.iter().any(|existing| existing.starts_with(&path)) {
                chart_path = Some(path);
                break;
            }
            suffix += 1;
        }
    }
    let chart_path = chart_path.unwrap();
    let path_text = |p: &Path| -> Result<String, ChartError> {
        let value = p
            .to_str()
            .ok_or_else(|| invalid("Package paths must be UTF-8"))?;
        if value.contains(['\\', ':', '\0']) {
            return Err(invalid("Unsafe package path"));
        }
        Ok(value.to_owned())
    };
    let mut manifest = serde_yaml::Mapping::new();
    if let Some(root) = &root {
        for name in ["info.yml", "info.yaml"] {
            let path = root.join(name);
            if path.is_file() && path.metadata()?.len() <= 1024 * 1024 {
                if let Ok(serde_yaml::Value::Mapping(value)) =
                    serde_yaml::from_slice::<serde_yaml::Value>(&fs::read(path)?)
                {
                    manifest = value;
                }
                break;
            }
        }
    }
    let mut set = |key: &str, value: serde_yaml::Value| {
        manifest.retain(|k, _| k.as_str().is_none_or(|k| !k.eq_ignore_ascii_case(key)));
        manifest.insert(key.into(), value);
    };
    for key in ["name", "composer", "charter", "level"] {
        set(key, options.display_field(document, key).into());
    }
    set("chart", path_text(&chart_path)?.into());
    for (key, asset) in [
        ("music", &options.music),
        ("illustration", &options.illustration),
    ] {
        if let Some(asset) = asset {
            let relative = crate::package::relative(asset)?;
            if !files.contains(&relative) {
                return Err(invalid(format!("Missing {key} resource: {asset}")));
            }
            set(key, path_text(&relative)?.into());
        } else {
            set(key, serde_yaml::Value::Null);
        }
    }
    set("format", "rpe".into());
    set(
        "offset",
        serde_yaml::to_value(options.manifest_offset_ms as f64 / 1000.).unwrap(),
    );
    set(
        "backgroundDim",
        serde_yaml::to_value(options.background_dim(document)).unwrap(),
    );
    for (key, value) in [
        ("useRpe170Speed", options.use_rpe_170_speed),
        ("noteUniformScale", options.note_uniform_scale),
        ("holdPartialCover", options.hold_partial_cover),
        ("forceAspectRatio", options.force_aspect_ratio),
    ] {
        set(key, value.into());
    }
    if let Some(value) = options.use_attach_ui_fix {
        set("useAttachUIFix", value.into());
    }
    if let Some(value) = options.aspect_ratio {
        set(
            "aspectRatio",
            serde_yaml::to_value(value).map_err(|e| invalid(e.to_string()))?,
        );
    }
    let manifest = serde_yaml::to_string(&manifest).map_err(|e| invalid(e.to_string()))?;
    #[cfg(target_os = "android")]
    let export_cache = std::env::current_dir()?.join("export-cache");
    #[cfg(target_os = "android")]
    fs::create_dir_all(&export_cache)?;
    let extra = document
        .as_json()
        .get(crate::shaders::EXTRA_KEY)
        .map(|v| {
            crate::shaders::validate_shader_extra(v)?;
            serde_json::to_vec_pretty(v).map_err(ChartError::from)
        })
        .transpose()?;
    let sources = crate::shaders::shader_sources(document)?;
    let source_bytes: usize = sources
        .values()
        .filter_map(serde_json::Value::as_str)
        .map(str::len)
        .sum();
    let (noise_chart, noise_resources) = crate::noise_fields::export::prepare(document, &files)?;
    let chart_bytes = serde_json::to_vec_pretty(&noise_chart)?;
    check()?;
    if chart_bytes.len() > ChartDocument::MAX_JSON_BYTES {
        return Err(invalid("Chart exceeds 256 MiB"));
    }
    let mut builder = tempfile::Builder::new();
    builder.prefix("phistudio-export-").suffix(".pez");
    #[cfg(target_os = "android")]
    let mut file = builder.tempfile_in(export_cache)?;
    #[cfg(not(target_os = "android"))]
    let mut file = builder.tempfile()?;
    {
        let mut zip = ZipWriter::new(file.as_file_mut());
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut total = 0;
        let mut count = 0;
        for path in files {
            check()?;
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase();
            if sources.contains_key(&path_text(&path)?)
                || (extra.is_some() && path == Path::new("extra.json"))
                || path == chart_path
                || matches!(name.as_str(), "info.yml" | "info.yaml" | "info.txt")
                || name.starts_with(".phistudio-")
            {
                continue;
            }
            count += 1;
            if count + sources.len() + noise_resources.len()
                > if extra.is_some() { 1021 } else { 1022 }
            {
                return Err(invalid("Package contains too many files"));
            }
            let source_path = root.as_ref().unwrap().join(&path);
            let filtered = filtered_chart_resource(&source_path, policy, &active)?;
            let mut source: Box<dyn Read> = match filtered {
                Some(bytes) => Box::new(io::Cursor::new(bytes)),
                None => Box::new(File::open(source_path)?.take(128 * 1024 * 1024 + 1)),
            };
            zip.start_file(path_text(&path)?, options)
                .map_err(|e| invalid(e.to_string()))?;
            let mut size = 0;
            let mut buffer = vec![0; 64 * 1024];
            loop {
                check()?;
                let read = source.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                zip.write_all(&buffer[..read])?;
                size += read as u64;
            }
            total += size;
            if size > 128 * 1024 * 1024
                || total
                    + chart_bytes.len() as u64
                    + source_bytes as u64
                    + manifest.len() as u64
                    + extra.as_ref().map_or(0, |v| v.len() as u64)
                    > 512 * 1024 * 1024
            {
                return Err(invalid("Package exceeds the export size limit"));
            }
        }
        for (path, bytes) in &noise_resources {
            check()?;
            zip.start_file(path_text(path)?, options)
                .map_err(|e| invalid(e.to_string()))?;
            zip.write_all(bytes)?;
        }
        for (path, source) in &sources {
            check()?;
            zip.start_file(path, options)
                .map_err(|e| invalid(e.to_string()))?;
            zip.write_all(source.as_str().unwrap().as_bytes())?;
        }
        if let Some(extra) = &extra {
            zip.start_file("extra.json", options)
                .map_err(|e| invalid(e.to_string()))?;
            zip.write_all(extra)?;
        }
        zip.start_file(path_text(&chart_path)?, options)
            .map_err(|e| invalid(e.to_string()))?;
        zip.write_all(&chart_bytes)?;
        zip.start_file("info.yml", options)
            .map_err(|e| invalid(e.to_string()))?;
        zip.write_all(manifest.as_bytes())?;
        zip.finish().map_err(|e| invalid(e.to_string()))?;
    }
    file.as_file().sync_all()?;
    check()?;
    if file.as_file().metadata()?.len() > 256 * 1024 * 1024 {
        return Err(invalid("Exported ZIP exceeds 256 MiB"));
    }
    let name: String = options
        .display_field(document, "name")
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(100)
        .collect();
    Ok(ProjectExport {
        file,
        name: format!(
            "{}.pez",
            if name.trim().is_empty() {
                "Untitled"
            } else {
                name.trim()
            }
        ),
    })
}

fn filtered_chart_resource(
    path: &Path,
    policy: phistudio_sdk::PluginDataExport,
    active: &impl Fn() -> bool,
) -> Result<Option<Vec<u8>>, ChartError> {
    if policy == phistudio_sdk::PluginDataExport::Preserve
        || path
            .extension()
            .is_none_or(|s| !s.eq_ignore_ascii_case("json"))
    {
        return Ok(None);
    }
    struct Reader<'a> {
        source: io::Take<File>,
        active: &'a dyn Fn() -> bool,
    }
    impl Read for Reader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if !(self.active)() {
                return Err(io::Error::other("Project export was cancelled"));
            }
            self.source.read(buffer)
        }
    }
    let source = File::open(path)?;
    if source.metadata()?.len() > 128 * 1024 * 1024 {
        return Err(invalid("Package exceeds the export size limit"));
    }
    let reader = Reader {
        source: source.take(128 * 1024 * 1024 + 1),
        active,
    };
    let value: serde_json::Value = match crate::json_io::read(reader) {
        Ok(value) => value,
        Err(error) if error.is_io() => return Err(error.into()),
        Err(_) => return Ok(None),
    };
    // Resource JSON is opaque. Only editable RPE charts carry this namespace.
    if !value.get("META").is_some_and(serde_json::Value::is_object)
        || !value
            .get("BPMList")
            .is_some_and(serde_json::Value::is_array)
        || !value
            .get("judgeLineList")
            .is_some_and(serde_json::Value::is_array)
    {
        return Ok(None);
    }
    match crate::plugin_data::export_value(&value, policy)? {
        std::borrow::Cow::Borrowed(_) => Ok(None),
        std::borrow::Cow::Owned(value) => {
            let bytes = serde_json::to_vec_pretty(&value)?;
            if bytes.len() > 128 * 1024 * 1024 {
                return Err(invalid("Filtered chart exceeds 128 MiB"));
            }
            Ok(Some(bytes))
        }
    }
}

#[cfg(test)]
mod plugin_data_tests {
    use super::*;
    use phistudio_sdk::{PLUGIN_DATA_KEY, PluginData, PluginDataExport, PluginDataPolicy};
    use std::cell::Cell;

    #[test]
    fn secondary_chart_filtering_propagates_cancellation_during_json_reads() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("secondary.json");
        let mut value = ChartDocument::empty("Cancellation", 120.)
            .unwrap()
            .as_json()
            .clone();
        value[PLUGIN_DATA_KEY] = serde_json::json!({"test.editor":PluginData::new(1, PluginDataPolicy::EditorOnly, serde_json::Value::Null)});
        value["largeUnknown"] = serde_json::json!("x".repeat(512 * 1024));
        let bytes = serde_json::to_vec(&value).unwrap();
        fs::write(&path, &bytes).unwrap();
        let reads = Cell::new(0);
        let result = filtered_chart_resource(&path, PluginDataExport::Runtime, &|| {
            reads.set(reads.get() + 1);
            reads.get() < 3
        });
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert!(reads.get() >= 2);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
