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
    Password {
        id: String,
        text: String,
        value: SecretText,
    },
    Table {
        id: String,
        columns: Vec<String>,
        rows: Vec<TableRow>,
        #[serde(default)]
        selected: Option<String>,
        #[serde(default = "default_table_height")]
        height: f32,
    },
    Tree {
        id: String,
        nodes: Vec<TreeNode>,
        #[serde(default)]
        selected: Option<String>,
    },
    Tabs {
        id: String,
        selected: String,
        tabs: Vec<UiTab>,
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
fn default_table_height() -> f32 {
    240.
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableRow {
    pub id: String,
    pub cells: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeNode {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub children: Vec<TreeNode>,
    #[serde(default)]
    pub expanded: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiTab {
    pub id: String,
    pub title: String,
    pub children: Vec<Widget>,
}

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretText(pub String);
impl std::fmt::Debug for SecretText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}
impl From<String> for SecretText {
    fn from(value: String) -> Self {
        Self(value)
    }
}
impl From<&str> for SecretText {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
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
