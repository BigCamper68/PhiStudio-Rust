use crate::scroll::TouchScroll as _;
use phistudio_sdk::{TableRow, TreeNode};
use serde_json::{Value, json};

pub(super) struct SecretBuffer<'a>(pub &'a mut String);
impl egui::TextBuffer for SecretBuffer<'_> {
    fn is_mutable(&self) -> bool {
        true
    }
    fn as_str(&self) -> &str {
        self.0
    }
    fn insert_text(&mut self, text: &str, index: usize) -> usize {
        let mut end = text
            .len()
            .min((16 * 1024usize).saturating_sub(self.0.len()));
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        egui::TextBuffer::insert_text(self.0, &text[..end], index)
    }
    fn delete_char_range(&mut self, range: std::ops::Range<usize>) {
        egui::TextBuffer::delete_char_range(self.0, range);
    }
}

pub(super) fn table(
    ui: &mut egui::Ui,
    id: &str,
    columns: &[String],
    rows: &[TableRow],
    selected: &mut Option<String>,
    height: f32,
) -> Option<Value> {
    let mut clicked = None;
    egui::ScrollArea::vertical()
        .id_source(("table", id))
        .max_height(height)
        .touch_show(ui, |ui| {
            egui::Grid::new(("table-grid", id))
                .striped(true)
                .show(ui, |ui| {
                    for title in columns {
                        ui.strong(crate::i18n::tr(title));
                    }
                    ui.end_row();
                    for row in rows {
                        for (column, cell) in row.cells.iter().enumerate() {
                            if ui
                                .push_id((&row.id, column), |ui| {
                                    ui.selectable_label(
                                        selected.as_deref() == Some(row.id.as_str()),
                                        crate::i18n::tr(cell),
                                    )
                                })
                                .inner
                                .clicked()
                            {
                                *selected = Some(row.id.clone());
                                clicked = Some(json!(row.id));
                            }
                        }
                        ui.end_row();
                    }
                });
        });
    clicked
}

pub(super) fn tree(
    ui: &mut egui::Ui,
    nodes: &[TreeNode],
    selected: &mut Option<String>,
    clicked: &mut Option<String>,
) {
    for node in nodes {
        ui.push_id(&node.id, |ui| {
            if node.children.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(ui.spacing().indent);
                    if ui
                        .selectable_label(
                            selected.as_deref() == Some(&node.id),
                            crate::i18n::tr(&node.text),
                        )
                        .clicked()
                    {
                        *selected = Some(node.id.clone());
                        *clicked = Some(node.id.clone());
                    }
                });
            } else {
                egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    ui.make_persistent_id("branch"),
                    node.expanded,
                )
                .show_header(ui, |ui| {
                    if ui
                        .selectable_label(
                            selected.as_deref() == Some(&node.id),
                            crate::i18n::tr(&node.text),
                        )
                        .clicked()
                    {
                        *selected = Some(node.id.clone());
                        *clicked = Some(node.id.clone());
                    }
                })
                .body(|ui| tree(ui, &node.children, selected, clicked));
            }
        });
    }
}
