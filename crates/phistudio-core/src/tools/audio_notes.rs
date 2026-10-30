//! Deterministic marker-to-note planning; no DSP, asset I/O or document mutation.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioPosition {
    #[default]
    Fixed,
    Alternating,
    Ascending,
    PingPong,
    Random,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioSide {
    #[default]
    Above,
    Below,
    Alternating,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioCollision {
    #[default]
    Position,
    Line,
    Allow,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioNoteOptions {
    pub minimum_strength: f32,
    pub every: usize,
    pub minimum_gap_beats: f64,
    pub max_notes: usize,
    pub quantize: bool,
    pub division: i32,
    pub max_snap_ms: f64,
    pub shift_ms: f64,
    pub kind: NoteKind,
    pub per_band: bool,
    pub band_kinds: [NoteKind; 3],
    pub hold_beats: f64,
    pub trim_holds: bool,
    pub side: AudioSide,
    pub position: AudioPosition,
    pub x: f64,
    pub min_x: f64,
    pub max_x: f64,
    pub lanes: usize,
    pub seed: u64,
    pub speed: f64,
    pub size: f64,
    pub alpha: u16,
    pub y_offset: f64,
    pub fake: bool,
    pub collision: AudioCollision,
    pub collision_ms: f64,
    pub collision_x: f64,
}
impl Default for AudioNoteOptions {
    fn default() -> Self {
        Self {
            minimum_strength: 0.,
            every: 1,
            minimum_gap_beats: 0.,
            max_notes: MAX_OBJECTS,
            quantize: true,
            division: 4,
            max_snap_ms: 0.,
            shift_ms: 0.,
            kind: NoteKind::Tap,
            per_band: false,
            band_kinds: [NoteKind::Tap, NoteKind::Drag, NoteKind::Flick],
            hold_beats: 1.,
            trim_holds: true,
            side: AudioSide::Above,
            position: AudioPosition::Fixed,
            x: 0.,
            min_x: -450.,
            max_x: 450.,
            lanes: 5,
            seed: 1,
            speed: 1.,
            size: 1.,
            alpha: 255,
            y_offset: 0.,
            fake: false,
            collision: AudioCollision::Position,
            collision_ms: 20.,
            collision_x: 40.,
        }
    }
}
impl AudioNoteOptions {
    pub fn normalized(mut self) -> Self {
        fn bound(v: f64, lo: f64, hi: f64, default: f64) -> f64 {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                default
            }
        }
        self.minimum_strength = bound(self.minimum_strength as f64, 0., 1., 0.) as f32;
        self.every = self.every.clamp(1, 1000);
        self.max_notes = self.max_notes.clamp(1, MAX_OBJECTS);
        self.minimum_gap_beats = bound(self.minimum_gap_beats, 0., 256., 0.);
        self.division = self.division.clamp(1, 1920);
        self.max_snap_ms = bound(self.max_snap_ms, 0., 2000., 0.);
        self.shift_ms = bound(self.shift_ms, -2000., 2000., 0.);
        self.hold_beats = bound(self.hold_beats, 0.000001, 256., 1.);
        self.x = bound(self.x, -100000., 100000., 0.);
        self.min_x = bound(self.min_x, -100000., 100000., -450.);
        self.max_x = bound(self.max_x, -100000., 100000., 450.);
        self.lanes = self.lanes.clamp(2, 128);
        self.speed = bound(self.speed, 0.000001, 10000., 1.);
        self.size = bound(self.size, 0.000001, 10000., 1.);
        self.alpha = self.alpha.min(255);
        self.y_offset = bound(self.y_offset, -100000., 100000., 0.);
        self.collision_ms = bound(self.collision_ms, 0., 2000., 20.);
        self.collision_x = bound(self.collision_x, 0., 100000., 40.);
        self
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioMarker {
    pub seconds: f64,
    pub strength: f32,
    pub band: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AudioNoteSpec {
    pub options: AudioNoteOptions,
    pub range: [Beat; 2],
    pub lines: Vec<usize>,
    pub audio_offset: f64,
}
#[derive(Clone, Debug)]
pub struct AudioGeneratedNote {
    pub line: usize,
    pub note: Note,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct AudioNoteStats {
    pub input: usize,
    pub outside: usize,
    pub weak: usize,
    pub off_grid: usize,
    pub merged: usize,
    pub thinned: usize,
    pub limited: usize,
    pub collisions: usize,
    pub trimmed: usize,
}
#[derive(Clone, Debug)]
pub struct AudioNotePreview {
    pub notes: Vec<AudioGeneratedNote>,
    pub stats: AudioNoteStats,
}
#[derive(Clone, Copy)]
struct Mapped {
    beat: Beat,
    strength: f32,
    band: usize,
    seconds: f64,
}

/// The callback allows the UI worker to cancel long plans without affecting DSP.
pub fn preview_audio_notes(
    doc: &ChartDocument,
    markers: &[AudioMarker],
    spec: &AudioNoteSpec,
    mut canceled: impl FnMut() -> bool,
) -> Result<AudioNotePreview, ChartError> {
    let o = &spec.options;
    if *o != o.clone().normalized()
        || o.min_x > o.max_x
        || spec.range[0] < Beat::ZERO
        || spec.range[1] <= spec.range[0]
        || !spec.audio_offset.is_finite()
        || spec.lines.is_empty()
        || spec.lines.len() > MAX_OBJECTS
    {
        return Err(invalid(
            "Use valid generator settings, an ordered beat range and target lines",
        ));
    }
    let mut lines = BTreeSet::new();
    for &line in &spec.lines {
        if line >= doc.line_count() || !lines.insert(line) {
            return Err(invalid("Choose distinct existing target lines"));
        }
    }
    if markers.len() > 200000 {
        return Err(invalid("Too many audio markers"));
    }
    let mut check = || {
        if canceled() {
            Err(invalid("Note generation canceled"))
        } else {
            Ok(())
        }
    };
    check()?;
    let tempo = doc.tempo();
    let mut stats = AudioNoteStats {
        input: markers.len(),
        ..Default::default()
    };
    let mut mapped = BTreeMap::<Beat, Mapped>::new();
    for (i, m) in markers.iter().enumerate() {
        if i % 256 == 0 {
            check()?;
        }
        if !m.seconds.is_finite()
            || m.seconds < 0.
            || !m.strength.is_finite()
            || !(0. ..=1.).contains(&m.strength)
            || m.band > 3
        {
            return Err(invalid(
                "Audio markers must contain finite times and strengths",
            ));
        }
        if m.strength < o.minimum_strength {
            stats.weak += 1;
            continue;
        }
        let seconds = m.seconds - spec.audio_offset + o.shift_ms / 1000.;
        let raw = tempo.beat_at(seconds);
        if raw < spec.range[0].as_f64() - 1e-9 || raw > spec.range[1].as_f64() + 1e-9 {
            stats.outside += 1;
            continue;
        }
        let beat = Beat::from_grid(raw, if o.quantize { o.division } else { 1000000 })?;
        if beat < spec.range[0] || beat > spec.range[1] {
            stats.outside += 1;
            continue;
        }
        let snapped = tempo.seconds_at(beat.as_f64());
        if !snapped.is_finite() {
            return Err(invalid(
                "Use valid generator settings, an ordered beat range and target lines",
            ));
        }
        if o.quantize
            && o.max_snap_ms > 0.
            && (snapped - seconds).abs() * 1000. > o.max_snap_ms + 1e-7
        {
            stats.off_grid += 1;
            continue;
        }
        let value = Mapped {
            beat,
            strength: m.strength,
            band: m.band,
            seconds: m.seconds,
        };
        if let Some(old) = mapped.get_mut(&beat) {
            stats.merged += 1;
            if m.strength > old.strength || (m.strength == old.strength && m.seconds < old.seconds)
            {
                *old = value;
            }
        } else {
            mapped.insert(beat, value);
        }
    }
    let mut candidates = mapped
        .into_values()
        .enumerate()
        .filter_map(|(i, m)| {
            if i % o.every == 0 {
                Some(m)
            } else {
                stats.thinned += 1;
                None
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| {
        b.strength
            .total_cmp(&a.strength)
            .then(a.beat.cmp(&b.beat))
            .then(a.seconds.total_cmp(&b.seconds))
    });
    let mut accepted = BTreeSet::<Beat>::new();
    let mut selected = Vec::new();
    for (i, m) in candidates.into_iter().enumerate() {
        if i % 256 == 0 {
            check()?;
        }
        let at = m.beat.as_f64();
        let conflict = accepted
            .range(..=m.beat)
            .next_back()
            .into_iter()
            .chain(accepted.range(m.beat..).next())
            .any(|b| (b.as_f64() - at).abs() + 1e-9 < o.minimum_gap_beats);
        if conflict {
            stats.thinned += 1;
            continue;
        }
        if selected.len() == o.max_notes {
            stats.limited += 1;
            continue;
        }
        accepted.insert(m.beat);
        selected.push(m);
    }
    selected.sort_by_key(|m| m.beat);
    let hold = Beat::from_grid(o.hold_beats, 1000000)?;
    let mut random = Random(o.seed);
    let mut notes = Vec::with_capacity(selected.len());
    for (i, m) in selected.iter().enumerate() {
        if i % 256 == 0 {
            check()?;
        }
        let lane = match o.position {
            AudioPosition::Fixed => 0,
            AudioPosition::Alternating => {
                if i % 2 == 0 {
                    0
                } else {
                    o.lanes - 1
                }
            }
            AudioPosition::Ascending => i % o.lanes,
            AudioPosition::PingPong => {
                let p = i % (2 * (o.lanes - 1));
                p.min(2 * (o.lanes - 1) - p)
            }
            AudioPosition::Random => (random.next_f64() * o.lanes as f64) as usize,
        };
        let x = if o.position == AudioPosition::Fixed {
            o.x
        } else {
            o.min_x + (o.max_x - o.min_x) * lane as f64 / (o.lanes - 1) as f64
        };
        let kind = if o.per_band && (1..=3).contains(&m.band) {
            o.band_kinds[m.band - 1]
        } else {
            o.kind
        };
        let above = match o.side {
            AudioSide::Above => 1,
            AudioSide::Below => 0,
            AudioSide::Alternating => u8::from(i % 2 == 0),
        };
        let end = if kind == NoteKind::Hold {
            m.beat.checked_add(hold)?.min(spec.range[1])
        } else {
            m.beat
        };
        if kind == NoteKind::Hold && end <= m.beat {
            stats.outside += 1;
            continue;
        }
        notes.push(AudioGeneratedNote {
            line: spec.lines[i % spec.lines.len()],
            note: Note {
                kind,
                start_time: m.beat,
                end_time: end,
                x,
                above,
                is_fake: u8::from(o.fake),
                alpha: o.alpha,
                size: o.size,
                speed: o.speed,
                y_offset: o.y_offset,
                ..Default::default()
            },
        });
    }
    if o.trim_holds {
        let mut next = BTreeMap::new();
        for item in notes.iter_mut().rev() {
            let key = (item.line, item.note.above);
            if item.note.kind == NoteKind::Hold
                && let Some(&beat) = next.get(&key)
                && item.note.end_time > beat
            {
                item.note.end_time = beat;
                stats.trimmed += 1;
            }
            next.insert(key, item.note.start_time);
        }
    }
    if o.collision != AudioCollision::Allow {
        // Nearby-X queries touch at most three spatial buckets. A long Hold
        // far away must not force a full time-index scan for every marker.
        let bucket = |x: f64| {
            if o.collision == AudioCollision::Position {
                (x / o.collision_x.max(0.000001)).floor() as i64
            } else {
                0
            }
        };
        let mut existing = BTreeMap::<(usize, u8, i64), Vec<Occupied>>::new();
        for &line in &spec.lines {
            check()?;
            for (i, (_, n)) in doc.notes(line).into_iter().enumerate() {
                if i % 256 == 0 {
                    check()?;
                }
                existing
                    .entry((line, u8::from(n.above == 1), bucket(n.x)))
                    .or_default()
                    .push(Occupied::new(&n, tempo));
            }
        }
        let existing = existing
            .into_iter()
            .map(|(key, rows)| (key, IntervalIndex::new(rows, true)))
            .collect::<BTreeMap<_, _>>();
        let mut generated = BTreeMap::<(usize, u8, i64), Vec<Occupied>>::new();
        for item in &notes {
            generated
                .entry((item.line, item.note.above, bucket(item.note.x)))
                .or_default()
                .push(Occupied::new(&item.note, tempo));
        }
        let mut generated = generated
            .into_iter()
            .map(|(key, rows)| (key, IntervalIndex::new(rows, false)))
            .collect::<BTreeMap<_, _>>();
        let mut cursor = BTreeMap::<(usize, u8, i64), usize>::new();
        let mut kept = Vec::with_capacity(notes.len());
        for (i, item) in notes.into_iter().enumerate() {
            if i % 256 == 0 {
                check()?;
            }
            let key = (item.line, item.note.above, bucket(item.note.x));
            let row = Occupied::new(&item.note, tempo);
            let index = *cursor.entry(key).or_default();
            *cursor.get_mut(&key).unwrap() += 1;
            let x = if o.collision == AudioCollision::Line {
                f64::INFINITY
            } else {
                o.collision_x
            };
            let neighbors = if o.collision == AudioCollision::Position {
                vec![key.2.checked_sub(1), Some(key.2), key.2.checked_add(1)]
            } else {
                vec![Some(0)]
            };
            let conflict = neighbors.into_iter().flatten().any(|b| {
                let probe = (key.0, key.1, b);
                existing
                    .get(&probe)
                    .is_some_and(|tree| tree.overlaps(row, o.collision_ms / 1000., x))
                    || generated
                        .get(&probe)
                        .is_some_and(|tree| tree.overlaps(row, o.collision_ms / 1000., x))
            });
            if conflict {
                stats.collisions += 1;
            } else {
                generated.get_mut(&key).unwrap().activate(index);
                kept.push(item);
            }
        }
        notes = kept;
    }
    check()?;
    Ok(AudioNotePreview { notes, stats })
}

pub fn insert_audio_notes(
    doc: &ChartDocument,
    notes: &[AudioGeneratedNote],
) -> Result<ToolEdit, ChartError> {
    if notes.len() > MAX_OBJECTS {
        return Err(invalid("The operation exceeds 20,000 objects"));
    }
    let items = notes
        .iter()
        .map(|item| {
            Ok(Object {
                line: item.line,
                event: None,
                value: item.note.merge_into(None)?,
            })
        })
        .collect::<Result<Vec<_>, ChartError>>()?;
    plan(
        doc,
        &Selection::default(),
        items,
        "Generate notes from audio markers",
    )
}

#[derive(Clone, Copy)]
struct Occupied {
    start: f64,
    end: f64,
    x: f64,
}
impl Occupied {
    fn new(note: &Note, tempo: &crate::TempoMap) -> Self {
        let start = tempo.seconds_at(note.start_time.as_f64());
        Self {
            start,
            end: if note.kind == NoteKind::Hold {
                tempo.seconds_at(note.end_time.as_f64())
            } else {
                start
            },
            x: note.x,
        }
    }
    fn hits(self, other: Self, t: f64, x: f64) -> bool {
        (self.x - other.x).abs() <= x
            && ((self.start - other.start).abs() <= t + 1e-9
                || (self.end > self.start
                    && other.start >= self.start
                    && other.start < self.end - 1e-9)
                || (other.end > other.start
                    && self.start >= other.start
                    && self.start < other.end - 1e-9))
    }
}
#[derive(Clone, Copy)]
struct Bounds {
    end: f64,
    min_x: f64,
    max_x: f64,
}
impl Bounds {
    const EMPTY: Self = Self {
        end: f64::NEG_INFINITY,
        min_x: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
    };
    fn merge(a: Self, b: Self) -> Self {
        Self {
            end: a.end.max(b.end),
            min_x: a.min_x.min(b.min_x),
            max_x: a.max_x.max(b.max_x),
        }
    }
}
/// Sorted starts plus augmented interval/X bounds avoid rescanning a long Hold
/// and all preceding notes for every marker. Generated leaves activate in O(log n).
struct IntervalIndex {
    rows: Vec<Occupied>,
    tree: Vec<Bounds>,
}
impl IntervalIndex {
    fn new(mut rows: Vec<Occupied>, active: bool) -> Self {
        rows.sort_by(|a, b| a.start.total_cmp(&b.start));
        let mut this = Self {
            tree: vec![Bounds::EMPTY; rows.len() * 4 + 4],
            rows,
        };
        if active && !this.rows.is_empty() {
            this.build(1, 0, this.rows.len());
        }
        this
    }
    fn build(&mut self, node: usize, lo: usize, hi: usize) -> Bounds {
        let b = if hi - lo == 1 {
            let r = self.rows[lo];
            Bounds {
                end: r.end,
                min_x: r.x,
                max_x: r.x,
            }
        } else {
            let mid = (lo + hi) / 2;
            let a = self.build(node * 2, lo, mid);
            let b = self.build(node * 2 + 1, mid, hi);
            Bounds::merge(a, b)
        };
        self.tree[node] = b;
        b
    }
    fn activate(&mut self, index: usize) {
        self.set(1, 0, self.rows.len(), index);
    }
    fn set(&mut self, node: usize, lo: usize, hi: usize, index: usize) {
        if hi - lo == 1 {
            let r = self.rows[index];
            self.tree[node] = Bounds {
                end: r.end,
                min_x: r.x,
                max_x: r.x,
            };
            return;
        }
        let mid = (lo + hi) / 2;
        if index < mid {
            self.set(node * 2, lo, mid, index)
        } else {
            self.set(node * 2 + 1, mid, hi, index)
        }
        self.tree[node] = Bounds::merge(self.tree[node * 2], self.tree[node * 2 + 1]);
    }
    fn overlaps(&self, q: Occupied, t: f64, x: f64) -> bool {
        !self.rows.is_empty() && self.find(1, 0, self.rows.len(), q, t, x)
    }
    fn find(&self, node: usize, lo: usize, hi: usize, q: Occupied, t: f64, x: f64) -> bool {
        let b = self.tree[node];
        if b.end < q.start - t - 1e-9
            || self.rows[lo].start > q.end + t + 1e-9
            || b.min_x > q.x + x
            || b.max_x < q.x - x
        {
            return false;
        }
        if hi - lo == 1 {
            return self.rows[lo].hits(q, t, x);
        }
        let mid = (lo + hi) / 2;
        self.find(node * 2, lo, mid, q, t, x) || self.find(node * 2 + 1, mid, hi, q, t, x)
    }
}
