use super::*;
use crate::{i18n::tr, scroll::TouchScroll as _, workbench::Workbench};
use egui::{Color32, FontId, Rect, pos2, vec2};

impl Workbench {
    pub(crate) fn open_line_textures(&mut self) {
        if let Some(draft) = &self.line_draft {
            self.line_tools.open(
                self.session,
                self.line,
                self.editor.revision(),
                draft.properties.texture.clone(),
            );
            self.transport.playing = false;
        }
    }
    pub(crate) fn stage_line_texture(&mut self, texture: String) {
        if self.line_tools.show
            && self.line_tools.target == (self.session, self.line, self.editor.revision())
            && let Some(draft) = &mut self.line_draft
            && draft.index == self.line
            && draft.revision == self.editor.revision()
        {
            draft.properties.texture = texture;
        }
        self.line_tools.show = false;
        self.line_tools.importing = false;
    }
    pub(crate) fn line_texture_window(&mut self, ctx: &Context) {
        if !self.line_tools.show {
            return;
        }
        if self.line_tools.target != (self.session, self.line, self.editor.revision()) {
            self.line_tools.show = false;
            return;
        }
        let root = self.asset_root().map(std::path::Path::to_owned);
        let can_import = self.current_workspace().is_some();
        self.line_tools
            .sync(ctx, self.session, self.asset_revision, root);
        let mut open = true;
        let mut choice = None;
        let mut import = false;
        let mut cancel = false;
        crate::i18n::window("Choose line texture")
            .open(&mut open)
            .default_pos([36., 48.])
            .default_width(680.)
            .default_height(440.)
            .resize(|r| {
                let maximum = vec2(
                    (ctx.screen_rect().width() - 24.).max(180.),
                    (ctx.screen_rect().height() - 80.).max(100.),
                );
                r.min_size([maximum.x.min(340.), maximum.y.min(240.)])
                    .max_size(maximum)
            })
            .show(ctx, |ui| {
                ui.set_enabled(!self.files.busy && self.files.media.is_none());
                ui.style_mut().wrap = Some(true);
                ui.horizontal(|ui| {
                    if ui.button(tr("Reload list")).clicked() {
                        self.line_tools.key = None;
                    }
                    ui.label(tr("Search"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.line_tools.query)
                            .desired_width(ui.available_width().max(20.)),
                    );
                });
                if let Some(error) = &self.line_tools.error {
                    ui.colored_label(crate::theme::flick(), tr(error));
                }
                let query = self.line_tools.query.to_lowercase();
                let names = std::iter::once("line.png".to_owned())
                    .chain(
                        self.line_tools
                            .names
                            .as_ref()
                            .into_iter()
                            .flatten()
                            .filter(|s| s.as_str() != "line.png")
                            .cloned(),
                    )
                    .filter(|s| s.to_lowercase().contains(&query))
                    .collect::<Vec<_>>();
                let height = (ui.available_height() - 78.).max(40.);
                let tile_height = if ctx.screen_rect().height() < 320. {
                    64.
                } else {
                    112.
                };
                egui::ScrollArea::vertical()
                    .id_source("line-texture-grid")
                    .max_height(height)
                    .touch_show_y(ui, |ui| {
                        let columns = ((ui.available_width() + 8.) / 148.).floor().max(1.) as usize;
                        let width = ((ui.available_width()
                            - 8. * columns.saturating_sub(1) as f32)
                            / columns as f32)
                            .max(80.);
                        egui::Grid::new("line-texture-tiles")
                            .spacing([8., 8.])
                            .show(ui, |ui| {
                                for (index, name) in names.iter().enumerate() {
                                    if self.line_tools.tile(ui, name, width, tile_height) {
                                        choice = Some(name.clone());
                                    }
                                    if (index + 1) % columns == 0 {
                                        ui.end_row();
                                    }
                                }
                            });
                        if self.line_tools.names.is_none() {
                            ui.spinner();
                        }
                    });
                ui.horizontal_wrapped(|ui| {
                    import = ui
                        .add_enabled(can_import, egui::Button::new(tr("Import image…")))
                        .clicked();
                    if ui.button(tr("Cancel")).clicked() {
                        cancel = true;
                    }
                });
                ui.small(tr("Choose a texture, then apply the line properties."));
            });
        self.line_tools.show = open && !cancel;
        if let Some(texture) = choice {
            self.stage_line_texture(texture);
        }
        if import {
            self.workspace.resource_target = None;
            self.open_media_picker(crate::media::MediaKind::LineTexture);
        }
    }
}
impl State {
    fn tile(&mut self, ui: &mut egui::Ui, name: &str, width: f32, height: f32) -> bool {
        let builtin = name == "line.png";
        let visible = ui.is_rect_visible(Rect::from_min_size(
            ui.next_widget_position(),
            vec2(width, height),
        ));
        let image = if !builtin && visible {
            self.image(ui.ctx(), name)
        } else {
            None
        };
        let selected = self.current == name;
        let ready = builtin || image.is_some();
        let mut button = egui::Button::new("");
        if selected {
            button = button
                .fill(ui.visuals().selection.bg_fill)
                .stroke(ui.visuals().selection.stroke);
        }
        let response = ui
            .add_enabled_ui(ready, |ui| ui.add_sized([width, height], button))
            .inner;
        if !ui.is_rect_visible(response.rect) {
            return false;
        }
        let painter = ui
            .painter()
            .with_clip_rect(response.rect.intersect(ui.clip_rect()));
        let caption_height = if height < 96. { 18. } else { 30. };
        let area = Rect::from_min_max(
            response.rect.min + vec2(8., 8.),
            pos2(
                response.rect.right() - 8.,
                response.rect.bottom() - caption_height - 4.,
            ),
        );
        painter.rect_filled(area, 2., ui.visuals().extreme_bg_color);
        if builtin {
            painter.line_segment(
                [
                    pos2(area.left() + 8., area.center().y),
                    pos2(area.right() - 8., area.center().y),
                ],
                egui::Stroke::new(2_f32, Color32::from_rgb(241, 216, 148)),
            );
        } else if let Some(image) = image {
            let size = image.size_vec2();
            let scale = (area.width() / size.x).min(area.height() / size.y);
            painter.image(
                image.id(),
                Rect::from_center_size(area.center(), size * scale),
                Rect::from_min_max(pos2(0., 0.), pos2(1., 1.)),
                Color32::WHITE,
            );
        } else if visible {
            let failed = self.entries.get(name).is_some_and(|e| !e.loading);
            painter.text(
                area.center(),
                egui::Align2::CENTER_CENTER,
                tr(if failed {
                    "Image unavailable"
                } else {
                    "Loading…"
                }),
                FontId::proportional(12.),
                ui.visuals().weak_text_color(),
            );
        }
        let caption = if builtin {
            tr("Default line")
        } else {
            name.rsplit('/').next().unwrap_or(name).to_owned()
        };
        let caption_rect = Rect::from_min_max(
            pos2(
                response.rect.left() + 6.,
                response.rect.bottom() - caption_height,
            ),
            response.rect.max - vec2(6., 4.),
        );
        let galley = ui.fonts(|f| {
            f.layout(
                caption,
                FontId::proportional(12.),
                ui.visuals().text_color(),
                caption_rect.width(),
            )
        });
        painter
            .with_clip_rect(caption_rect.intersect(ui.clip_rect()))
            .galley(caption_rect.min, galley);
        response.on_hover_text(name).clicked()
    }
}
