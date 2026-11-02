//! ID-buffer picking uses the original chart/note renderer, including Holds,
//! parent transforms, distance controls, clipping, textures and render order.
mod state;
use crate::Preview;
use anyhow::Result;
use macroquad::prelude::*;
use phistudio_core::{ChartDocument, NoteId};
use prpr::core::{Anim, JudgeLineKind};
pub(super) use state::PickingState;
use std::collections::{BTreeMap, VecDeque};
const VERTEX: &str = r#"#version 100
attribute vec3 position;attribute vec2 texcoord;attribute vec4 color0;
uniform mat4 Model;uniform mat4 Projection;varying mediump vec2 uv;varying mediump vec4 color;
void main(){gl_Position=Projection*Model*vec4(position,1.0);uv=texcoord;color=color0/255.0;}"#;
const FRAGMENT: &str = r#"#version 100
varying mediump vec2 uv;varying mediump vec4 color;uniform sampler2D Texture;
void main(){if(texture2D(Texture,uv).a*color.a<0.002)discard;gl_FragColor=vec4(color.rgb,1.0);}"#;
fn key(time: f64, kind: u64, x: f32, speed: f32, above: bool, fake: bool, y: f32) -> [u64; 7] {
    [
        (time * 1e7).round() as i64 as u64,
        kind,
        x.to_bits() as u64,
        speed.to_bits() as u64,
        u64::from(above),
        u64::from(fake),
        y.to_bits() as u64,
    ]
}
pub(crate) fn source_map(doc: &ChartDocument, chart: &prpr::core::Chart) -> Vec<Option<NoteId>> {
    let mut result = Vec::new();
    let mut bpm = chart.bpm_list.borrow_mut();
    for (line, parsed) in chart.lines.iter().enumerate() {
        let mut sources: BTreeMap<_, VecDeque<_>> = BTreeMap::new();
        for (index, n) in doc.iter_notes(line) {
            let kind = match n.kind {
                phistudio_core::NoteKind::Tap => 1,
                phistudio_core::NoteKind::Hold => 2,
                phistudio_core::NoteKind::Flick => 3,
                phistudio_core::NoteKind::Drag => 4,
            };
            let speed = n.speed as f32;
            let y = n.y_offset as f32 * 2. / 900. * speed;
            let k = key(
                bpm.time_beats(n.start_time.as_f64()),
                kind,
                n.x as f32 / 675.,
                speed,
                n.above == 1,
                n.is_fake != 0,
                y,
            );
            sources
                .entry(k)
                .or_default()
                .push_back(NoteId { line, index });
        }
        for n in &parsed.notes {
            let kind = match n.kind {
                prpr::core::NoteKind::Click => 1,
                prpr::core::NoteKind::Hold { .. } => 2,
                prpr::core::NoteKind::Flick => 3,
                prpr::core::NoteKind::Drag => 4,
            };
            let k = key(
                n.time,
                kind,
                n.object.translation.0.now(),
                n.speed as f32,
                n.above,
                n.fake,
                n.object.translation.1.now(),
            );
            result.push(sources.get_mut(&k).and_then(VecDeque::pop_front));
        }
    }
    result
}
impl Preview {
    pub fn pick_note(&mut self, point: [f32; 2], radius: [f32; 2]) -> Result<Option<NoteId>> {
        let direct = self.pick_unshaded(point, radius)?;
        if self.resource.no_effect
            || point
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        {
            return Ok(direct);
        }
        self.pick_with_effects(point, radius)
    }
    /// Test the actual postprocessed pixels. Binary masks avoid decoding or
    /// approximating arbitrary GLSL distortion/color functions on the CPU.
    fn pick_with_effects(&mut self, point: [f32; 2], radius: [f32; 2]) -> Result<Option<NoteId>> {
        let mut candidates = self.picking.visible_ids.clone();
        if candidates.is_empty() {
            return Ok(None);
        }
        let original = self.snapshot();
        let camera = self.resource.camera;
        let paints = PaintSnapshots::capture(&self.chart);
        let viewport = self.resource.last_vp;
        let flat: Vec<_> = self
            .chart
            .lines
            .iter()
            .enumerate()
            .flat_map(|(l, line)| (0..line.notes.len()).map(move |i| (l, i)))
            .collect();
        self.draw_frame(viewport, 0.);
        let baseline = self.snapshot();
        paints.restore(&self.chart);
        let result = (|| {
            if self.mask_score(&candidates, &flat, &baseline, point, radius, &paints) < 0.2 {
                return None;
            }
            while candidates.len() > 1 {
                let middle = candidates.len() / 2;
                let a = self.mask_score(
                    &candidates[..middle],
                    &flat,
                    &baseline,
                    point,
                    radius,
                    &paints,
                );
                let b = self.mask_score(
                    &candidates[middle..],
                    &flat,
                    &baseline,
                    point,
                    radius,
                    &paints,
                );
                if a.max(b) < 0.2 {
                    return None;
                }
                if a >= b {
                    candidates.truncate(middle);
                } else {
                    candidates.drain(..middle);
                }
            }
            self.picking.sources.get(candidates[0]).copied().flatten()
        })();
        paints.restore(&self.chart);
        self.resource.camera = camera;
        self.target.texture.update(&original);
        set_default_camera();
        Ok(result)
    }
    fn mask_score(
        &mut self,
        ids: &[usize],
        flat: &[(usize, usize)],
        baseline: &Image,
        point: [f32; 2],
        radius: [f32; 2],
        paints: &PaintSnapshots,
    ) -> f64 {
        let mut saved = Vec::with_capacity(ids.len());
        for id in ids {
            let (l, i) = flat[*id];
            saved.push((
                l,
                i,
                std::mem::replace(
                    &mut self.chart.lines[l].notes[i].object.alpha,
                    Anim::fixed(0.),
                ),
            ));
        }
        self.draw_frame(self.resource.last_vp, 0.);
        let masked = self.snapshot();
        for (l, i, alpha) in saved {
            self.chart.lines[l].notes[i].object.alpha = alpha;
        }
        paints.restore(&self.chart);
        let v = self
            .resource
            .camera
            .viewport
            .unwrap_or(self.target_viewport());
        let x = v.0 as f32 + point[0] * v.2 as f32;
        let y = v.1 as f32 + (1. - point[1]) * v.3 as f32;
        let rx = (radius[0] * v.2 as f32).clamp(2., self.target.texture.width() / 1280. * 40.);
        let ry = (radius[1] * v.3 as f32).clamp(2., self.target.texture.height() / 720. * 40.);
        let mut score = 0.;
        for py in ((y - ry).floor() as i32).max(v.1)..=((y + ry).ceil() as i32).min(v.1 + v.3 - 1) {
            for px in
                ((x - rx).floor() as i32).max(v.0)..=((x + rx).ceil() as i32).min(v.0 + v.2 - 1)
            {
                let distance = ((px as f32 - x) / rx).powi(2) + ((py as f32 - y) / ry).powi(2);
                if distance <= 1. {
                    let at = (py as usize * baseline.width as usize + px as usize) * 4;
                    let delta = (0..3)
                        .map(|i| baseline.bytes[at + i].abs_diff(masked.bytes[at + i]) as f64)
                        .sum::<f64>();
                    score += delta / (1. + f64::from(distance) * 10.);
                }
            }
        }
        score
    }
    fn pick_unshaded(&mut self, point: [f32; 2], radius: [f32; 2]) -> Result<Option<NoteId>> {
        if point
            .iter()
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        {
            return Ok(None);
        }
        let count = self
            .chart
            .lines
            .iter()
            .map(|l| l.notes.len())
            .sum::<usize>();
        anyhow::ensure!(count < 0xFF_FFFE, "Too many notes for preview picking");
        let size = self.render_size();
        let (target, material) = self.picking.target(size)?;
        let mut camera = self.resource.camera;
        camera.render_target = Some(target);
        let viewport = camera.viewport.unwrap_or(self.target_viewport());
        let mut clear = camera;
        clear.viewport = None;
        set_camera(&clear);
        clear_background(BLACK);
        set_camera(&camera);
        let old_debug = self.resource.config.chart_debug;
        self.resource.config.chart_debug = false;
        let mut colors = Vec::with_capacity(count);
        let mut lines = Vec::new();
        let mut id = 1_u32;
        for line in &mut self.chart.lines {
            lines.push((
                std::mem::replace(&mut line.kind, JudgeLineKind::Normal),
                std::mem::replace(&mut line.object.scale.0, Anim::fixed(0.)),
                std::mem::replace(&mut line.object.scale.1, Anim::fixed(0.)),
            ));
            for note in &mut line.notes {
                colors.push(note.color);
                note.color = Color::from_rgba(
                    (id & 255) as u8,
                    ((id >> 8) & 255) as u8,
                    ((id >> 16) & 255) as u8,
                    255,
                );
                id += 1;
            }
        }
        gl_use_material(material);
        let mut ui = prpr::ui::Ui::new(&mut self.text, Some(viewport));
        set_camera(&camera);
        let effects = self.resource.no_effect;
        self.resource.no_effect = true;
        self.chart.render(&mut ui, &mut self.resource);
        self.resource.no_effect = effects;
        unsafe { get_internal_gl() }.flush();
        gl_use_default_material();
        let image = target.texture.get_texture_data();
        self.picking.record_visible(&image.bytes, count);
        let mut colors = colors.into_iter();
        for (line, (kind, x, y)) in self.chart.lines.iter_mut().zip(lines) {
            line.kind = kind;
            line.object.scale.0 = x;
            line.object.scale.1 = y;
            for note in &mut line.notes {
                note.color = colors.next().unwrap();
            }
        }
        self.resource.config.chart_debug = old_debug;
        set_default_camera();
        let x = viewport.0 as f32 + point[0] * viewport.2 as f32;
        let y = viewport.1 as f32 + (1. - point[1]) * viewport.3 as f32;
        let rx =
            (radius[0] * viewport.2 as f32).clamp(1., self.target.texture.width() / 1280. * 40.);
        let ry =
            (radius[1] * viewport.3 as f32).clamp(1., self.target.texture.height() / 720. * 40.);
        let mut best = None;
        for py in ((y - ry).floor() as i32).max(viewport.1)
            ..=((y + ry).ceil() as i32).min(viewport.1 + viewport.3 - 1)
        {
            for px in ((x - rx).floor() as i32).max(viewport.0)
                ..=((x + rx).ceil() as i32).min(viewport.0 + viewport.2 - 1)
            {
                let distance = ((px as f32 - x) / rx).powi(2) + ((py as f32 - y) / ry).powi(2);
                if distance > 1. {
                    continue;
                }
                let at = (py as usize * image.width as usize + px as usize) * 4;
                let id = u32::from(image.bytes[at])
                    | (u32::from(image.bytes[at + 1]) << 8)
                    | (u32::from(image.bytes[at + 2]) << 16);
                if id > 0
                    && let Some(Some(note)) = self.picking.sources.get(id as usize - 1)
                    && best.is_none_or(|(d, _)| distance < d)
                {
                    best = Some((distance, *note));
                }
            }
        }
        Ok(best.map(|(_, id)| id))
    }
}

#[derive(Clone, Debug)]
pub struct LineState {
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    pub alpha: f32,
    pub remaining: usize,
}
impl Preview {
    pub fn line_states(&self) -> Vec<LineState> {
        self.chart
            .lines
            .iter()
            .take(self.anchors.source_lines)
            .map(|line| {
                let pos = line.fetch_pos(&self.resource, &self.chart.lines);
                LineState {
                    x: pos.x * 675.,
                    y: pos.y * self.resource.aspect_ratio * 450.,
                    rotation: -line.fetch_rot(&self.chart.lines),
                    alpha: line.object.alpha.now_opt().unwrap_or(1.) * 255.,
                    remaining: line
                        .notes
                        .iter()
                        .filter(|n| !matches!(n.judge, prpr::judge::JudgeStatus::Judged))
                        .count(),
                }
            })
            .collect()
    }
    pub fn visible_note_lines(&mut self) -> Result<std::collections::BTreeSet<usize>> {
        self.pick_unshaded([0.5, 0.5], [0., 0.])?;
        Ok(self.picking.visible_lines.clone())
    }
}

// Native Paint accumulates into a texture. Probe frames must start with the same
// contents and restore them, including the original native state flag.
pub(crate) struct PaintSnapshots(Vec<(usize, Texture2D, Image, bool)>);
impl PaintSnapshots {
    pub(crate) fn capture(chart: &prpr::core::Chart) -> Self {
        let mut snapshots = Vec::new();
        for (i, line) in chart.lines.iter().enumerate() {
            if let JudgeLineKind::Paint(_, state) = &line.kind {
                let state = state.borrow();
                if let Some(pass) = state.0 {
                    let tex = Texture2D::from_miniquad_texture(
                        pass.texture(unsafe { get_internal_gl() }.quad_context),
                    );
                    snapshots.push((i, tex, tex.get_texture_data(), state.1));
                }
            }
        }
        Self(snapshots)
    }
    pub(crate) fn restore(&self, chart: &prpr::core::Chart) {
        for (i, texture, image, flag) in &self.0 {
            texture.update(image);
            if let JudgeLineKind::Paint(_, state) = &chart.lines[*i].kind {
                state.borrow_mut().1 = *flag;
            }
        }
    }
}
