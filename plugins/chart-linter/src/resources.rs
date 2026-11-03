use crate::lint::{Report, Severity};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct Reference {
    pub name: String,
    pub source: String,
    pub directory: String,
    pub optional: bool,
}
#[derive(Default, Debug)]
pub struct Resources {
    pub references: Vec<Reference>,
    pub files: BTreeSet<String>,
    pub visited: BTreeSet<String>,
    pending: BTreeMap<(String, String), usize>,
    pub truncated: bool,
    bytes: usize,
    verified: usize,
    folded: BTreeMap<String, Vec<String>>,
    basenames: BTreeMap<String, Vec<String>>,
    indexed: usize,
}
pub fn normalize(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    if path.starts_with('/') || path.contains(':') || path.contains('\0') {
        return None;
    }
    let mut parts = vec![];
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}
pub fn directory(path: &str) -> String {
    path.rsplit_once('/')
        .map_or(String::new(), |(p, _)| p.into())
}
fn file_like(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif", ".json", ".glsl", ".frag", ".vert",
        ".mp3", ".ogg", ".wav", ".flac", ".m4a", ".ttf", ".otf", ".webm", ".mp4",
    ]
    .iter()
    .any(|extension| lower.ends_with(extension))
}
impl Resources {
    pub fn add(&mut self, name: &str, source: &str, relative_to: &str, optional: bool) {
        if name.is_empty() || matches!(name, "line.png" | "tap.mp3" | "flick.mp3" | "drag.mp3") {
            return;
        }
        let key = (relative_to.into(), name.into());
        if let Some(index) = self.pending.get(&key)
            && (!self.references[*index].optional || optional)
        {
            return;
        }
        let bytes = relative_to.len() + source.len() + name.len();
        if self.references.len() >= 20_000 || self.bytes.saturating_add(bytes) > 8 * 1024 * 1024 {
            self.truncated = true;
            return;
        }
        self.bytes += bytes;
        self.pending.insert(key, self.references.len());
        self.references.push(Reference {
            name: name.into(),
            source: source.into(),
            directory: relative_to.into(),
            optional,
        });
    }
    pub fn collect(&mut self, value: &Value, source: &str, relative_to: &str) {
        let mut stack = vec![(value, source.to_owned())];
        while let Some((value, path)) = stack.pop() {
            match value {
                Value::String(name) if file_like(name) => self.add(
                    if path.ends_with("/shader") {
                        name.strip_prefix('/').unwrap_or(name)
                    } else {
                        name
                    },
                    &path,
                    relative_to,
                    false,
                ),
                Value::Array(values) => {
                    for (i, v) in values.iter().enumerate() {
                        stack.push((v, format!("{path}/{i}")))
                    }
                }
                Value::Object(values) => {
                    for (key, v) in values {
                        if !matches!(
                            key.as_str(),
                            "author"
                                | "authors"
                                | "name"
                                | "description"
                                | "skinPresets"
                                | "text"
                                | "textEvents"
                        ) {
                            stack.push((v, format!("{path}/{}", crate::lint::escape(key))));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    pub fn resolve(&self, reference: &Reference) -> Result<Option<String>, String> {
        let root = normalize(&reference.name);
        let local = if reference.directory.is_empty() {
            root.clone()
        } else {
            normalize(&format!("{}/{}", reference.directory, reference.name))
        };
        let Some(root) = root.or_else(|| local.clone()) else {
            return Err("Resource path escapes the project or is not relative".into());
        };
        for candidate in local.iter().chain(std::iter::once(&root)) {
            if self.files.contains(candidate) {
                return Ok(Some(candidate.clone()));
            }
        }
        for candidate in local.iter().chain(std::iter::once(&root)) {
            let matches = self
                .folded
                .get(&candidate.to_lowercase())
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if matches.len() == 1 {
                return Ok(Some(matches[0].clone()));
            }
            if matches.len() > 1 {
                return Err("Resource has multiple case-insensitive matches".into());
            }
        }
        let basename = root.rsplit('/').next().unwrap_or(&root).to_lowercase();
        let matches = self
            .basenames
            .get(&basename)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        match matches {
            [file] => Ok(Some(file.clone())),
            [] if reference.optional => Ok(None),
            [] => Err("Referenced resource is missing".into()),
            _ => Err("Resource basename is ambiguous".into()),
        }
    }
    pub fn verify(&mut self, report: &mut Report) -> Vec<(String, String)> {
        if self.indexed != self.files.len() {
            self.folded.clear();
            self.basenames.clear();
            for file in &self.files {
                self.folded
                    .entry(file.to_lowercase())
                    .or_default()
                    .push(file.clone());
                self.basenames
                    .entry(file.rsplit('/').next().unwrap_or(file).to_lowercase())
                    .or_default()
                    .push(file.clone());
            }
            self.indexed = self.files.len();
        }
        let mut indirect = BTreeMap::new();
        for reference in &self.references[self.verified..] {
            match self.resolve(reference) {
                Ok(Some(file)) => {
                    let extension = file.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
                    if matches!(extension.as_str(), "json" | "glsl" | "vert" | "frag")
                        && !self.visited.contains(&file)
                    {
                        indirect
                            .entry(file)
                            .or_insert_with(|| reference.source.clone());
                    }
                }
                Ok(None) => {}
                Err(error) => report.push(
                    Severity::Error,
                    "resource_missing",
                    &reference.source,
                    None,
                    format!("{error}: {}", reference.name),
                    false,
                ),
            }
        }
        self.verified = self.references.len();
        indirect.into_iter().collect()
    }
    pub fn inspect_text(&mut self, path: &str, source: &str, bytes: &[u8], report: &mut Report) {
        self.visited.insert(path.into());
        let base = directory(path);
        if path.to_ascii_lowercase().ends_with(".json") {
            match serde_json::from_slice::<Value>(bytes) {
                Ok(value) => self.collect(&value, source, &base),
                Err(error) => report.push(
                    Severity::Error,
                    "resource_json",
                    source,
                    None,
                    format!("Invalid resource JSON in {path}: {error}"),
                    false,
                ),
            }
        } else {
            match std::str::from_utf8(bytes) {
                Ok(text) => {
                    for line in text.lines() {
                        let line = line.trim();
                        if let Some(include) = line.strip_prefix("#include")
                            && let Some(value) = include
                                .trim()
                                .strip_prefix(['"', '<'])
                                .and_then(|v| v.split(['"', '>']).next())
                        {
                            self.add(value, source, &base, false)
                        }
                        if let Some(texture) = line
                            .strip_prefix("//")
                            .and_then(|v| v.trim().strip_prefix("@texture"))
                        {
                            for value in texture.split_whitespace().filter(|v| file_like(v)) {
                                self.add(value.trim_matches('"'), source, &base, false)
                            }
                        }
                    }
                }
                Err(error) => report.push(
                    Severity::Error,
                    "resource_text",
                    source,
                    None,
                    format!("Resource {path} is not UTF-8: {error}"),
                    false,
                ),
            }
        }
    }
}

pub fn compatibility(value: &Value, path: &str, target: &str, report: &mut Report) {
    if target == "next" {
        return;
    }
    let nonempty = |key: &str| {
        value.get(key).is_some_and(|v| match v {
            Value::Array(a) => !a.is_empty(),
            Value::Object(o) => !o.is_empty(),
            Value::Null => false,
            _ => true,
        })
    };
    for (key, message) in [
        ("blockAreaList", "Noise Fields"),
        ("globalEvents", "Foreign global events"),
    ] {
        let used = if key == "globalEvents" {
            value[key]["types"].as_object().is_some_and(|types| {
                types.values().any(|t| {
                    t["columns"].as_object().is_some_and(|columns| {
                        columns.values().any(|c| {
                            c["events"]
                                .as_array()
                                .is_some_and(|events| !events.is_empty())
                        })
                    })
                })
            })
        } else {
            nonempty(key)
        };
        if used {
            report.push(
                Severity::Error,
                "compatibility",
                &format!("{path}/{key}"),
                None,
                format!(
                    "{message} require rpe-next-compatible playback; selected target: {target}"
                ),
                false,
            )
        }
    }
}
