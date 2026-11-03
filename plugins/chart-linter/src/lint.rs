//! Independent diagnostic rules. Source values are inspected, never normalized.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const BASE: [&str; 5] = [
    "moveXEvents",
    "moveYEvents",
    "rotateEvents",
    "alphaEvents",
    "speedEvents",
];
pub const EXTRA: [&str; 7] = [
    "scaleXEvents",
    "scaleYEvents",
    "colorEvents",
    "textEvents",
    "inclineEvents",
    "gifEvents",
    "paintEvents",
];
pub const NOTE_FIELDS: [&str; 19] = [
    "type",
    "startTime",
    "endTime",
    "positionX",
    "above",
    "isFake",
    "speed",
    "size",
    "alpha",
    "yOffset",
    "visibleTime",
    "judgeArea",
    "tint",
    "color",
    "tintHitEffects",
    "hitsound",
    "hitSound",
    "customEasing",
    "trajectory",
];
pub const EVENT_FIELDS: [&str; 17] = [
    "startTime",
    "endTime",
    "start",
    "end",
    "easingType",
    "easingLeft",
    "easingRight",
    "bezier",
    "bezierPoints",
    "linkgroup",
    "inst",
    "transformStep",
    "customEasing",
    "trajectory",
    "vars",
    "shader",
    "name",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Issue {
    pub severity: Severity,
    pub code: String,
    pub category: String,
    pub path: String,
    pub beat: Option<f64>,
    pub message: String,
    pub removable: bool,
    pub deep: bool,
}
#[derive(Default, Clone, Debug)]
pub struct Report {
    pub issues: Vec<Issue>,
    pub counts: [usize; 3],
    pub checked_notes: usize,
    pub checked_events: usize,
    pub complete: bool,
    pub deep_complete: bool,
    retained_bytes: usize,
}
impl Report {
    pub fn push(
        &mut self,
        severity: Severity,
        code: &str,
        path: &str,
        beat: Option<f64>,
        message: impl Into<String>,
        removable: bool,
    ) {
        self.counts[severity as usize] += 1;
        let message = message.into();
        let bytes = code.len() + path.len() + message.len() + 128;
        // Counts remain exact even when a pathological chart exceeds the UI's
        // retained row budget. The panel explicitly displays this limit.
        if self.issues.len() < 10_000
            && self.retained_bytes.saturating_add(bytes) <= 8 * 1024 * 1024
        {
            self.retained_bytes += bytes;
            let category = if path.contains("/notes/") {
                "notes"
            } else if path.contains("Events") || path.contains("/eventLayers") {
                "events"
            } else if code.starts_with("resource") {
                "resources"
            } else {
                "chart"
            };
            self.issues.push(Issue {
                severity,
                code: code.into(),
                category: category.into(),
                path: path.into(),
                beat,
                message,
                removable,
                deep: false,
            });
        }
    }
    pub fn deep(
        &mut self,
        severity: Severity,
        code: &str,
        note: &Note,
        message: impl Into<String>,
    ) {
        let before = self.issues.len();
        self.push(
            severity,
            code,
            &note.path(),
            Some(note.beat),
            message,
            false,
        );
        if let Some(issue) = self.issues.get_mut(before) {
            issue.deep = true
        }
    }
}
pub fn number(value: &Value) -> Option<f64> {
    value.as_f64().filter(|v| v.is_finite())
}
pub fn numeric_bool(value: &Value) -> Option<f64> {
    value
        .as_bool()
        .map(|v| f64::from(u8::from(v)))
        .or_else(|| number(value))
}
pub fn beat(value: &Value) -> Option<f64> {
    let a = value.as_array()?;
    if a.len() != 3 {
        return None;
    }
    let w = a[0].as_i64()?;
    let n = a[1].as_i64()?;
    let d = a[2].as_i64()?;
    if d <= 0 {
        return None;
    }
    let value = w as f64 + n as f64 / d as f64;
    value.is_finite().then_some(value)
}
fn rgb(v: &Value) -> bool {
    v.as_array().is_some_and(|a| {
        a.len() == 3
            && a.iter()
                .all(|v| number(v).is_some_and(|v| (0. ..=255.).contains(&v)))
    })
}
pub fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}
fn json_value(value: &Value) -> bool {
    let mut stack = vec![value];
    while let Some(v) = stack.pop() {
        match v {
            Value::String(_) | Value::Bool(_) => {}
            Value::Number(_) => {
                if number(v).is_none() {
                    return false;
                }
            }
            Value::Array(a) => stack.extend(a),
            _ => return false,
        }
    }
    true
}

#[derive(Clone, Debug)]
pub struct Note {
    pub line: usize,
    pub index: usize,
    pub kind: u8,
    pub beat: f64,
    pub end_beat: f64,
    pub start: f64,
    pub end: f64,
    pub x: f64,
    pub above: bool,
    pub fake: bool,
    pub size: f64,
}
impl Note {
    pub fn path(&self) -> String {
        format!("/judgeLineList/{}/notes/{}", self.line, self.index)
    }
}

/// A diagnostic clock, independent of rendering. The host's native preview
/// supplies authoritative judgment times during deep inspection.
#[derive(Default, Debug)]
pub struct Clock {
    points: Vec<(f64, f64, f64)>,
}
impl Clock {
    pub fn new(values: &[Value], report: &mut Report) -> Self {
        let mut points = vec![];
        for (i, value) in values.iter().enumerate() {
            if let (Some(beat), Some(bpm)) = (
                beat(&value["startTime"]),
                number(&value["bpm"]).filter(|v| *v > 0.),
            ) {
                points.push((beat, bpm, i));
            } else {
                report.push(
                    Severity::Error,
                    "bpm",
                    &format!("/BPMList/{i}"),
                    None,
                    "BPM must be positive and its beat must be a valid fraction",
                    false,
                );
            }
        }
        points.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
        let mut keys: Vec<(f64, f64, f64)> = vec![];
        for (beat, bpm, index) in points {
            if let Some(last) = keys.last_mut()
                && last.0 == beat
            {
                report.push(
                    Severity::Info,
                    "bpm_duplicate",
                    &format!("/BPMList/{index}"),
                    Some(beat),
                    "Multiple BPM entries at the same beat; the last entry wins",
                    true,
                );
                last.1 = bpm;
                continue;
            }
            let time = keys.last().map_or(0., |(b, p, t)| t + (beat - b) * 60. / p);
            keys.push((beat, bpm, time));
        }
        if keys.is_empty() {
            report.push(
                Severity::Error,
                "bpm_empty",
                "/BPMList",
                None,
                "At least one valid BPM entry is required",
                false,
            )
        }
        Self { points: keys }
    }
    pub fn seconds(&self, beat: f64) -> Option<f64> {
        let index = self
            .points
            .partition_point(|p| p.0 <= beat)
            .saturating_sub(1);
        let (b, p, t) = self.points.get(index)?;
        Some(t + (beat - b) * 60. / p)
    }
}
#[derive(Default, Debug)]
pub struct Bounds {
    pub offset: f64,
    pub duration: Option<f64>,
}
impl Bounds {
    pub fn outside(&self, start: f64, end: f64, interval: bool) -> bool {
        (if interval { end } else { start }) + self.offset < -0.01000001
            || self.duration.is_some_and(|d| {
                d.is_finite()
                    && d > 0.
                    && (if interval { start } else { end }) + self.offset > d + 0.05000001
            })
    }
}

pub fn inspect_note(
    value: &Value,
    line: usize,
    index: usize,
    clock: &Clock,
    bounds: &Bounds,
    report: &mut Report,
) -> Option<Note> {
    report.checked_notes += 1;
    let path = format!("/judgeLineList/{line}/notes/{index}");
    let (Some(start), Some(end)) = (beat(&value["startTime"]), beat(&value["endTime"])) else {
        report.push(
            Severity::Error,
            "beat",
            &path,
            None,
            "Note has an invalid start or end beat",
            true,
        );
        return None;
    };
    let mut emit = |s, c, m| report.push(s, c, &path, Some(start), m, true);
    let kind = value["type"].as_u64().filter(|v| (1..=4).contains(v));
    let x = number(&value["positionX"]);
    if kind.is_none()
        || x.is_none()
        || [
            "speed",
            "size",
            "alpha",
            "yOffset",
            "visibleTime",
            "judgeArea",
        ]
        .iter()
        .any(|key| value.get(key).is_some_and(|v| number(v).is_none()))
    {
        emit(
            Severity::Error,
            "note_value",
            "Note type or numeric properties are invalid",
        );
    }
    for (key, maximum) in [("above", 2.), ("isFake", 1.)] {
        if value.get(key).is_some_and(|v| {
            numeric_bool(v).is_none_or(|v| v.fract() != 0. || !(0. ..=maximum).contains(&v))
        }) {
            emit(
                Severity::Error,
                "note_flag",
                "Note side or fake flag is invalid",
            );
        }
    }
    for key in ["tint", "color", "tintHitEffects"] {
        if value.get(key).is_some_and(|v| !rgb(v)) {
            emit(
                Severity::Error,
                "note_color",
                "Note RGB color must contain three channels from 0 to 255",
            )
        }
    }
    if end < start {
        emit(Severity::Error, "note_time", "Note ends before it starts")
    }
    if kind == Some(2) && end == start {
        emit(Severity::Warning, "hold_zero", "Hold has zero duration")
    }
    if number(&value["size"]) == Some(0.) {
        emit(Severity::Warning, "size_zero", "Note size is zero")
    }
    if number(&value["size"]).is_some_and(|n| n < 0.)
        || number(&value["visibleTime"]).is_some_and(|n| n < 0.)
    {
        emit(
            Severity::Warning,
            "note_range",
            "Note size or visible time is negative",
        );
    }
    if number(&value["alpha"]).is_some_and(|n| n > 255.) {
        emit(Severity::Warning, "alpha_range", "Note alpha exceeds 255")
    }
    let fake = numeric_bool(&value["isFake"]) == Some(1.);
    if !fake && x.is_some_and(|x| x.abs() > 700.) {
        emit(
            Severity::Info,
            "x_range",
            "Note X lies outside the usual chart area",
        )
    }
    let seconds = clock.seconds(start)?;
    let end_seconds = clock.seconds(end)?;
    if bounds.outside(seconds, end_seconds, false) {
        emit(
            Severity::Error,
            "audio_range",
            "Note lies outside the available music interval",
        )
    }
    Some(Note {
        line,
        index,
        kind: kind? as u8,
        beat: start,
        end_beat: end,
        start: seconds,
        end: end_seconds,
        x: x?,
        above: numeric_bool(&value["above"]).unwrap_or(1.) == 1.,
        fake,
        size: number(&value["size"]).unwrap_or(1.),
    })
}
pub fn note_overlaps(notes: &[Note], report: &mut Report) {
    let mut entries: Vec<_> = notes
        .iter()
        .filter(|n| !n.fake && matches!(n.kind, 1..=3))
        .collect();
    entries.sort_by(|a, b| {
        a.above
            .cmp(&b.above)
            .then(a.x.total_cmp(&b.x))
            .then((a.kind == 3).cmp(&(b.kind == 3)))
            .then(a.beat.total_cmp(&b.beat))
            .then(a.index.cmp(&b.index))
    });
    let mut previous: Option<&Note> = None;
    let mut furthest = f64::NEG_INFINITY;
    for note in entries {
        let same = previous.is_some_and(|n| {
            n.above == note.above && n.x == note.x && (n.kind == 3) == (note.kind == 3)
        });
        if !same {
            furthest = f64::NEG_INFINITY
        }
        let overlap = if note.kind == 3 {
            same && previous.is_some_and(|n| n.beat == note.beat)
        } else {
            note.beat <= furthest
        };
        if overlap {
            report.push(
                Severity::Error,
                "note_overlap",
                &note.path(),
                Some(note.beat),
                "Note overlaps another note at the same X and side",
                true,
            )
        }
        furthest = furthest.max(if note.kind == 2 {
            note.end_beat.max(note.beat)
        } else {
            note.beat
        });
        previous = Some(note);
    }
}

#[derive(Default, Debug)]
pub struct Track {
    pub intervals: Vec<(f64, f64, usize)>,
    pub initial: bool,
}
pub struct EventSite<'a> {
    pub path: &'a str,
    pub key: &'a str,
    pub layer: Option<usize>,
    pub index: usize,
}
pub fn inspect_event(
    value: &Value,
    site: EventSite<'_>,
    clock: &Clock,
    bounds: &Bounds,
    track: &mut Track,
    report: &mut Report,
) {
    let EventSite {
        path,
        key,
        layer,
        index,
    } = site;
    report.checked_events += 1;
    let (Some(start), Some(end)) = (beat(&value["startTime"]), beat(&value["endTime"])) else {
        report.push(
            Severity::Error,
            "beat",
            path,
            None,
            "Event has an invalid start or end beat",
            true,
        );
        return;
    };
    let mut emit = |s: Severity, c: &str, m: &str| report.push(s, c, path, Some(start), m, true);
    let changed = value["start"] != value["end"];
    track.initial |= start <= 0. && end >= start;
    if end < start
        || (end == start
            && changed
            && !value["inst"].as_bool().unwrap_or(false)
            && !value["transformStep"].as_bool().unwrap_or(false)
            && key != "activeIntervals")
    {
        emit(
            Severity::Error,
            "event_time",
            "Event duration is invalid for its changing value",
        );
    }
    if let (Some(a), Some(b)) = (clock.seconds(start), clock.seconds(end))
        && bounds.outside(a, b, true)
    {
        emit(
            Severity::Error,
            "audio_range",
            "Event is entirely outside the available music interval",
        );
    }
    if key != "activeIntervals" {
        let valid = [&value["start"], &value["end"]]
            .into_iter()
            .all(|v| match key {
                "textEvents" => v.is_string(),
                "colorEvents" => v
                    .as_array()
                    .is_some_and(|a| a.len() == 3 && a.iter().all(|v| number(v).is_some())),
                "paintEvents" => v.is_string() || number(v).is_some() || v.is_object(),
                "jsonEvents" => json_value(v),
                _ => number(v).is_some(),
            });
        if !valid {
            emit(
                Severity::Error,
                "event_value",
                "Event endpoint type is invalid",
            )
        }
        if key == "alphaEvents"
            && [&value["start"], &value["end"]]
                .iter()
                .any(|v| number(v).is_some_and(|v| v > 255.))
        {
            emit(Severity::Warning, "alpha_range", "Event alpha exceeds 255");
        }
        if key == "colorEvents" && [&value["start"], &value["end"]].iter().any(|v| !rgb(v)) {
            emit(
                Severity::Warning,
                "color_range",
                "Event color channels must be between 0 and 255",
            );
        }
        if changed {
            easing(value, &mut emit)
        }
        if value.get("linkgroup").is_some_and(|v| v.as_u64().is_none())
            || value
                .get("bezier")
                .is_some_and(|v| !matches!(numeric_bool(v), Some(0. | 1.)))
        {
            emit(
                Severity::Error,
                "event_value",
                "Bezier flag or link group is invalid",
            );
        }
    }
    if key == "speedEvents" && layer.is_some_and(|l| l > 0) {
        emit(
            Severity::Warning,
            "layer_speed",
            "Speed events outside layer 0 are not consistently supported",
        )
    }
    if end > start {
        track.intervals.push((start, end, index))
    }
}
fn easing(value: &Value, emit: &mut impl FnMut(Severity, &str, &str)) {
    if let Some(custom) = value
        .get("customEasing")
        .filter(|v| !v.is_null() && **v != Value::Bool(false))
    {
        if !custom.is_object() && !custom.is_array() {
            emit(
                Severity::Error,
                "custom_easing",
                "Custom easing must be a structured curve",
            )
        } else {
            emit(
                Severity::Info,
                "custom_easing",
                "Foreign custom easing requires a compatible provider; native preview is authoritative",
            )
        }
    } else if numeric_bool(&value["bezier"]) == Some(1.) {
        let valid = value["bezierPoints"].as_array().is_some_and(|a| {
            a.len() == 4
                && a.iter().all(|v| number(v).is_some())
                && [0, 2]
                    .iter()
                    .all(|i| number(&a[*i]).is_some_and(|v| (0. ..=1.).contains(&v)))
        });
        if !valid {
            emit(
                Severity::Error,
                "bezier",
                "Bezier needs four finite controls with X controls between 0 and 1",
            )
        }
    } else {
        if value
            .get("easingType")
            .is_some_and(|v| v.as_u64().is_none_or(|v| !(1..=29).contains(&v)))
        {
            emit(
                Severity::Error,
                "easing",
                "Easing ID must be an integer from 1 to 29",
            );
        }
        let left = value.get("easingLeft").map_or(Some(0.), number);
        let right = value.get("easingRight").map_or(Some(1.), number);
        if !matches!((left,right),(Some(l),Some(r)) if (0. ..=1.).contains(&l) && (l..=1.).contains(&r))
        {
            emit(
                Severity::Error,
                "easing_bounds",
                "Easing slice must satisfy 0 <= left <= right <= 1",
            );
        }
    }
}
pub fn finish_track(
    mut track: Track,
    path: &str,
    key: &str,
    layer: Option<usize>,
    overlap: bool,
    report: &mut Report,
) {
    if layer == Some(0) && BASE.contains(&key) && !track.initial {
        report.push(
            Severity::Warning,
            "missing_baseline",
            path,
            Some(0.),
            "No valid event establishes this track's value at beat zero",
            false,
        );
    }
    if overlap || key == "paintEvents" {
        return;
    }
    track
        .intervals
        .sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
    let mut furthest = f64::NEG_INFINITY;
    for (start, end, index) in track.intervals {
        if start < furthest {
            report.push(
                Severity::Error,
                "event_overlap",
                &format!("{path}/{index}"),
                Some(start),
                "Positive-duration events overlap on this track",
                true,
            )
        }
        furthest = furthest.max(end);
    }
}

pub fn parents(values: &[Option<i64>], report: &mut Report) {
    let mut state = vec![0u8; values.len()];
    for first in 0..values.len() {
        if state[first] != 0 {
            continue;
        }
        let mut chain = vec![];
        let mut current = first;
        let valid = loop {
            match state[current] {
                1 => break false,
                2 => break true,
                3 => break false,
                _ => {}
            }
            state[current] = 1;
            chain.push(current);
            match values[current] {
                Some(-1) => break true,
                Some(parent) if parent >= 0 && (parent as usize) < values.len() => {
                    current = parent as usize
                }
                _ => break false,
            }
        };
        for index in chain {
            state[index] = if valid { 2 } else { 3 };
            if !valid {
                report.push(
                    Severity::Error,
                    "parent",
                    &format!("/judgeLineList/{index}/father"),
                    None,
                    "Parent chain has an invalid line, cycle or invalid ancestor",
                    false,
                )
            }
        }
    }
}

/// Time and X indexed existence queries; no all-pairs candidate list is built.
struct Nearby<'a> {
    groups: BTreeMap<i64, RangeTree<'a>>,
}
struct RangeTree<'a> {
    values: Vec<&'a Note>,
    size: usize,
    min: Vec<f64>,
    max: Vec<f64>,
    end: Vec<f64>,
    tails: bool,
}
impl<'a> Nearby<'a> {
    fn new(notes: impl Iterator<Item = &'a Note>, tails: bool) -> Self {
        let mut buckets: BTreeMap<i64, Vec<&Note>> = BTreeMap::new();
        for n in notes {
            buckets
                .entry((n.x / 200.).floor() as i64)
                .or_default()
                .push(n)
        }
        Self {
            groups: buckets
                .into_iter()
                .map(|(key, mut values)| {
                    let time = |n: &&Note| if tails { n.end } else { n.start };
                    values.sort_by(|a, b| time(a).total_cmp(&time(b)));
                    let size = values.len().next_power_of_two();
                    let mut tree = RangeTree {
                        values,
                        size,
                        min: vec![f64::INFINITY; size * 2],
                        max: vec![f64::NEG_INFINITY; size * 2],
                        end: vec![f64::NEG_INFINITY; size * 2],
                        tails,
                    };
                    for (i, n) in tree.values.iter().enumerate() {
                        tree.min[size + i] = n.x;
                        tree.max[size + i] = n.x;
                        tree.end[size + i] = n.end;
                    }
                    for i in (1..size).rev() {
                        tree.min[i] = tree.min[i * 2].min(tree.min[i * 2 + 1]);
                        tree.max[i] = tree.max[i * 2].max(tree.max[i * 2 + 1]);
                        tree.end[i] = tree.end[i * 2].max(tree.end[i * 2 + 1]);
                    }
                    (key, tree)
                })
                .collect(),
        }
    }
    fn find(
        &self,
        start: f64,
        end: f64,
        x: f64,
        minimum_end: f64,
        exclude: usize,
    ) -> Option<&'a Note> {
        let bucket = (x / 200.).floor() as i64;
        for k in bucket.saturating_sub(1)..=bucket.saturating_add(1) {
            let Some(tree) = self.groups.get(&k) else {
                continue;
            };
            let time = |n: &&Note| if tree.tails { n.end } else { n.start };
            let first = tree.values.partition_point(|n| time(n) < start);
            let last = tree.values.partition_point(|n| time(n) <= end);
            let mut stack = vec![(1, 0, tree.size)];
            while let Some((node, a, b)) = stack.pop() {
                if b <= first
                    || a >= last
                    || tree.min[node] >= x + 200.
                    || tree.max[node] <= x - 200.
                    || tree.end[node] < minimum_end
                {
                    continue;
                }
                if b - a == 1 {
                    if let Some(n) = tree.values.get(a)
                        && n.index != exclude
                    {
                        return Some(n);
                    }
                } else {
                    let middle = (a + b) / 2;
                    stack.push((node * 2 + 1, middle, b));
                    stack.push((node * 2, a, middle));
                }
            }
        }
        None
    }
}
pub fn timing(notes: &[Note], report: &mut Report) {
    for side in [true, false] {
        let selected = || notes.iter().filter(|n| !n.fake && n.above == side);
        let taps = Nearby::new(selected().filter(|n| n.kind == 1), false);
        let previous = Nearby::new(selected().filter(|n| matches!(n.kind, 3 | 4)), false);
        let holds = Nearby::new(selected().filter(|n| n.kind == 2), false);
        let tails = Nearby::new(selected().filter(|n| n.kind == 2), true);
        for note in selected().filter(|n| matches!(n.kind, 3 | 4)) {
            let Some(tap) = taps.find(
                note.start + 0.04 - 1e-8,
                note.start + 0.16 + 1e-8,
                note.x,
                f64::NEG_INFINITY,
                usize::MAX,
            ) else {
                continue;
            };
            let context = previous
                .find(
                    note.start - 0.16,
                    note.start + 1e-8,
                    note.x,
                    f64::NEG_INFINITY,
                    note.index,
                )
                .is_some()
                || if note.kind == 3 {
                    tails
                        .find(
                            note.start - 1e-8,
                            note.start + 1e-8,
                            note.x,
                            f64::NEG_INFINITY,
                            usize::MAX,
                        )
                        .is_some()
                } else {
                    holds
                        .find(
                            f64::NEG_INFINITY,
                            note.start + 1e-8,
                            note.x,
                            note.start,
                            usize::MAX,
                        )
                        .is_some()
                };
            let elapsed = tap.start - note.start;
            if !context || note.kind == 3 || elapsed <= 0.04 + 1e-8 {
                let severity = if context {
                    Severity::Info
                } else if note.kind == 3 {
                    Severity::Error
                } else {
                    Severity::Warning
                };
                report.deep(
                    severity,
                    if note.kind == 3 {
                        "tap_after_flick"
                    } else {
                        "tap_after_drag"
                    },
                    note,
                    format!(
                        "Tap follows {} after {:.0} ms within 200 X units{}",
                        if note.kind == 3 { "Flick" } else { "Drag" },
                        elapsed * 1000.,
                        if context {
                            " (preceding input provides context)"
                        } else {
                            ""
                        }
                    ),
                );
            }
        }
    }
}

pub fn line_values(value: &Value, index: usize, report: &mut Report) {
    let path = format!("/judgeLineList/{index}");
    if value
        .get("bpmfactor")
        .is_some_and(|v| number(v).is_none_or(|v| v <= 0.))
    {
        report.push(
            Severity::Error,
            "line_value",
            &path,
            None,
            "Line BPM factor must be positive and finite",
            false,
        );
    } else if number(&value["bpmfactor"]).is_some_and(|v| v != 1.) {
        report.push(
            Severity::Info,
            "native_bpm_factor",
            &path,
            None,
            "Original prpr currently ignores bpmfactor; timing checks follow native playback",
            false,
        );
    }
    if value.get("anchor").is_some_and(|v| {
        !v.as_array()
            .is_some_and(|a| a.len() == 2 && a.iter().all(|v| number(v).is_some()))
    }) || value.get("zOrder").is_some_and(|v| number(v).is_none())
    {
        report.push(
            Severity::Error,
            "line_value",
            &path,
            None,
            "Line anchor or Z order is invalid",
            false,
        );
    }
    for (key, field) in [
        ("alphaControl", "alpha"),
        ("posControl", "pos"),
        ("sizeControl", "size"),
        ("skewControl", "skew"),
        ("yControl", "y"),
    ] {
        if let Some(points) = value.get(key) {
            let valid = points.as_array().is_some_and(|a| {
                a.iter().all(|p| {
                    number(&p["x"]).is_some()
                        && number(&p[field]).is_some()
                        && p.get("easing")
                            .is_none_or(|v| v.as_u64().is_some_and(|v| (1..=29).contains(&v)))
                })
            });
            if !valid {
                report.push(
                    Severity::Error,
                    "control",
                    &format!("{path}/{key}"),
                    None,
                    "Distance-control points need finite X/value and easing IDs 1–29",
                    false,
                )
            }
        }
    }
}

pub fn unique_offsets(values: impl Iterator<Item = f64>) -> Vec<f64> {
    let mut bits = BTreeSet::new();
    let mut values: Vec<_> = values
        .filter(|v| v.is_finite() && (-0.30000001..=0.).contains(v) && bits.insert(v.to_bits()))
        .collect();
    values.sort_by(|a, b| b.total_cmp(a));
    values
}

pub fn inspect_aux_interval(
    value: &Value,
    path: &str,
    clock: &Clock,
    bounds: &Bounds,
    report: &mut Report,
) {
    let shader = value.get("shader").is_some();
    let timed = value.get("startTime").is_some() || value.get("endTime").is_some();
    if !shader && timed && value.get("start").is_some() && value.get("end").is_some() {
        let before = report.issues.len();
        inspect_event(
            value,
            EventSite {
                path,
                key: "jsonEvents",
                layer: None,
                index: 0,
            },
            clock,
            bounds,
            &mut Track::default(),
            report,
        );
        for issue in &mut report.issues[before..] {
            issue.removable = false
        }
    } else if timed || shader && (value.get("start").is_some() || value.get("end").is_some()) {
        let read = |key: &str, fallback: &str| {
            value
                .get(key)
                .or_else(|| value.get(fallback))
                .and_then(|v| beat(v).or_else(|| shader.then(|| number(v)).flatten()))
        };
        match (read("startTime", "start"), read("endTime", "end")) {
            (Some(a), Some(b)) => {
                if b < a {
                    report.push(
                        Severity::Error,
                        "interval_time",
                        path,
                        Some(a),
                        "Interval ends before it starts",
                        false,
                    )
                }
                if let (Some(a), Some(b)) = (clock.seconds(a), clock.seconds(b))
                    && bounds.outside(a, b, true)
                {
                    report.push(
                        Severity::Error,
                        "audio_range",
                        path,
                        None,
                        "Interval is outside the available music",
                        false,
                    );
                }
            }
            _ => report.push(
                Severity::Error,
                "beat",
                path,
                None,
                "Interval has an invalid start or end beat",
                false,
            ),
        }
    }
}
