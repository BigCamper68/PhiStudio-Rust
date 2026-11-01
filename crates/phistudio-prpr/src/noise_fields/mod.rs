//! Preview-only support for Phigros blockAreaList; no authoring state is added.
mod input;
mod model;
mod render;

use anyhow::{Context, Result};
use model::BlockArea;
use phistudio_core::ChartDocument;
use prpr::core::Resource;

#[derive(Default)]
pub(crate) struct NoiseFields {
    areas: Vec<BlockArea>,
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
        self.areas = areas;
        Ok(())
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
        t: f32,
        aspect: f32,
        dt: f32,
    ) {
        self.input.filter(touches, &self.areas, t, aspect, dt);
    }

    pub(crate) fn suppressed(&self, id: u64) -> bool {
        self.input.suppressed(id)
    }

    pub(crate) fn touching(&self) -> bool {
        !self.input.positions.is_empty()
    }

    pub(crate) fn render(&mut self, resource: &mut Resource, viewport: (i32, i32, i32, i32)) {
        if let Some(renderer) = &mut self.renderer {
            renderer.render(resource, &self.areas, &self.input, viewport);
        }
    }
}

fn parse(document: &ChartDocument) -> Result<Vec<BlockArea>> {
    let raw = &document.as_json()["blockAreaList"];
    if raw.is_null() {
        return Ok(Vec::new());
    }
    let mut areas: Vec<BlockArea> =
        serde_json::from_value(raw.clone()).context("Invalid blockAreaList")?;
    for (index, area) in areas.iter_mut().enumerate() {
        area.normalize()
            .with_context(|| format!("Invalid Noise Field {index}"))?;
    }
    Ok(areas)
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
        raw["blockAreaList"] = json!([{"phiStudioColor":[1,2,3],"scaleEvents":[
            {"time":3.,"anchor":{"x":0.5,"y":0.5},"scale":{"x":1.,"y":1.}},
            {"time":1.,"anchor":{"x":0.5,"y":0.5},"scale":{"x":0.,"y":0.}}
        ]}]);
        let doc = ChartDocument::from_value(raw.clone()).unwrap();
        let fields = parse(&doc).unwrap();
        assert_eq!(fields[0].scale_events[0].time, 1.);
        assert_eq!(doc.as_json(), &raw);
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
