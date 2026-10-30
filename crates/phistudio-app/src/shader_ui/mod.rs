//! Shader workspace: native Phira effects, shared project history and transport.
mod uniforms;
use uniforms::{is_animated, rgba_value, uniform_widget};
mod animation_values;
use animation_values::animation_draft;
mod highlight;
use highlight::glsl_layout;
mod animation;
mod commands;
mod model;
mod properties;
mod shortcuts;
mod source;
mod toolbar;
use crate::{
    advanced_ui::RightMode,
    authoring_ui::{draw_curve, shader_easing_fields},
    media::{MediaFile, MediaKind},
    scroll::{TouchCombo as _, TouchScroll as _},
    theme::*,
    workbench::Workbench,
};
use egui::{FontId, Rect, Sense, vec2};
pub(crate) mod grid;
mod navigation;
use phistudio_core::{Beat, Command, EventType, LineEvent, TempoMap, shaders::*};
use serde_json::{Value, json};

const TEMPLATE: &str = "#version 100\nprecision mediump float;\nvarying vec2 uv;\nuniform sampler2D screenTexture;\nuniform float time;\nuniform vec2 screenSize;\nuniform float amount; // %0.5%\n\nvoid main() {\n    vec4 color = texture2D(screenTexture, uv);\n    gl_FragColor = vec4(mix(color.rgb, color.bgr, amount), color.a);\n}\n";
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
enum ShaderPage {
    #[default]
    Effect,
    Parameters,
    Animation,
    Tempo,
}
#[derive(Default)]
pub(crate) struct ShaderState {
    key: Option<(u64, u64)>,
    source: Value,
    effects: Vec<ShaderEffect>,
    tempo: Option<TempoMap>,
    error: Option<String>,
    pub selected: Option<usize>,
    draft: Option<EffectDraft>,
    parameters: Vec<ShaderParameter>,
    preset: usize,
    code_view: bool,
    code: String,
    code_original: String,
    code_path: String,
    code_target: String,
    new_uniform: String,
    uniform_kind: usize,
    animation: Option<AnimationDraft>,
    track: Option<String>,
    page: ShaderPage,
    tempo_rows: Option<Vec<BpmDraft>>,
    clipboard: Option<Value>,
    keyframe_clipboard: Option<Value>,
}
impl ShaderState {
    fn animation_track(&self) -> Option<&str> {
        (self.page == ShaderPage::Animation)
            .then_some(self.track.as_deref())
            .flatten()
    }
}
struct BpmDraft {
    raw: Value,
    beat: String,
    bpm: f64,
}
struct EffectDraft {
    index: usize,
    raw: Value,
    start: String,
    end: String,
}
struct AnimationDraft {
    name: String,
    index: usize,
    raw: Value,
    start: String,
    end: String,
    curve: LineEvent,
}

#[cfg(test)]
mod tests;
