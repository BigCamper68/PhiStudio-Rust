use super::{invalid, relative};
use crate::{ChartDocument, ChartError, ProjectOptions};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub(super) struct Manifest {
    pub path: PathBuf,
    yaml: bool,
    fields: BTreeMap<String, Option<String>>,
}
impl Manifest {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .get(key)?
            .as_deref()
            .filter(|v| !v.trim().is_empty())
    }
    pub fn resolve(&self, value: &str) -> Result<PathBuf, ChartError> {
        Ok(self
            .path
            .parent()
            .unwrap_or(Path::new(""))
            .join(relative(value.trim())?))
    }
}
fn indent(line: &str) -> usize {
    line.bytes().take_while(|c| *c == b' ').count()
}
fn comment(value: &str) -> &str {
    let mut single = false;
    let mut double = false;
    let mut chars = value.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' if double => {
                chars.next();
            }
            '\'' if !double && (single || i == 0) => {
                if single && chars.peek().is_some_and(|(_, c)| *c == '\'') {
                    chars.next();
                } else {
                    single = !single;
                }
            }
            '"' if !single && (double || i == 0) => double = !double,
            '#' if !single && !double && (i == 0 || value[..i].ends_with(char::is_whitespace)) => {
                return value[..i].trim_end();
            }
            _ => {}
        }
    }
    value
}
fn scalar(value: &str) -> Option<String> {
    if value.eq_ignore_ascii_case("null") || value == "~" {
        return None;
    }
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        return Some(
            serde_yaml::from_str::<String>(value)
                .unwrap_or_else(|_| value[1..value.len() - 1].to_owned()),
        );
    }
    Some(value.to_owned())
}
fn parse(path: PathBuf, yaml: bool, source: &str) -> Manifest {
    let source = source.trim_start_matches('\u{feff}');
    let lines: Vec<_> = source.lines().collect();
    let root = if yaml {
        lines
            .iter()
            .filter(|line| !line.trim_start().starts_with('#') && line.contains(':'))
            .map(|line| indent(line))
            .min()
            .unwrap_or(0)
    } else {
        0
    };
    let mut fields = BTreeMap::new();
    for (i, line) in lines.iter().enumerate() {
        if yaml && indent(line) != root {
            continue;
        }
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_lowercase();
        if key.is_empty() || fields.contains_key(&key) {
            continue;
        }
        let value = if yaml {
            comment(value.trim()).trim()
        } else {
            value.trim()
        };
        let parsed = if yaml && (value.starts_with('|') || value.starts_with('>')) {
            let body = lines[i + 1..]
                .iter()
                .take_while(|line| line.trim().is_empty() || indent(line) > root)
                .map(|line| line.get(root..).unwrap_or(""))
                .collect::<Vec<_>>()
                .join("\n");
            let literal = format!("value: {value}\n{body}\n");
            serde_yaml::from_str::<BTreeMap<String, String>>(&literal)
                .ok()
                .and_then(|m| m.get("value").cloned())
        } else {
            scalar(value)
        };
        fields.insert(key, parsed);
    }
    Manifest { path, yaml, fields }
}
pub(super) fn read(base: &Path, files: &[PathBuf]) -> Result<Vec<Manifest>, ChartError> {
    let mut result = Vec::new();
    for path in files {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        if !matches!(name.as_str(), "info.yml" | "info.yaml" | "info.txt") {
            continue;
        }
        let full = base.join(path);
        if full.metadata()?.len() > 1024 * 1024 {
            continue;
        }
        let source = fs::read_to_string(full)
            .map_err(|e| invalid(format!("Could not read {}: {e}", path.display())))?;
        result.push(parse(path.clone(), name != "info.txt", &source));
    }
    result.sort_by(|a, b| b.yaml.cmp(&a.yaml).then_with(|| a.path.cmp(&b.path)));
    Ok(result)
}
pub(super) fn relevant<'a>(all: &'a [Manifest], common: &Path, chart: &Path) -> Vec<&'a Manifest> {
    let mut active: Vec<_> = all
        .iter()
        .filter(|m| m.path.parent() == Some(common) || m.path.parent() == chart.parent())
        .collect();
    active.sort_by(|a, b| {
        (a.path.parent() != Some(common))
            .cmp(&(b.path.parent() != Some(common)))
            .then_with(|| b.yaml.cmp(&a.yaml))
            .then_with(|| a.path.cmp(&b.path))
    });
    active
}
fn first<'a>(manifests: &[&'a Manifest], key: &str) -> Option<&'a str> {
    manifests.iter().find_map(|m| m.get(key))
}
fn resource(
    manifests: &[&Manifest],
    files: &[PathBuf],
    chart: &Path,
    keys: &[&str],
    extensions: &[&str],
    meta: &str,
) -> Result<Option<PathBuf>, ChartError> {
    let paths: HashSet<_> = files.iter().collect();
    for manifest in manifests {
        for key in keys {
            if let Some(value) = manifest.get(key) {
                let path = manifest.resolve(value).map_err(|_| {
                    invalid(format!(
                        "Unsafe {} resource path: {value}",
                        manifest.path.display()
                    ))
                })?;
                if paths.contains(&path) {
                    return Ok(Some(path));
                }
            }
        }
    }
    if !meta.trim().is_empty()
        && let Ok(path) = relative(meta.trim())
    {
        if paths.contains(&path) {
            return Ok(Some(path));
        }
        let path = chart.parent().unwrap_or(Path::new("")).join(path);
        if paths.contains(&path) {
            return Ok(Some(path));
        }
    }
    let candidates: Vec<_> = files
        .iter()
        .filter(|p| {
            p.extension()
                .is_some_and(|e| extensions.iter().any(|ext| e.eq_ignore_ascii_case(ext)))
        })
        .collect();
    Ok(if candidates.len() == 1 {
        Some(candidates[0].clone())
    } else {
        None
    })
}
fn milliseconds(text: &str) -> Option<i64> {
    let (mantissa, exponent) = text
        .split_once(['e', 'E'])
        .map_or((text, Some(0)), |(a, b)| (a, b.parse::<i32>().ok()));
    let negative = mantissa.starts_with('-');
    let mantissa = mantissa.trim_start_matches(['-', '+']);
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty() && fraction.is_empty()
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let mut n = format!("{whole}{fraction}").parse::<i128>().ok()?;
    let shift = 3_i32
        .checked_add(exponent?)?
        .checked_sub(fraction.len().try_into().ok()?)?;
    if shift >= 0 {
        n = n.checked_mul(10_i128.checked_pow(shift as u32)?)?;
    } else if shift >= -38 {
        let d = 10_i128.checked_pow((-shift) as u32)?;
        let q = n / d;
        n = q + i128::from(n % d >= (d + 1) / 2);
    } else {
        n = 0;
    }
    if negative {
        n = -n;
    }
    n.try_into().ok()
}
pub(super) fn options(
    manifests: &[&Manifest],
    files: &[PathBuf],
    chart: &Path,
    assets: &Path,
    document: &ChartDocument,
    source_name: &str,
) -> Result<ProjectOptions, ChartError> {
    let meta = &document.as_json()["META"];
    let relative_asset = |path: Option<PathBuf>| {
        path.map(|p| {
            p.strip_prefix(assets)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .map_err(|_| invalid("Resource is outside the package asset root"))
        })
        .transpose()
    };
    let mut options = ProjectOptions {
        music: relative_asset(resource(
            manifests,
            files,
            chart,
            &["music", "song"],
            &["ogg", "mp3", "wav", "flac", "m4a", "aac"],
            meta["song"].as_str().unwrap_or(""),
        )?)?,
        illustration: relative_asset(resource(
            manifests,
            files,
            chart,
            &["illustration", "picture", "background"],
            &["png", "jpg", "jpeg", "webp", "bmp"],
            meta["background"].as_str().unwrap_or(""),
        )?)?,
        ..ProjectOptions::default()
    };
    for key in ["name", "composer", "charter", "level"] {
        let authored = meta[key]
            .as_str()
            .unwrap_or(if key == "name" { "Untitled" } else { "" })
            .to_owned();
        let display = first(manifests, key).map(str::to_owned).unwrap_or_else(|| {
            if key == "name" && (authored.trim().is_empty() || authored == "Untitled") {
                let lower = source_name.to_ascii_lowercase();
                if lower.ends_with(".zip") || lower.ends_with(".pez") {
                    source_name[..source_name.len() - 4].to_owned()
                } else {
                    source_name.to_owned()
                }
            } else {
                authored.clone()
            }
        });
        options.authored.insert(key.into(), authored);
        options.display.insert(key.into(), display);
    }
    let yaml: Vec<_> = manifests.iter().copied().filter(|m| m.yaml).collect();
    options.manifest_offset_ms = first(&yaml, "offset")
        .and_then(|v| milliseconds(v.trim()))
        .unwrap_or(0);
    options.background_dim = first(&yaml, "backgrounddim")
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite() && (0. ..=1.).contains(v));
    options.use_rpe_170_speed =
        first(&yaml, "userpe170speed").is_some_and(|v| v.eq_ignore_ascii_case("true"));
    options.note_uniform_scale =
        first(&yaml, "noteuniformscale").is_some_and(|v| v.eq_ignore_ascii_case("true"));
    options.hold_partial_cover =
        first(&yaml, "holdpartialcover").is_some_and(|v| v.eq_ignore_ascii_case("true"));
    options.force_aspect_ratio =
        first(&yaml, "forceaspectratio").is_some_and(|v| v.eq_ignore_ascii_case("true"));
    options.use_attach_ui_fix =
        first(&yaml, "useattachuifix").map(|v| v.eq_ignore_ascii_case("true"));
    options.aspect_ratio = first(&yaml, "aspectratio")
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v > 0.);
    Ok(options)
}
