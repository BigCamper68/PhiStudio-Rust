//! Noise Field playback uses shared native/Next snapshots and original Phigros materials.
mod colors;
mod input;
mod model;
mod render;

use anyhow::{Context, Result};
use phistudio_core::ChartDocument;
use phistudio_core::noise_fields::{Field, Sample};
use prpr::core::Resource;

#[derive(Default)]
pub(crate) struct NoiseFields {
    areas: Vec<Field>,
    override_area: Option<(usize, Field)>,
    samples: Vec<Sample>,
    sample_key: Option<(u64, u32)>,
    next_parity: bool,
    single_inversion: bool,
    input: input::Input,
    renderer: Option<render::Renderer>,
}

impl NoiseFields {
    pub(crate) async fn load(&mut self, document: &ChartDocument) -> Result<()> {
        let areas = parse(document)?;
        if !areas.is_empty() && self.renderer.is_none() {
            self.renderer =
                Some(render::Renderer::new().context("Could not initialize Noise Fields")?);
        }
        if areas.is_empty() {
            self.renderer = None;
        }
        self.input.reset();
        self.samples.clear();
        self.sample_key = None;
        self.next_parity = areas.iter().any(|a| a.next.is_some());
        self.single_inversion = self.next_parity
            && document.as_json()["noiseAreaOptions"]["ignoreTripleInversion"]
                .as_bool()
                .unwrap_or(false);
        self.override_area = None;
        self.areas = areas;
        Ok(())
    }

    pub(crate) fn set_override(&mut self, field: Option<(usize, Field)>) {
        self.override_area = field;
        self.sample_key = None;
    }
    pub(crate) fn enabled(&self) -> bool {
        !self.areas.is_empty()
    }

    pub(crate) fn reset_input(&mut self) {
        self.input.reset();
    }

    pub(crate) fn filter_touches(
        &mut self,
        touches: &mut Vec<macroquad::input::Touch>,
        t: f64,
        aspect: f32,
        dt: f32,
    ) {
        self.sample(t, aspect);
        self.input
            .filter_samples(touches, &self.samples, aspect, dt, self.single_inversion);
    }

    pub(crate) fn suppressed(&self, id: u64) -> bool {
        self.input.suppressed(id)
    }

    pub(crate) fn touching(&self) -> bool {
        !self.input.positions.is_empty()
    }

    fn sample(&mut self, time: f64, aspect: f32) {
        let key = (time.to_bits(), aspect.to_bits());
        if self.sample_key == Some(key) {
            return;
        }
        self.samples = self
            .areas
            .iter()
            .enumerate()
            .filter_map(|(index, original)| {
                let field = self
                    .override_area
                    .as_ref()
                    .filter(|(i, _)| *i == index)
                    .map_or(original, |(_, field)| field);
                field.sample(time, aspect)
            })
            .collect();
        self.sample_key = Some(key);
    }
    pub(crate) fn render(&mut self, resource: &mut Resource, viewport: (i32, i32, i32, i32)) {
        self.sample(resource.time, resource.aspect_ratio);
        if let Some(renderer) = &mut self.renderer {
            renderer.render(
                resource,
                &self.samples,
                &self.input,
                viewport,
                self.next_parity && !self.single_inversion,
                self.single_inversion,
            );
        }
    }
}

fn parse(document: &ChartDocument) -> Result<Vec<Field>> {
    phistudio_core::noise_fields::fields(document)?
        .iter()
        .enumerate()
        .map(|(i, value)| {
            Field::new(value, document.tempo()).with_context(|| format!("Invalid Noise Field {i}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn absent_null_and_empty_fields_preserve_the_existing_chart_path() {
        let mut raw = ChartDocument::empty("Noise", 120.)
            .unwrap()
            .as_json()
            .clone();
        for value in [serde_json::Value::Null, json!([])] {
            raw["blockAreaList"] = value;
            assert!(
                parse(&ChartDocument::from_value(raw.clone()).unwrap())
                    .unwrap()
                    .is_empty()
            );
        }
    }
    #[test]
    fn sorting_renderer_keys_does_not_rewrite_the_authored_document() {
        let mut raw = ChartDocument::empty("Noise", 120.)
            .unwrap()
            .as_json()
            .clone();
        raw["blockAreaList"] = json!([{"bottomLeftPercentage":{"x":0.,"y":0.},"topRightPercentage":{"x":1.,"y":1.},"appearTime":0.,"enableTime":1.,"disableTime":3.,"disappearTime":4.,"phiStudioColor":[1,2,3],"scaleEvents":[
            {"time":3.,"anchor":{"x":0.5,"y":0.5},"scale":{"x":1.,"y":1.}},
            {"time":1.,"anchor":{"x":0.5,"y":0.5},"scale":{"x":0.,"y":0.}}
        ]}]);
        let doc = ChartDocument::from_value(raw.clone()).unwrap();
        let fields = parse(&doc).unwrap();
        assert_eq!(fields[0].native.as_ref().unwrap().scale_events[0].time, 1.);
        assert_eq!(doc.as_json(), &raw);
    }
    #[test]
    fn staged_geometry_invalidates_samples_and_clears_without_changing_source() {
        let mut source = ChartDocument::empty("Override", 120.)
            .unwrap()
            .as_json()
            .clone();
        source["blockAreaList"] = json!([{"bottomLeftPercentage":{"x":0.1,"y":0.1},"topRightPercentage":{"x":0.8,"y":0.8},"appearTime":0.,"enableTime":1.,"disableTime":3.,"disappearTime":4.}]);
        let doc = ChartDocument::from_value(source.clone()).unwrap();
        let mut fields = NoiseFields {
            areas: parse(&doc).unwrap(),
            ..Default::default()
        };
        fields.sample(2., 16. / 9.);
        let initial = fields.samples[0].pose.center;
        let mut raw = source["blockAreaList"][0].clone();
        raw["bottomLeftPercentage"]["x"] = json!(0.4);
        fields.set_override(Some((0, Field::new(&raw, doc.tempo()).unwrap())));
        fields.sample(2., 16. / 9.);
        assert_ne!(fields.samples[0].pose.center, initial);
        fields.set_override(None);
        fields.sample(2., 16. / 9.);
        assert_eq!(fields.samples[0].pose.center, initial);
        assert_eq!(doc.as_json(), &source);
    }
    #[test]
    fn draft_samples_keep_lifetime_phase_opacity_and_subtract_in_the_shared_list() {
        use phistudio_core::{
            Beat,
            noise_fields::{self as noise, Format},
        };
        let doc = ChartDocument::empty("Draft sampling", 120.).unwrap();
        for format in [Format::Native, Format::Next] {
            let raw = noise::create(
                Beat::new(4, 0, 1).unwrap(),
                Beat::new(12, 0, 1).unwrap(),
                format,
                true,
                doc.tempo(),
            );
            let field = Field::new(&raw, doc.tempo()).unwrap();
            let mut fields = NoiseFields {
                areas: vec![field.clone()],
                ..Default::default()
            };
            fields.set_override(Some((0, field.clone())));
            for time in [0., 2., 2.25, 3., 5.5, 6., 10.] {
                fields.sample(time, 16. / 9.);
                let expected = field.sample(time, 16. / 9.);
                assert_eq!(fields.samples.len(), usize::from(expected.is_some()));
                if let Some(expected) = expected {
                    let actual = fields.samples[0];
                    assert_eq!(actual.pose.center, expected.pose.center);
                    assert_eq!(actual.phase, expected.phase);
                    assert_eq!(actual.opacity, expected.opacity);
                    assert!(actual.subtract);
                }
            }
        }
    }
    #[test]
    fn malformed_field_data_is_reported_instead_of_silently_ignored() {
        let mut raw = ChartDocument::empty("Noise", 120.)
            .unwrap()
            .as_json()
            .clone();
        raw["blockAreaList"] = json!([{"rotateEvents":[{"time":0.,"rotation":0.,"anchor":{"x":0.,"y":0.},"easeType":15}]}]);
        assert!(
            parse(&ChartDocument::from_value(raw).unwrap())
                .unwrap_err()
                .to_string()
                .contains("Noise Field 0")
        );
    }
}
