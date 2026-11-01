//! Use Phira's original extra.json parser, shader materials and ping-pong passes.
use crate::Preview;
use macroquad::prelude::*;
use phistudio_core::{ChartDocument, shaders};
use prpr::core::ChartExtra;
use std::path::Path;
#[derive(PartialEq)]
pub(crate) struct ShaderKey {
    source: serde_json::Value,
    sources: serde_json::Map<String, serde_json::Value>,
    root: Option<std::path::PathBuf>,
    files: Vec<(String, Option<std::time::SystemTime>, u64)>,
}
fn key(document: &ChartDocument, root: Option<&Path>) -> anyhow::Result<ShaderKey> {
    let source = shaders::load_shader_extra(document, root)?;
    let mut files = Vec::new();
    for e in shaders::shader_effects(&source)? {
        if let Some(name) = e.shader.strip_prefix('/') {
            let meta = root.and_then(|r| r.join(name.replace('\\', "/")).metadata().ok());
            files.push((
                name.into(),
                meta.as_ref().and_then(|m| m.modified().ok()),
                meta.map_or(0, |m| m.len()),
            ));
        }
    }
    Ok(ShaderKey {
        source,
        sources: shaders::shader_sources(document)?,
        root: root.map(Path::to_owned),
        files,
    })
}
async fn compile(key: &ShaderKey) -> anyhow::Result<ChartExtra> {
    let mut fs =
        crate::assets::ChartAssets::new(key.root.as_deref())?.with_sources(key.sources.clone());
    prpr::parse::parse_extra(&serde_json::to_string(&key.source)?, &mut fs).await
}
pub(crate) async fn load(
    document: &ChartDocument,
    root: Option<&Path>,
) -> (ChartExtra, Option<String>, Option<ShaderKey>) {
    match key(document, root) {
        Ok(key) => match compile(&key).await {
            Ok(extra) => (extra, None, Some(key)),
            Err(e) => (
                ChartExtra::default(),
                Some(format!("Could not load chart shaders: {e:#}")),
                Some(key),
            ),
        },
        Err(e) => (
            ChartExtra::default(),
            Some(format!("Could not load chart shaders: {e:#}")),
            None,
        ),
    }
}
pub(crate) async fn replace(
    preview: &mut Preview,
    chart: &mut prpr::core::Chart,
    document: &ChartDocument,
    root: Option<&Path>,
) {
    if let Ok(next) = key(document, root)
        && preview.shader_key.as_ref() == Some(&next)
    {
        chart.extra = std::mem::take(&mut preview.chart.extra);
        return;
    }
    let (extra, warning, key) = load(document, root).await;
    let failed = warning.is_some();
    preview.shader_warning = warning;
    preview.shader_key = key;
    chart.extra = if failed {
        std::mem::take(&mut preview.chart.extra)
    } else {
        extra
    };
}
impl Preview {
    pub(crate) fn configure_effects(&mut self) {
        let disabled = !self.noise_fields.enabled()
            && (!self.editor_options.preview_shaders
                || (self.chart.extra.effects.is_empty()
                    && self.chart.extra.global_effects.is_empty()));
        if self.resource.no_effect != disabled {
            self.resource.no_effect = disabled;
            self.resource.config.disable_effect = disabled;
            self.resource.last_vp = (0, 0, 0, 0);
            if disabled {
                self.resource.chart_target = None;
            }
        }
    }
    pub(crate) fn effect_target(&self) -> RenderTarget {
        self.resource
            .chart_target
            .as_ref()
            .map_or(self.target, |t| t.output())
    }
    pub(crate) fn finish_effects(&mut self, viewport: (i32, i32, i32, i32)) {
        if self.resource.no_effect {
            return;
        }
        let aspect = viewport.2 as f32 / viewport.3 as f32;
        let target = self.effect_target();
        set_camera(&Camera2D {
            zoom: vec2(1., aspect),
            viewport: None,
            render_target: Some(target),
            ..Default::default()
        });
        for effect in self
            .chart
            .extra
            .global_effects
            .iter_mut()
            .filter(|_| self.editor_options.preview_shaders)
        {
            effect.update(&self.resource);
            effect.render(&mut self.resource);
        }
        let target = self.effect_target();
        unsafe { get_internal_gl() }.flush();
        set_camera(&Camera2D {
            zoom: vec2(1., aspect),
            viewport: Some(viewport),
            render_target: Some(self.target),
            ..Default::default()
        });
        draw_texture_ex(
            target.texture,
            -1.,
            -1. / aspect,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(2., 2. / aspect)),
                ..Default::default()
            },
        );
    }
    pub fn shader_report(&self) -> serde_json::Value {
        serde_json::json!({"enabled":!self.resource.no_effect,"local":self.chart.extra.effects.len(),"global":self.chart.extra.global_effects.len(),"warning":self.shader_warning})
    }
}

/// The authoring inspector discovers parameters from the exact playback source.
pub fn shader_preset(name: &str) -> Option<&'static str> {
    prpr::core::Effect::get_preset(name)
}
