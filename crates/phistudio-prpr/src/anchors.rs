//! Visual pivots use note-free native lines; authored note transforms stay intact.
use phistudio_core::ChartDocument;
use prpr::{
    core::{
        Anim, AnimFloat, AnimVector, Chart, CtrlObject, JudgeLine, JudgeLineCache, JudgeLineKind,
        Object, Resource,
    },
    ui::Ui,
};
use std::cell::RefCell;

struct Visual {
    source: usize,
    native: usize,
    anchor: [f32; 2],
}
pub(crate) struct Anchors {
    pub source_lines: usize,
    visuals: Vec<Visual>,
}
impl Anchors {
    pub fn install(document: &ChartDocument, chart: &mut Chart) -> Self {
        let count = chart.lines.len();
        let mut visuals = vec![];
        let mut indices = vec![None; count];
        for (index, raw) in document.as_json()["judgeLineList"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let anchor = [
                raw["anchor"][0].as_f64().unwrap_or(0.5) as f32,
                raw["anchor"][1].as_f64().unwrap_or(0.5) as f32,
            ];
            if anchor == [0.5; 2] || !anchor.iter().all(|v| v.is_finite()) {
                continue;
            }
            let source = &mut chart.lines[index];
            // HUD bindings and paint canvases use their original native geometry.
            if source.attach_ui.is_some() || matches!(source.kind, JudgeLineKind::Paint(..)) {
                continue;
            }
            let kind = std::mem::replace(
                &mut source.kind,
                JudgeLineKind::Text(Anim::fixed(String::new())),
            );
            let mut notes = vec![];
            let native = JudgeLine {
                object: Object {
                    alpha: source.object.alpha.clone(),
                    scale: AnimVector(source.object.scale.0.clone(), source.object.scale.1.clone()),
                    rotation: AnimFloat::fixed(0.),
                    translation: AnimVector(AnimFloat::fixed(0.), AnimFloat::fixed(0.)),
                },
                ctrl_obj: RefCell::new(CtrlObject::default()),
                kind,
                height: AnimFloat::fixed(0.),
                incline: AnimFloat::fixed(0.),
                cache: JudgeLineCache::new(&mut notes),
                notes,
                color: source.color.clone(),
                parent: Some(index),
                rot_with_parent: true,
                z_index: source.z_index,
                show_below: source.show_below,
                attach_ui: None,
            };
            let id = chart.lines.len();
            chart.lines.push(native);
            indices[index] = Some(id);
            visuals.push(Visual {
                source: index,
                native: id,
                anchor,
            });
        }
        let original = std::mem::take(&mut chart.order);
        for id in original {
            if let Some(visual) = indices[id] {
                chart.order.push(visual);
            }
            chart.order.push(id);
        }
        Self {
            source_lines: count,
            visuals,
        }
    }

    pub fn visual_line(&self, source: usize) -> usize {
        self.visuals
            .iter()
            .find(|v| v.source == source)
            .map_or(source, |v| v.native)
    }
    pub fn has_visuals(&self) -> bool {
        !self.visuals.is_empty()
    }

    pub fn draw_source_ids(&self, chart: &Chart, resource: &mut Resource, ui: &mut Ui) {
        // Helper visuals never become editor-visible line IDs.
        use prpr::core::{Matrix, Vector};
        resource.apply_model_of(
            &Matrix::identity().append_nonuniform_scaling(&Vector::new(
                if resource.config.flip_x() { -1. } else { 1. },
                -1.,
            )),
            |res| {
                for &id in chart.order.iter().filter(|id| **id < self.source_lines) {
                    res.with_model(chart.lines[id].now_transform(res, &chart.lines), |res| {
                        res.apply_model(|_| {
                            ui.text(id.to_string())
                                .pos(0., -0.01)
                                .anchor(0.5, 1.)
                                .size(0.8)
                                .draw();
                        });
                    });
                }
            },
        );
    }

    pub fn update(&self, chart: &mut Chart, resource: &Resource, ui: &mut Ui) {
        for visual in &self.visuals {
            let line = &mut chart.lines[visual.native];
            let (width, height) = match &line.kind {
                JudgeLineKind::Normal => (
                    resource.info.line_length * 2.,
                    if (line.object.scale.1.now() - 1.).abs() > 1e-4 {
                        0.0076
                    } else {
                        0.01
                    },
                ),
                JudgeLineKind::Texture(texture, _) => (texture.width(), texture.height()),
                JudgeLineKind::TextureGif(anim, frames, _) => {
                    let texture = frames.get_prog_frame(anim.now_opt().unwrap_or(0.));
                    (texture.width(), texture.height())
                }
                JudgeLineKind::Text(anim) => {
                    let text = anim.now();
                    let rect = ui
                        .text(&text)
                        .pos(0., 0.)
                        .anchor(0.5, 0.5)
                        .size(1.)
                        .multiline()
                        .measure();
                    (rect.w, rect.h)
                }
                JudgeLineKind::Paint(..) => continue,
            };
            let scale = line.object.scale.now_with_def(1., 1.);
            let x = (0.5 - visual.anchor[0]) * width * scale.x;
            let y = (0.5 - visual.anchor[1]) * height * scale.y;
            line.object.translation.0.map_value(|_| x);
            line.object
                .translation
                .1
                .map_value(|_| y * resource.aspect_ratio);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn parse(document: &ChartDocument) -> Chart {
        let mut fs = crate::assets::ChartAssets::new(None).unwrap();
        pollster::block_on(prpr::parse::parse_rpe(
            &crate::compatibility::render_source(document).unwrap(),
            &mut fs,
            prpr::core::ChartExtra::default(),
            false,
        ))
        .unwrap()
    }
    #[test]
    fn center_defaults_leave_original_native_lines_and_render_order_untouched() {
        let document = ChartDocument::empty("Centered", 120.).unwrap();
        let mut chart = parse(&document);
        let order = chart.order.clone();
        let anchors = Anchors::install(&document, &mut chart);
        assert!(anchors.visuals.is_empty());
        assert_eq!(anchors.source_lines, 1);
        assert_eq!(chart.lines.len(), 1);
        assert_eq!(chart.order, order);
        assert!(matches!(chart.lines[0].kind, JudgeLineKind::Normal));
    }
    #[test]
    fn anchored_visuals_preserve_note_ids_parent_rotation_and_original_transforms() {
        let mut raw = ChartDocument::empty("Parent anchors", 120.)
            .unwrap()
            .as_json()
            .clone();
        let line = raw["judgeLineList"][0].clone();
        raw["judgeLineList"] = json!([line.clone(), line]);
        raw["judgeLineList"][0]["father"] = json!(1);
        raw["judgeLineList"][0]["rotateWithFather"] = json!(true);
        raw["judgeLineList"][0]["anchor"] = json!([0., 1.]);
        raw["judgeLineList"][0]["notes"] =
            json!([serde_json::to_value(phistudio_core::Note::default()).unwrap()]);
        for (index, angle) in [(0, 30.), (1, 45.)] {
            raw["judgeLineList"][index]["eventLayers"][0]["rotateEvents"][0]["start"] =
                json!(angle);
            raw["judgeLineList"][index]["eventLayers"][0]["rotateEvents"][0]["end"] = json!(angle);
        }
        let document = ChartDocument::from_value(raw).unwrap();
        let mut chart = parse(&document);
        for line in &mut chart.lines {
            line.object.set_time(0.);
        }
        let rotation = chart.lines[0].fetch_rot(&chart.lines);
        let translation = chart.lines[0].object.translation.now();
        let height = chart.lines[0].notes[0].height;
        let ids = crate::picking::source_map(&document, &chart);
        let anchors = Anchors::install(&document, &mut chart);
        assert_eq!(chart.lines.len(), 3);
        assert_eq!(anchors.source_lines, 2);
        assert_eq!(chart.order, vec![2, 0, 1]);
        assert_eq!(chart.lines[0].object.translation.now(), translation);
        assert_eq!(chart.lines[0].notes[0].height, height);
        assert_eq!(chart.lines[0].parent, Some(1));
        assert_eq!(chart.lines[2].parent, Some(0));
        assert!(chart.lines[2].notes.is_empty());
        assert_eq!(chart.lines[0].fetch_rot(&chart.lines), rotation);
        assert_eq!(chart.lines[2].fetch_rot(&chart.lines), rotation);
        assert_eq!(crate::picking::source_map(&document, &chart), ids);
    }
}
