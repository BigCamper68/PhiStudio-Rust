use crate::{DrawColor, DrawLayer};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The host renders descriptions with its own theme, localization and touch UI.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Widget {
    Label {
        text: String,
    },
    Button {
        id: String,
        text: String,
        #[serde(default = "enabled")]
        enabled: bool,
    },
    Bool {
        id: String,
        text: String,
        value: bool,
    },
    Number {
        id: String,
        text: String,
        value: f64,
        min: f64,
        max: f64,
    },
    Text {
        id: String,
        text: String,
        value: String,
    },
    Choice {
        id: String,
        text: String,
        value: String,
        options: Vec<(String, String)>,
    },
    Row {
        children: Vec<Widget>,
    },
    Column {
        children: Vec<Widget>,
    },
    Canvas {
        height: f32,
        layers: Vec<DrawLayer>,
    },
}
fn enabled() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UiEvent {
    pub panel: String,
    pub widget: String,
    pub value: Value,
}

/// Coordinates use the containing layer's space; dimensions remain in points.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DrawPrimitive {
    /// An image prepared through host/assets.image by this plugin activation.
    Image {
        file: u64,
        min: [f64; 2],
        max: [f64; 2],
        tint: DrawColor,
    },
    Line {
        from: [f64; 2],
        to: [f64; 2],
        color: DrawColor,
        width: f32,
    },
    Rect {
        min: [f64; 2],
        max: [f64; 2],
        color: DrawColor,
    },
    Circle {
        center: [f64; 2],
        radius: f32,
        color: DrawColor,
    },
    Text {
        position: [f64; 2],
        text: String,
        size: f32,
        color: DrawColor,
    },
}
