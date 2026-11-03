//! Paged, cancelable inspection pipeline; only bounded service payloads are retained.
use super::*;

#[derive(Clone, Debug)]
pub(crate) enum Task {
    Project,
    Meta,
    Bpm(usize),
    Lines,
    Line(usize),
    Layers(usize),
    Notes(usize, usize),
    Events {
        path: String,
        key: String,
        line: Option<usize>,
        layer: Option<usize>,
        offset: usize,
        overlap: bool,
    },
    Aux(String),
    Validate(String),
    Assets(String, usize),
    Open(String, String),
    Read {
        path: String,
        source: String,
        file: u64,
        offset: u64,
    },
    Close(u64),
    Probe {
        line: usize,
        note: usize,
        offsets: Vec<f64>,
    },
}
#[derive(Debug)]
struct Deep {
    line: usize,
    note: usize,
    offsets: VecDeque<f64>,
    points: Vec<PreviewInspectionPoint>,
    refine: Option<(f64, f64)>,
}
#[derive(Debug)]
pub(crate) struct Scan {
    version: DocumentVersion,
    pub(crate) deep: bool,
    project: Option<ProjectInfo>,
    pub(crate) tasks: VecDeque<Task>,
    bpm: Vec<Value>,
    clock: Clock,
    bounds: Bounds,
    notes: Vec<Vec<Note>>,
    parents: Vec<Option<i64>>,
    cuts: Vec<Vec<f64>>,
    tracks: BTreeMap<String, Track>,
    resources: Resources,
    file_bytes: Vec<u8>,
    pub(crate) open_file: Option<u64>,
    assets_done: bool,
    pub(crate) assets_failed: bool,
    asset_entries: usize,
    asset_path_bytes: usize,
    resources_done: bool,
    deep_state: Option<Deep>,
    deep_line: usize,
    deep_note: usize,
    sample_limit: usize,
    pub(crate) deep_checked: usize,
    pub(crate) failed: bool,
    pub(crate) native_failed: bool,
}
impl Scan {
    pub(crate) fn new(version: DocumentVersion, deep: bool) -> Self {
        Self {
            version,
            deep,
            project: None,
            tasks: VecDeque::from([
                Task::Project,
                Task::Meta,
                Task::Bpm(0),
                Task::Lines,
                Task::Aux("/blockAreaList".into()),
                Task::Aux("/noiseAreaLayers".into()),
                Task::Aux("/globalEvents".into()),
                Task::Aux("/effects".into()),
                Task::Aux("/shaderEvents".into()),
                Task::Aux("/phiStudioExtra".into()),
            ]),
            bpm: vec![],
            clock: Clock::default(),
            bounds: Bounds::default(),
            notes: vec![],
            parents: vec![],
            cuts: vec![],
            tracks: BTreeMap::new(),
            resources: Resources::default(),
            file_bytes: vec![],
            open_file: None,
            assets_done: false,
            assets_failed: false,
            asset_entries: 0,
            asset_path_bytes: 0,
            resources_done: false,
            deep_state: None,
            deep_line: 0,
            deep_note: 0,
            sample_limit: MAX_PREVIEW_INSPECTION_POINTS,
            deep_checked: 0,
            failed: false,
            native_failed: false,
        }
    }
    fn base(&self) -> String {
        self.project
            .as_ref()
            .and_then(|p| p.chart_name.as_deref())
            .map(resources::directory)
            .unwrap_or_default()
    }
    pub(crate) fn event_task(line: usize, key: &str, layer: Option<usize>) -> Task {
        let parent = layer.map_or_else(
            || format!("/judgeLineList/{line}/extended"),
            |layer| format!("/judgeLineList/{line}/eventLayers/{layer}"),
        );
        Task::Events {
            path: format!("{parent}/{key}"),
            key: key.into(),
            line: Some(line),
            layer,
            offset: 0,
            overlap: false,
        }
    }
    pub(crate) fn request(&self, task: &Task) -> (&'static str, &'static str, Value) {
        let query = |path: &str| {
            json!(DocumentQuery {
                version: self.version,
                path: path.into()
            })
        };
        let rows = |path: String, offset: usize, fields: &[&str]| {
            json!(DocumentRows {
                version: self.version,
                path,
                offset,
                limit: 128,
                fields: fields.iter().map(|s| (*s).into()).collect()
            })
        };
        match task {
            Task::Project => (
                "project",
                "info",
                json!(ProjectInfoQuery {
                    version: self.version
                }),
            ),
            Task::Meta => (
                "document",
                "fields",
                json!(DocumentFields {
                    version: self.version,
                    path: "/META".into(),
                    fields: ["offset", "song", "bg", "background", "effects"]
                        .map(String::from)
                        .into()
                }),
            ),
            Task::Bpm(offset) => (
                "document",
                "rows",
                rows("/BPMList".into(), *offset, &["startTime", "bpm"]),
            ),
            Task::Lines => ("document", "inspect", query("/judgeLineList")),
            Task::Line(line) => (
                "document",
                "fields",
                json!(DocumentFields {
                    version: self.version,
                    path: format!("/judgeLineList/{line}"),
                    fields: [
                        "father",
                        "bpmfactor",
                        "anchor",
                        "zOrder",
                        "Texture",
                        "alphaControl",
                        "posControl",
                        "sizeControl",
                        "skewControl",
                        "yControl"
                    ]
                    .map(String::from)
                    .into()
                }),
            ),
            Task::Layers(line) => (
                "document",
                "inspect",
                query(&format!("/judgeLineList/{line}/eventLayers")),
            ),
            Task::Notes(line, offset) => (
                "document",
                "rows",
                rows(
                    format!("/judgeLineList/{line}/notes"),
                    *offset,
                    &NOTE_FIELDS,
                ),
            ),
            Task::Events { path, offset, .. } => (
                "document",
                "rows",
                rows(path.clone(), *offset, &EVENT_FIELDS),
            ),
            Task::Aux(path) => ("document", "read", query(path)),
            Task::Validate(path) => ("document", "validate", query(path)),
            Task::Assets(directory, offset) => (
                "assets",
                "list",
                json!(AssetList {
                    source: AssetSource::Project,
                    directory: directory.clone(),
                    offset: *offset,
                    limit: 256
                }),
            ),
            Task::Open(path, _) => (
                "assets",
                "open",
                json!(AssetOpen {
                    source: AssetSource::Project,
                    path: path.clone()
                }),
            ),
            Task::Read { file, offset, .. } => (
                "assets",
                "read",
                json!(AssetRead {
                    file: *file,
                    offset: *offset,
                    limit: 256 * 1024
                }),
            ),
            Task::Close(file) => ("assets", "close", json!(AssetHandle { file: *file })),
            Task::Probe {
                line,
                note,
                offsets,
            } => (
                "preview",
                "inspect",
                json!(PreviewInspection {
                    version: self.version,
                    line: *line,
                    note: *note,
                    offsets: offsets.clone()
                }),
            ),
        }
    }
    pub(crate) fn next(&mut self, report: &mut Report) -> Option<Task> {
        if let Some(task) = self.tasks.pop_front() {
            return Some(task);
        }
        if !self.assets_done {
            self.assets_done = true;
            parents(&self.parents, report);
            for cuts in &mut self.cuts {
                cuts.sort_by(f64::total_cmp);
                cuts.dedup_by(|a, b| *a == *b)
            }
            if self.project.as_ref().is_some_and(|p| p.has_assets) {
                return Some(Task::Assets(String::new(), 0));
            }
            if !self.resources.references.is_empty() {
                report.push(
                    Severity::Info,
                    "resource_unavailable",
                    "",
                    None,
                    "Project resources are unavailable; file existence was not checked",
                    false,
                )
            }
        }
        if !self.resources_done {
            let pending =
                if !self.assets_failed && self.project.as_ref().is_some_and(|p| p.has_assets) {
                    self.resources.verify(report)
                } else {
                    vec![]
                };
            if self.deep && !pending.is_empty() {
                for (path, source) in pending {
                    self.resources.visited.insert(path.clone());
                    self.tasks.push_back(Task::Open(path, source))
                }
                return self.tasks.pop_front();
            }
            self.resources_done = true;
            if self.resources.truncated {
                self.failed = true;
                report.push(Severity::Error,"scan_coverage","",None,"Resource references exceeded the 20,000-entry / 8 MiB budget; resource results are incomplete",false);
            }
        }
        if !self.deep || self.native_failed {
            return None;
        }
        loop {
            if let Some(active) = &mut self.deep_state {
                if let Some((lo, hi)) = active.refine {
                    if hi - lo > 0.001 {
                        return Some(Task::Probe {
                            line: active.line,
                            note: active.note,
                            offsets: vec![(lo + hi) / 2.],
                        });
                    }
                    let n = &self.notes[active.line][active.note];
                    if -hi < 0.299 {
                        report.deep(
                            Severity::Info,
                            "short_read",
                            n,
                            format!(
                                "Visible for only {:.0} ms immediately before judgment",
                                -hi * 1000.
                            ),
                        )
                    }
                    self.deep_state = None;
                    self.deep_checked += 1;
                    continue;
                }
                if !active.offsets.is_empty() {
                    let offsets = active
                        .offsets
                        .drain(..active.offsets.len().min(self.sample_limit))
                        .collect();
                    return Some(Task::Probe {
                        line: active.line,
                        note: active.note,
                        offsets,
                    });
                }
                active.points.sort_by(|a, b| b.offset.total_cmp(&a.offset));
                let n = &self.notes[active.line][active.note];
                if let Some(judged) = active.points.first() {
                    if !judged.inside {
                        report.deep(
                            Severity::Warning,
                            "out_screen",
                            n,
                            "Note's judgment position is outside the native preview",
                        )
                    }
                    if judged.width == Some(0.) && n.size != 0. {
                        report.deep(
                            Severity::Warning,
                            "zero_width",
                            n,
                            "Native distance controls reduce note width to zero",
                        )
                    }
                    if judged.inside {
                        if !judged.visible {
                            report.deep(
                                Severity::Info,
                                "short_read",
                                n,
                                "Note is invisible at its judgment time (0 ms readability)",
                            )
                        } else {
                            let mut previous = 0.;
                            for point in &active.points {
                                if !point.visible {
                                    active.refine = Some((point.offset, previous));
                                    break;
                                }
                                previous = point.offset;
                            }
                            if active.refine.is_some() {
                                continue;
                            }
                        }
                    }
                }
                self.deep_state = None;
                self.deep_checked += 1;
                continue;
            }
            while self.deep_line < self.notes.len()
                && self.deep_note >= self.notes[self.deep_line].len()
            {
                self.deep_line += 1;
                self.deep_note = 0;
            }
            let note = self.notes.get(self.deep_line)?.get(self.deep_note)?;
            let position = self.deep_note;
            self.deep_note += 1;
            if note.fake {
                continue;
            }
            let mut offsets: Vec<f64> = (0..=60).map(|n| -f64::from(n) * 0.005).collect();
            let mut visited = std::collections::BTreeSet::new();
            let mut ancestor = Some(note.line);
            while let Some(line) = ancestor.filter(|l| *l < self.cuts.len() && visited.insert(*l)) {
                let cuts = &self.cuts[line];
                let first = cuts.partition_point(|t| *t < note.start - 0.3);
                let last = cuts.partition_point(|t| *t <= note.start);
                offsets.extend(
                    cuts[first..last]
                        .iter()
                        .flat_map(|t| [t - note.start, t - note.start - 1e-7]),
                );
                ancestor = self.parents[line].and_then(|p| usize::try_from(p).ok());
            }
            self.deep_state = Some(Deep {
                line: note.line,
                note: position,
                offsets: unique_offsets(offsets.into_iter()).into(),
                points: vec![],
                refine: None,
            });
        }
    }
    pub(crate) fn retry_inspection(&mut self, offsets: &[f64]) -> bool {
        if offsets.len() <= 1 {
            return false;
        }
        let Some(active) = &mut self.deep_state else {
            return false;
        };
        self.sample_limit = (offsets.len() / 2).max(1);
        for offset in offsets.iter().rev() {
            active.offsets.push_front(*offset)
        }
        true
    }
    pub(crate) fn accept(
        &mut self,
        task: Task,
        value: Value,
        report: &mut Report,
        target: &str,
    ) -> Result<(), String> {
        let parse = |v: Value| {
            serde_json::from_value::<DocumentValue>(v)
                .map(|v| v.value)
                .map_err(|e| e.to_string())
        };
        let page = |v: Value| serde_json::from_value::<DocumentItems>(v).map_err(|e| e.to_string());
        match task {
            Task::Project => {
                let project: ProjectInfo =
                    serde_json::from_value(value).map_err(|e| e.to_string())?;
                if project.version != self.version {
                    return Err("Project document changed".into());
                }
                self.bounds = Bounds {
                    offset: project.music_offset,
                    duration: project.music_duration,
                };
                for name in [&project.music, &project.illustration]
                    .into_iter()
                    .flatten()
                {
                    self.resources.add(name, "/META", "", false)
                }
                self.project = Some(project);
            }
            Task::Meta => {
                let value = parse(value)?;
                if value.get("offset").is_some_and(|v| number(v).is_none()) {
                    report.push(
                        Severity::Error,
                        "offset",
                        "/META/offset",
                        None,
                        "Music offset must be finite milliseconds",
                        false,
                    )
                }
                for key in ["song", "bg", "background"] {
                    if let Some(name) = value[key].as_str() {
                        self.resources
                            .add(name, &format!("/META/{key}"), &self.base(), false)
                    }
                }
                if let Some(effects) = value.get("effects") {
                    self.inspect_aux(effects, "/META/effects", report, target)
                }
            }
            Task::Bpm(_) => {
                let page = page(value)?;
                if self.bpm.len() + page.items.len() > 10_000 {
                    return Err("BPM list exceeds the 10,000-entry scan budget".into());
                }
                self.bpm.extend(page.items);
                if let Some(next) = page.next_offset {
                    self.tasks.push_front(Task::Bpm(next))
                } else {
                    self.clock = Clock::new(&self.bpm, report);
                    self.bpm.clear()
                }
            }
            Task::Lines => {
                if value["kind"] != "array" {
                    return Err("judgeLineList must be an array".into());
                }
                let total = value["length"].as_u64().ok_or("Missing line count")? as usize;
                if total > 20_000 {
                    return Err("Chart exceeds the 20,000-line scan budget".into());
                }
                self.notes.resize_with(total, Vec::new);
                self.cuts.resize_with(total, Vec::new);
                self.parents.resize(total, Some(-1));
                for line in (0..total).rev() {
                    self.tasks.push_front(Task::Line(line))
                }
            }
            Task::Line(line) => {
                let value = parse(value)?;
                line_values(&value, line, report);
                self.parents[line] = value.get("father").map_or(Some(-1), Value::as_i64);
                if let Some(texture) = value["Texture"].as_str() {
                    self.resources.add(
                        texture,
                        &format!("/judgeLineList/{line}/Texture"),
                        &self.base(),
                        false,
                    )
                }
                for key in EXTRA.into_iter().rev() {
                    self.tasks.push_front(Self::event_task(line, key, None))
                }
                self.tasks.push_front(Task::Layers(line));
                self.tasks.push_front(Task::Notes(line, 0));
            }
            Task::Layers(line) => {
                let total = value["length"].as_u64().unwrap_or(0) as usize;
                if total > 1024 {
                    return Err("Line exceeds the 1,024-layer scan budget".into());
                }
                for layer in (0..total.max(1)).rev() {
                    for key in BASE.into_iter().rev() {
                        self.tasks
                            .push_front(Self::event_task(line, key, Some(layer)))
                    }
                }
            }
            Task::Notes(line, _) => {
                let page = page(value)?;
                if report.checked_notes + report.checked_events + page.items.len() > 500_000 {
                    return Err("Chart exceeds the 500,000-object scan budget".into());
                }
                for (index, value) in page.items.into_iter().enumerate() {
                    let index = page.offset + index;
                    let path = format!("/judgeLineList/{line}/notes/{index}");
                    for key in ["hitsound", "hitSound"] {
                        if let Some(name) = value[key].as_str() {
                            self.resources.add(name, &path, &self.base(), false)
                        }
                    }
                    if let Some(note) =
                        inspect_note(&value, line, index, &self.clock, &self.bounds, report)
                    {
                        self.notes[line].push(note)
                    }
                }
                if let Some(next) = page.next_offset {
                    self.tasks.push_front(Task::Notes(line, next))
                } else {
                    note_overlaps(&self.notes[line], report);
                    if self.deep {
                        timing(&self.notes[line], report)
                    } else {
                        self.notes[line].clear()
                    }
                }
            }
            Task::Events {
                path,
                key,
                line,
                layer,
                overlap,
                ..
            } => {
                let page = page(value)?;
                if report.checked_notes + report.checked_events + page.items.len() > 500_000 {
                    return Err("Chart exceeds the 500,000-object scan budget".into());
                }
                let base = self.base();
                let track = self.tracks.entry(path.clone()).or_default();
                for (index, value) in page.items.iter().enumerate() {
                    let source = format!("{path}/{}", page.offset + index);
                    inspect_event(
                        value,
                        EventSite {
                            path: &source,
                            key: &key,
                            layer,
                            index: page.offset + index,
                        },
                        &self.clock,
                        &self.bounds,
                        track,
                        report,
                    );
                    if let Some(line) = line {
                        for field in ["startTime", "endTime"] {
                            if let Some(t) = beat(&value[field]).and_then(|b| self.clock.seconds(b))
                            {
                                self.cuts[line].push(t)
                            }
                        }
                    }
                    if key != "textEvents" {
                        self.resources.collect(value, &source, &base)
                    }
                    if target != "next"
                        && (value.get("customEasing").is_some()
                            || value.get("trajectory").is_some())
                    {
                        report.push(
                            Severity::Info,
                            "compatibility",
                            &source,
                            beat(&value["startTime"]),
                            format!("Custom curve or trajectory may differ on {target}"),
                            false,
                        );
                    }
                    if target == "phira"
                        && value["bezierPoints"].as_array().is_some_and(|a| {
                            [1, 3]
                                .iter()
                                .any(|i| a.get(*i).and_then(number).is_some_and(|v| v.abs() > 10.))
                        })
                    {
                        report.push(
                            Severity::Warning,
                            "compatibility",
                            &source,
                            beat(&value["startTime"]),
                            "Extreme Bezier Y controls may behave differently in Phira",
                            false,
                        );
                    }
                }
                if let Some(next) = page.next_offset {
                    self.tasks.push_front(Task::Events {
                        path,
                        key,
                        line,
                        layer,
                        overlap,
                        offset: next,
                    })
                } else {
                    let track = self.tracks.remove(&path).unwrap_or_default();
                    finish_track(track, &path, &key, layer, overlap, report)
                }
            }
            Task::Aux(path) => {
                let value = parse(value)?;
                self.inspect_aux(&value, &path, report, target);
                if path == "/blockAreaList" {
                    if let Some(fields) = value.as_array() {
                        for index in 0..fields.len() {
                            self.tasks
                                .push_back(Task::Validate(format!("/blockAreaList/{index}")))
                        }
                    } else {
                        report.push(
                            Severity::Error,
                            "noise_geometry",
                            &path,
                            None,
                            "Noise Fields must be an array",
                            false,
                        )
                    }
                }
                if path == "/phiStudioExtra" {
                    self.tasks.push_back(Task::Validate(path))
                }
            }
            Task::Validate(path) => {
                let validation: DocumentValidation =
                    serde_json::from_value(value).map_err(|e| e.to_string())?;
                if !validation.valid {
                    report.push(
                        Severity::Error,
                        "native_validation",
                        &path,
                        None,
                        validation
                            .message
                            .unwrap_or_else(|| "Native object validation failed".into()),
                        false,
                    )
                }
            }
            Task::Assets(directory, _) => {
                let page: AssetPage = serde_json::from_value(value).map_err(|e| e.to_string())?;
                if self
                    .project
                    .as_ref()
                    .is_some_and(|p| page.project != Some(p.assets))
                {
                    return Err("Project resources changed during the scan".into());
                }
                for entry in page.entries {
                    self.asset_entries += 1;
                    self.asset_path_bytes += entry.path.len();
                    if self.asset_entries > 20_000 || self.asset_path_bytes > 8 * 1024 * 1024 {
                        return Err(
                            "Project asset index exceeds the 20,000-entry / 8 MiB scan budget"
                                .into(),
                        );
                    }
                    if entry.directory {
                        self.tasks.push_back(Task::Assets(entry.path, 0))
                    } else {
                        self.resources.files.insert(entry.path);
                    }
                }
                if let Some(next) = page.next_offset {
                    self.tasks.push_front(Task::Assets(directory, next))
                }
            }
            Task::Open(path, source) => {
                let file: AssetFile = serde_json::from_value(value).map_err(|e| e.to_string())?;
                self.open_file = Some(file.id);
                if self
                    .project
                    .as_ref()
                    .is_some_and(|p| file.project != Some(p.assets))
                {
                    return Err("Resource snapshot changed during the scan".into());
                }
                if file.size > 4 * 1024 * 1024 {
                    self.tasks.push_front(Task::Close(file.id));
                    return Err(format!(
                        "Resource {path} exceeds the 4 MiB indirect-inspection budget"
                    ));
                }
                self.file_bytes.clear();
                self.tasks.push_front(Task::Read {
                    path,
                    source,
                    file: file.id,
                    offset: 0,
                });
            }
            Task::Read {
                path, source, file, ..
            } => {
                let chunk: FileChunk = serde_json::from_value(value).map_err(|e| e.to_string())?;
                self.file_bytes.extend(decode_file_chunk(&chunk.data)?);
                if self.file_bytes.len() > 4 * 1024 * 1024 {
                    self.tasks.push_front(Task::Close(file));
                    return Err("Indirect resource exceeds inspection budget".into());
                }
                if let Some(offset) = chunk.next_offset {
                    self.tasks.push_front(Task::Read {
                        path,
                        source,
                        file,
                        offset,
                    })
                } else {
                    self.resources
                        .inspect_text(&path, &source, &self.file_bytes, report);
                    self.file_bytes.clear();
                    self.tasks.push_front(Task::Close(file))
                }
            }
            Task::Close(_) => self.open_file = None,
            Task::Probe { offsets, .. } => {
                let result: PreviewInspectionResult =
                    serde_json::from_value(value).map_err(|e| e.to_string())?;
                if result.version != self.version {
                    return Err("Native preview document changed".into());
                }
                let active = self.deep_state.as_mut().ok_or("Deep scan was canceled")?;
                if let Some((lo, hi)) = active.refine {
                    let visible = result
                        .points
                        .first()
                        .ok_or("Preview returned no points")?
                        .visible;
                    active.refine = Some(if visible {
                        (lo, offsets[0])
                    } else {
                        (offsets[0], hi)
                    });
                } else {
                    active.points.extend(result.points)
                }
            }
        }
        Ok(())
    }
}

mod auxiliary;
