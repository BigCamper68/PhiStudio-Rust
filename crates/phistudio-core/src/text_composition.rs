//! Font outlines materialized as ordinary RPE judge lines or stationary fake notes.
use crate::{Beat, ChartDocument, ChartError, Editor, Note, NoteKind, tools::Selection};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Clone, Copy, Default, Debug, PartialEq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Lines,
    Tap,
    Drag,
    Flick,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Serialize, Deserialize)]
pub enum Font {
    #[default]
    Phira,
    Phigros,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spec {
    pub text: String,
    pub name: String,
    pub mode: Mode,
    pub font: Font,
    pub start: Beat,
    pub end: Beat,
    pub height: f64,
    pub center: [f64; 2],
    pub rotation: f64,
    pub spacing: f64,
    pub tracking: f64,
    pub line_spacing: f64,
    pub flatness: f64,
    pub thickness: f64,
    pub note_size: f64,
    pub aspect: f64,
}
impl Default for Spec {
    fn default() -> Self {
        Self {
            text: "PhiStudio".into(),
            name: "Text".into(),
            mode: Mode::Lines,
            font: Font::Phira,
            start: Beat::ZERO,
            end: Beat::new(4, 0, 1).unwrap(),
            height: 120.,
            center: [0.; 2],
            rotation: 0.,
            spacing: 8.,
            tracking: 0.,
            line_spacing: 1.3,
            flatness: 1.5,
            thickness: 2.,
            note_size: 0.05,
            aspect: 16. / 9.,
        }
    }
}
fn invalid(s: &str) -> ChartError {
    ChartError::Invalid(s.into())
}
fn mix(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.]
}
fn distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let squared = d[0] * d[0] + d[1] * d[1];
    let t = if squared < 1e-20 {
        0.
    } else {
        (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / squared).clamp(0., 1.)
    };
    (p[0] - a[0] - t * d[0]).hypot(p[1] - a[1] - t * d[1])
}

struct Outline {
    paths: Vec<Vec<[f64; 2]>>,
    current: Vec<[f64; 2]>,
    tolerance: f64,
    points: usize,
    overflow: bool,
}
impl Outline {
    fn push(&mut self, p: [f64; 2]) {
        if self.points >= 20000 {
            self.overflow = true;
            return;
        }
        self.current.push(p);
        self.points += 1;
    }
    fn finish(&mut self) {
        if self.current.len() > 1 {
            self.paths.push(std::mem::take(&mut self.current));
        } else {
            self.current.clear();
        }
    }
    fn quad(&mut self, a: [f64; 2], b: [f64; 2], c: [f64; 2], depth: u8) {
        if self.overflow {
            return;
        }
        if depth >= 14 || distance(b, a, c) <= self.tolerance {
            self.push(c);
        } else {
            let ab = mix(a, b);
            let bc = mix(b, c);
            let mid = mix(ab, bc);
            self.quad(a, ab, mid, depth + 1);
            self.quad(mid, bc, c, depth + 1);
        }
    }
    fn cubic(&mut self, a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2], depth: u8) {
        if self.overflow {
            return;
        }
        if depth >= 14 || distance(b, a, d).max(distance(c, a, d)) <= self.tolerance {
            self.push(d);
        } else {
            let ab = mix(a, b);
            let bc = mix(b, c);
            let cd = mix(c, d);
            let abc = mix(ab, bc);
            let bcd = mix(bc, cd);
            let mid = mix(abc, bcd);
            self.cubic(a, ab, abc, mid, depth + 1);
            self.cubic(mid, bcd, cd, d, depth + 1);
        }
    }
}
impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish();
        self.push([f64::from(x), f64::from(y)]);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.push([f64::from(x), f64::from(y)]);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        if let Some(&p) = self.current.last() {
            self.quad(
                p,
                [f64::from(x1), f64::from(y1)],
                [f64::from(x), f64::from(y)],
                0,
            );
        }
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        if let Some(&p) = self.current.last() {
            self.cubic(
                p,
                [f64::from(x1), f64::from(y1)],
                [f64::from(x2), f64::from(y2)],
                [f64::from(x), f64::from(y)],
                0,
            );
        }
    }
    fn close(&mut self) {
        if let Some(&p) = self.current.first() {
            self.push(p);
        }
        self.finish();
    }
}
pub struct Plan {
    pub spec: Spec,
    pub contours: Vec<Vec<[f64; 2]>>,
    pub marks: Vec<[f64; 2]>,
    pub notes: usize,
    pub lines: usize,
    pub(crate) values: Vec<Value>,
}
impl Plan {
    pub fn apply(self, editor: &mut Editor) -> Result<Selection, ChartError> {
        editor.execute_text_composition(self.values, self.spec.mode == Mode::Lines)
    }
}
pub fn prepare(spec: Spec, font: &[u8], tempo: &crate::TempoMap) -> Result<Plan, ChartError> {
    if spec.text.chars().count() > 512
        || spec.text.trim().is_empty()
        || spec.name.trim().is_empty()
        || spec.name.len() > 512
        || spec.start < Beat::ZERO
        || spec.end <= spec.start
        || [
            spec.height,
            spec.center[0],
            spec.center[1],
            spec.rotation,
            spec.spacing,
            spec.tracking,
            spec.line_spacing,
            spec.flatness,
            spec.thickness,
            spec.note_size,
            spec.aspect,
        ]
        .iter()
        .any(|v| !v.is_finite() || v.abs() > 1e6)
        || spec.height < 1.
        || spec.spacing < 1.
        || spec.flatness < 0.1
        || spec.thickness <= 0.
        || spec.note_size <= 0.
        || spec.aspect < 0.1
        || spec.aspect > 10.
        || spec.line_spacing <= 0.
    {
        return Err(invalid(
            "Use valid text, an ordered beat range and positive text dimensions",
        ));
    }
    let face = ttf_parser::Face::parse(font, 0)
        .map_err(|_| invalid("Cannot read the text composition font"))?;
    let scale = spec.height / f64::from(face.units_per_em());
    let mut paths = vec![];
    let mut points = 0;
    for (row, line) in spec.text.lines().enumerate() {
        let (mut x, y) = (0., -(row as f64) * spec.height * spec.line_spacing);
        for ch in line.chars() {
            if ch == '\t' {
                x += spec.height * 2.;
                continue;
            }
            let glyph = face
                .glyph_index(ch)
                .ok_or_else(|| invalid(&format!("Font does not contain character {ch}")))?;
            let mut builder = Outline {
                paths: vec![],
                current: vec![],
                tolerance: spec.flatness / scale,
                points: 0,
                overflow: false,
            };
            face.outline_glyph(glyph, &mut builder);
            builder.finish();
            if builder.overflow {
                return Err(invalid(
                    "Text outline exceeds 20,000 points; increase flatness",
                ));
            }
            for contour in builder.paths {
                points += contour.len();
                if points > 20000 {
                    return Err(invalid(
                        "Text outline exceeds 20,000 points; increase flatness",
                    ));
                }
                paths.push(
                    contour
                        .into_iter()
                        .map(|p| [x + p[0] * scale, y + p[1] * scale])
                        .collect::<Vec<_>>(),
                );
            }
            x += f64::from(face.glyph_hor_advance(glyph).unwrap_or(face.units_per_em())) * scale
                + spec.tracking;
        }
    }
    if paths.is_empty() {
        return Err(invalid("Text has no drawable outlines"));
    }
    let lo = [
        paths
            .iter()
            .flatten()
            .map(|p| p[0])
            .fold(f64::INFINITY, f64::min),
        paths
            .iter()
            .flatten()
            .map(|p| p[1])
            .fold(f64::INFINITY, f64::min),
    ];
    let hi = [
        paths
            .iter()
            .flatten()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max),
        paths
            .iter()
            .flatten()
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max),
    ];
    for p in paths.iter_mut().flatten() {
        p[0] -= (lo[0] + hi[0]) / 2.;
        p[1] = (p[1] - (lo[1] + hi[1]) / 2.) * spec.aspect * 2. / 3.;
    }
    let local_paths = paths.clone();
    let (sn, cs) = (-spec.rotation.to_radians()).sin_cos();
    let world = |p: [f64; 2]| {
        let x = p[0];
        let y = p[1] / spec.aspect * 1.5;
        [
            spec.center[0] + x * cs - y * sn,
            spec.center[1] + (x * sn + y * cs) * spec.aspect * 2. / 3.,
        ]
    };
    for p in paths.iter_mut().flatten() {
        *p = world(*p);
    }
    let event = |a: Beat, b: Beat, value: f64| json!({"startTime":a,"endTime":b,"start":value,"end":value,"easingType":1});
    let forever = spec.end.checked_add(Beat::ONE)?;
    let visibility = |visible: f64| {
        let mut events = vec![event(Beat::ZERO, spec.start, 0.)];
        if spec.start == Beat::ZERO {
            events.clear();
        }
        events.push(event(spec.start, spec.end, visible));
        events.push(event(spec.end, forever, 0.));
        events
    };
    let mut values = vec![];
    let mut notes = 0;
    let mut marks = vec![];
    match spec.mode {
        Mode::Lines => {
            for contour in &local_paths {
                for pair in contour.windows(2) {
                    let a = pair[0];
                    let b = pair[1];
                    let dx = (b[0] - a[0]) / 675.;
                    let dy = (b[1] - a[1]) / 450. / spec.aspect;
                    let length = dx.hypot(dy);
                    if length < 1e-8 {
                        continue;
                    }
                    if values.len() >= 1023 {
                        return Err(invalid(
                            "Text needs more than 1024 lines; increase flatness or shorten the text",
                        ));
                    }
                    let mut line =
                        ChartDocument::new_line(&format!("{} {}", spec.name, values.len() + 1));
                    line["eventLayers"] = json!([{"alphaEvents":visibility(255.),"moveXEvents":[event(Beat::ZERO,forever,(a[0]+b[0])/2.)],"moveYEvents":[event(Beat::ZERO,forever,(a[1]+b[1])/2.)],"rotateEvents":[event(Beat::ZERO,forever,-dy.atan2(dx).to_degrees())],"speedEvents":[event(Beat::ZERO,forever,0.)]}]);
                    line["extended"] = json!({"scaleXEvents":[event(Beat::ZERO,forever,length/6.)],"scaleYEvents":[event(Beat::ZERO,forever,spec.thickness/675./0.0076)],"colorEvents":[{"startTime":Beat::ZERO,"endTime":forever,"start":[255,255,255],"end":[255,255,255],"easingType":1}]});
                    line["rotateWithFather"] = json!(true);
                    values.push(line);
                }
            }
        }
        _ => {
            let mut line = ChartDocument::new_line(&spec.name);
            let mut output = vec![];
            let visible = (tempo.seconds_at(spec.end.as_f64())
                - tempo.seconds_at(spec.start.as_f64()))
            .max(0.);
            for contour in &local_paths {
                let mut remaining = 0.;
                for pair in contour.windows(2) {
                    let a = pair[0];
                    let b = pair[1];
                    let length = (b[0] - a[0]).hypot((b[1] - a[1]) / spec.aspect * 1.5);
                    if length <= 1e-10 {
                        continue;
                    }
                    let mut d = remaining;
                    while d < length {
                        let t = d / length;
                        if output.len() >= 20000 {
                            return Err(invalid(
                                "Text needs more than 20,000 notes; increase spacing or shorten the text",
                            ));
                        }
                        let note = Note {
                            kind: match spec.mode {
                                Mode::Tap => NoteKind::Tap,
                                Mode::Flick => NoteKind::Flick,
                                _ => NoteKind::Drag,
                            },
                            start_time: spec.end,
                            end_time: spec.end,
                            x: a[0] + t * (b[0] - a[0]),
                            y_offset: a[1] + t * (b[1] - a[1]),
                            is_fake: 1,
                            speed: 1.,
                            size: spec.note_size,
                            visible_time: visible,
                            ..Default::default()
                        };
                        note.validate()?;
                        marks.push(world([note.x, note.y_offset]));
                        output.push(serde_json::to_value(note)?);
                        d += spec.spacing;
                    }
                    remaining = d - length;
                }
            }
            notes = output.len();
            line["notes"] = json!(output);
            line["numOfNotes"] = json!(notes);
            line["eventLayers"] = json!([{"alphaEvents":[event(Beat::ZERO,forever,0.)],"moveXEvents":[event(Beat::ZERO,forever,spec.center[0])],"moveYEvents":[event(Beat::ZERO,forever,spec.center[1])],"rotateEvents":[event(Beat::ZERO,forever,spec.rotation)],"speedEvents":[event(Beat::ZERO,forever,0.)]}]);
            values.push(line);
        }
    }
    if spec.mode == Mode::Lines {
        let mut controller = ChartDocument::new_line(&spec.name);
        controller["eventLayers"] = json!([{"alphaEvents":[event(Beat::ZERO,forever,0.)],"moveXEvents":[event(Beat::ZERO,forever,spec.center[0])],"moveYEvents":[event(Beat::ZERO,forever,spec.center[1])],"rotateEvents":[event(Beat::ZERO,forever,spec.rotation)],"speedEvents":[event(Beat::ZERO,forever,0.)]}]);
        values.insert(0, controller);
    }
    if let Some(line) = values.first_mut() {
        line["phiStudioTextComposition"] = serde_json::to_value(&spec)?;
    }
    Ok(Plan {
        lines: values.len(),
        spec,
        contours: paths,
        marks,
        notes,
        values,
    })
}
