//! Shared color palette: controls on the left, visual color space on the right.
use crate::{
    advanced_ui::StoryId,
    scroll::{TouchCombo as _, TouchScroll as _},
    workbench::Workbench,
};
use egui::{Color32, Rect, Sense, Stroke, ecolor::HsvaGamma as Hsva, pos2, vec2};

const ROLES: [&str; 11] = [
    "Background",
    "Panels",
    "Controls",
    "Borders",
    "Muted text",
    "Text",
    "Accent",
    "Tap guides",
    "Hold guides",
    "Flick guides",
    "Drag guides",
];
#[derive(Clone)]
enum Target {
    NoteDefaults {
        before: Option<[u8; 3]>,
    },
    Noise {
        field: usize,
        keyframe: Option<(usize, bool)>,
        session: u64,
        revision: u64,
    },
    Note {
        id: phistudio_core::NoteId,
        session: u64,
        revision: u64,
    },
    Shader {
        effect: usize,
        name: String,
        keyframe: Option<(usize, bool)>,
        session: u64,
        revision: u64,
    },
    Story {
        id: StoryId,
        end: bool,
        session: u64,
        revision: u64,
    },
    Theme(Option<usize>),
}
pub(crate) struct ColorPicker {
    target: Target,
    color: Hsva,
    original: [u8; 4],
    hex: String,
    theme: phistudio_core::ThemeColors,
    live: bool,
}
pub(crate) fn swatch(ui: &mut egui::Ui, rgb: [u8; 3], label: &str) -> egui::Response {
    let response = ui.add_sized(
        [ui.available_width().min(160.), 32.],
        egui::Button::new(crate::i18n::tr(label)),
    );
    let rect = Rect::from_min_size(response.rect.min + vec2(6., 6.), vec2(18., 20.));
    ui.painter()
        .rect_filled(rect, 2., Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
    response
}
impl Workbench {
    pub(crate) fn open_note_default_color(&mut self, rgb: [u8; 3]) {
        self.open_color(
            Target::NoteDefaults {
                before: self.options.new_note_defaults.tint_hit_effects,
            },
            [rgb[0], rgb[1], rgb[2], 255],
        );
    }
    pub(crate) fn open_note_color(&mut self, id: phistudio_core::NoteId, rgb: [u8; 3]) {
        self.open_color(
            Target::Note {
                id,
                session: self.session,
                revision: self.editor.revision(),
            },
            [rgb[0], rgb[1], rgb[2], 255],
        );
    }
    pub(crate) fn open_story_color(&mut self, id: StoryId, end: bool, rgb: [u8; 3]) {
        self.open_color(
            Target::Story {
                id,
                end,
                session: self.session,
                revision: self.editor.revision(),
            },
            [rgb[0], rgb[1], rgb[2], 255],
        );
    }
    pub(crate) fn open_shader_color(
        &mut self,
        name: String,
        keyframe: Option<(usize, bool)>,
        rgba: [u8; 4],
    ) {
        if let Some(effect) = self.shaders.selected {
            self.open_color(
                Target::Shader {
                    effect,
                    name,
                    keyframe,
                    session: self.session,
                    revision: self.editor.revision(),
                },
                rgba,
            );
        }
    }
    pub(crate) fn open_noise_color(&mut self, keyframe: Option<(usize, bool)>, rgb: [u8; 3]) {
        if let Some(field) = self.noise.selected {
            self.open_color(
                Target::Noise {
                    field,
                    keyframe,
                    session: self.session,
                    revision: self.editor.revision(),
                },
                [rgb[0], rgb[1], rgb[2], 255],
            );
        }
    }
    fn open_color(&mut self, target: Target, rgba: [u8; 4]) {
        self.palette = Some(ColorPicker {
            target,
            color: Hsva::from_srgba_unmultiplied(rgba),
            original: rgba,
            hex: hex(rgba),
            theme: self.options.theme,
            live: true,
        });
    }
    pub(crate) fn close_color_picker(&mut self) -> bool {
        if let Some(p) = self.palette.take() {
            if matches!(p.target, Target::Theme(_)) {
                self.options.theme = p.theme;
            }
            true
        } else {
            false
        }
    }
    pub(crate) fn theme_settings(&mut self, ui: &mut egui::Ui) {
        ui.strong(crate::i18n::tr("Appearance"));
        let c = self.options.theme.colors[6];
        if swatch(ui, [c[0], c[1], c[2]], "Theme color…").clicked() {
            self.open_color(Target::Theme(None), c);
        }
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_source("theme-role")
                .width(130.)
                .selected_text(crate::i18n::tr(ROLES[self.theme_role]))
                .touch_show_ui(ui, |ui| {
                    for (i, label) in ROLES.iter().enumerate() {
                        ui.selectable_value(&mut self.theme_role, i, crate::i18n::tr(*label));
                    }
                });
            let c = self.options.theme.colors[self.theme_role];
            if swatch(ui, [c[0], c[1], c[2]], "Customize…").clicked() {
                self.open_color(Target::Theme(Some(self.theme_role)), c);
            }
        });
        if ui.button(crate::i18n::tr("Reset theme")).clicked() {
            self.options.theme = Default::default();
        }
    }
    pub(crate) fn color_window(&mut self, ctx: &egui::Context) {
        let Some(mut p) = self.palette.take() else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        let before = p.color.to_srgba_unmultiplied();
        crate::i18n::window("Color palette")
            .id(egui::Id::new("color-palette"))
            .open(&mut open)
            .collapsible(false)
            .default_pos([48., 48.])
            .default_width((ctx.screen_rect().width() - 32.).min(610.))
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_width((ctx.screen_rect().width() - 48.).min(598.));
                let height = (ctx.screen_rect().height() - 160.).clamp(140., 370.);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        vec2((ui.available_width() * 0.42).clamp(140., 205.), height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .id_source("palette-settings")
                                .max_height(height)
                                .touch_show(ui, |ui| {
                                    ui.strong(crate::i18n::tr("Color settings"));
                                    let mut rgba = p.color.to_srgba_unmultiplied();
                                    let mut changed = false;
                                    ui.horizontal(|ui| {
                                        for (i, label) in ["R ", "G ", "B "].iter().enumerate() {
                                            changed |= ui
                                                .add(
                                                    crate::numeric::Value::new(&mut rgba[i])
                                                        .prefix(*label)
                                                        .clamp_range(0..=255),
                                                )
                                                .changed();
                                        }
                                    });
                                    let alpha = !matches!(
                                        p.target,
                                        Target::Story { .. }
                                            | Target::Note { .. }
                                            | Target::NoteDefaults { .. }
                                            | Target::Noise { .. }
                                    );
                                    changed |= ui
                                        .add_enabled(
                                            alpha,
                                            crate::numeric::Value::new(&mut rgba[3])
                                                .prefix(crate::i18n::tr("A "))
                                                .clamp_range(0..=255),
                                        )
                                        .changed();
                                    if matches!(p.target, Target::Story { .. }) {
                                        ui.small(crate::i18n::tr("Opacity uses Alpha events."));
                                    }
                                    if changed {
                                        p.color = Hsva::from_srgba_unmultiplied(rgba);
                                    }
                                    let mut degrees = p.color.h * 360.;
                                    ui.label(crate::i18n::tr("Hue"));
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut degrees, 0. ..=360.).suffix("°"),
                                        )
                                        .changed()
                                    {
                                        p.color.h = degrees / 360.;
                                    }
                                    ui.label(crate::i18n::tr("Saturation"));
                                    ui.add(egui::Slider::new(&mut p.color.s, 0. ..=1.));
                                    ui.label(crate::i18n::tr("Brightness"));
                                    ui.add(egui::Slider::new(&mut p.color.v, 0. ..=1.));
                                    ui.label(crate::i18n::tr("Hex RGB"));
                                    if ui.text_edit_singleline(&mut p.hex).changed()
                                        && let Some(rgb) = parse_hex(&p.hex)
                                    {
                                        p.color = Hsva::from_srgba_unmultiplied([
                                            rgb[0],
                                            rgb[1],
                                            rgb[2],
                                            p.color.to_srgba_unmultiplied()[3],
                                        ]);
                                    }
                                    if matches!(p.target, Target::Theme(_)) {
                                        ui.checkbox(
                                            &mut p.live,
                                            crate::i18n::tr("Live theme preview"),
                                        );
                                    }
                                });
                        },
                    );
                    ui.vertical(|ui| {
                        let width = ui.available_width().clamp(150., 340.);
                        let side = (height - 90.).min(width).max(64.);
                        sv(ui, &mut p.color, vec2(width, side));
                        hue(ui, &mut p.color, width);
                        ui.horizontal(|ui| {
                            if color_tile(ui, p.original, vec2(60., 24.))
                                .on_hover_text(crate::i18n::tr("Original color"))
                                .clicked()
                            {
                                p.color = Hsva::from_srgba_unmultiplied(p.original);
                            }
                            color_tile(ui, p.color.to_srgba_unmultiplied(), vec2(60., 24.))
                                .on_hover_text(crate::i18n::tr("New color"));
                        });
                        let recent = ctx
                            .data_mut(|d| {
                                d.get_temp::<Vec<[u8; 4]>>(egui::Id::new("recent-colors"))
                            })
                            .unwrap_or_default();
                        if height > 200. {
                            ui.horizontal_wrapped(|ui| {
                                for rgba in recent {
                                    if color_tile(ui, rgba, vec2(20., 20.)).clicked() {
                                        p.color = Hsva::from_srgba_unmultiplied(rgba);
                                    }
                                }
                            });
                        }
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    apply = ui.button(crate::i18n::tr("Apply color")).clicked();
                    cancel = ui.button(crate::i18n::tr("Cancel")).clicked();
                });
            });
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            cancel = true;
        }
        let rgba = p.color.to_srgba_unmultiplied();
        if rgba != before {
            p.hex = hex(rgba);
        }
        if let Target::Theme(role) = p.target {
            self.options.theme = p.theme;
            if p.live || apply {
                if let Some(i) = role {
                    self.options.theme.colors[i] = rgba;
                } else {
                    self.options.theme = tinted(p.color);
                }
            }
        }
        if apply {
            if let Target::NoteDefaults { before } = p.target
                && self.options.new_note_defaults.tint_hit_effects == before
            {
                self.options.new_note_defaults.tint_hit_effects = Some([rgba[0], rgba[1], rgba[2]]);
            }
            if let Target::Note {
                id,
                session,
                revision,
            } = p.target
            {
                if session == self.session
                    && revision == self.editor.revision()
                    && self.selected == Some(id)
                {
                    if let Ok(mut note) = self.editor.document().note(id.line, id.index) {
                        note.tint_hit_effects = Some([rgba[0], rgba[1], rgba[2]]);
                        self.execute(phistudio_core::Command::UpdateNote { id, note });
                    }
                } else {
                    self.error = Some("Selection changed. Reopen the color palette.".into());
                }
            }
            if let Target::Noise {
                field,
                keyframe,
                session,
                revision,
            } = &p.target
            {
                if *session == self.session
                    && *revision == self.editor.revision()
                    && self.noise.selected == Some(*field)
                {
                    self.apply_noise_color(*keyframe, [rgba[0], rgba[1], rgba[2]]);
                } else {
                    self.error = Some("Selection changed. Reopen the color palette.".into());
                }
            }
            if let Target::Shader {
                effect,
                name,
                keyframe,
                session,
                revision,
            } = &p.target
            {
                if *session == self.session
                    && *revision == self.editor.revision()
                    && self.shaders.selected == Some(*effect)
                {
                    self.apply_shader_color(name, *keyframe, rgba);
                } else {
                    self.error = Some("Selection changed. Reopen the color palette.".into());
                }
            }
            if let Target::Story {
                id,
                end,
                session,
                revision,
            } = p.target
            {
                if session == self.session
                    && revision == self.editor.revision()
                    && self.advanced.story_selected == Some(id)
                {
                    if let Some(draft) = &mut self.advanced.story {
                        let value = serde_json::json!([rgba[0], rgba[1], rgba[2]]);
                        if end {
                            draft.event.end = value;
                        } else {
                            draft.event.start = value;
                        }
                    }
                } else {
                    self.error = Some("Selection changed. Reopen the color palette.".into());
                }
            }
            ctx.data_mut(|d| {
                let colors =
                    d.get_temp_mut_or_default::<Vec<[u8; 4]>>(egui::Id::new("recent-colors"));
                colors.retain(|v| *v != rgba);
                colors.insert(0, rgba);
                colors.truncate(8);
            });
        } else if !open || cancel {
            if matches!(p.target, Target::Theme(_)) {
                self.options.theme = p.theme;
            }
        } else {
            self.palette = Some(p);
        }
    }
}
fn hex(c: [u8; 4]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}
fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    let n = u32::from_str_radix(s, 16).ok()?;
    if s.len() == 6 {
        Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
    } else if s.len() == 3 {
        Some([
            ((n >> 8) & 15) as u8 * 17,
            ((n >> 4) & 15) as u8 * 17,
            (n & 15) as u8 * 17,
        ])
    } else {
        None
    }
}
fn tinted(color: Hsva) -> phistudio_core::ThemeColors {
    let mut theme = phistudio_core::ThemeColors::default();
    for (i, c) in theme.colors.iter_mut().enumerate().take(7) {
        let old = Hsva::from_srgba_unmultiplied(*c);
        let s = if i == 6 {
            color.s
        } else if i < 4 {
            0.35 * color.s
        } else {
            0.12 * color.s
        };
        *c = Hsva {
            h: color.h,
            s,
            v: if i == 6 { color.v } else { old.v },
            a: if i == 6 { color.a } else { 1. },
        }
        .to_srgba_unmultiplied();
    }
    theme
}
fn color_tile(ui: &mut egui::Ui, c: [u8; 4], size: egui::Vec2) -> egui::Response {
    let (r, response) = ui.allocate_exact_size(size, Sense::click());
    ui.painter().rect_filled(
        r,
        2.,
        Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]),
    );
    ui.painter()
        .rect_stroke(r, 2., Stroke::new(1_f32, crate::theme::border()));
    response
}
// Match egui's perceptual HSV picker and sample its nonlinear saturation
// conversion. Four corner colors do not reproduce the selected RGB value.
trait PickerRgba {
    fn from_srgba_unmultiplied(rgba: [u8; 4]) -> Self;
    fn to_srgba_unmultiplied(&self) -> [u8; 4];
}
impl PickerRgba for Hsva {
    fn from_srgba_unmultiplied(rgba: [u8; 4]) -> Self {
        egui::ecolor::Hsva::from_srgba_unmultiplied(rgba).into()
    }
    fn to_srgba_unmultiplied(&self) -> [u8; 4] {
        egui::ecolor::Hsva::from(*self).to_srgba_unmultiplied()
    }
}
fn gradient(p: &egui::Painter, r: Rect, nx: u32, ny: u32, color: impl Fn(f32, f32) -> Color32) {
    let mut mesh = egui::epaint::Mesh::default();
    for y in 0..=ny {
        for x in 0..=nx {
            let tx = x as f32 / nx as f32;
            let ty = y as f32 / ny as f32;
            mesh.vertices.push(egui::epaint::Vertex {
                pos: pos2(r.left() + r.width() * tx, r.top() + r.height() * ty),
                uv: egui::epaint::WHITE_UV,
                color: color(tx, ty),
            });
            if x < nx && y < ny {
                let i = y * (nx + 1) + x;
                mesh.indices.extend_from_slice(&[
                    i,
                    i + 1,
                    i + nx + 1,
                    i + 1,
                    i + nx + 2,
                    i + nx + 1,
                ]);
            }
        }
    }
    p.add(egui::Shape::mesh(mesh));
}
fn sv(ui: &mut egui::Ui, c: &mut Hsva, size: egui::Vec2) {
    let (r, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    gradient(ui.painter(), r, 64, 64, |s, y| {
        Hsva {
            h: c.h,
            s,
            v: 1. - y,
            a: 1.,
        }
        .into()
    });
    if (response.clicked() || response.dragged())
        && let Some(p) = response.interact_pointer_pos()
    {
        c.s = ((p.x - r.left()) / r.width()).clamp(0., 1.);
        c.v = (1. - (p.y - r.top()) / r.height()).clamp(0., 1.);
    }
    let p = pos2(r.left() + c.s * r.width(), r.bottom() - c.v * r.height());
    ui.painter()
        .circle_stroke(p, 5., Stroke::new(2_f32, Color32::BLACK));
    ui.painter()
        .circle_stroke(p, 4., Stroke::new(1.5_f32, Color32::WHITE));
}
fn hue(ui: &mut egui::Ui, c: &mut Hsva, width: f32) {
    let (r, response) = ui.allocate_exact_size(vec2(width, 20.), Sense::click_and_drag());
    gradient(ui.painter(), r, 256, 1, |h, _| {
        Hsva {
            h,
            s: 1.,
            v: 1.,
            a: 1.,
        }
        .into()
    });
    if (response.clicked() || response.dragged())
        && let Some(p) = response.interact_pointer_pos()
    {
        c.h = ((p.x - r.left()) / r.width()).clamp(0., 1.);
    }
    let x = r.left() + r.width() * c.h;
    ui.painter().line_segment(
        [pos2(x, r.top()), pos2(x, r.bottom())],
        Stroke::new(2_f32, Color32::WHITE),
    );
}

#[cfg(test)]
mod tests;
