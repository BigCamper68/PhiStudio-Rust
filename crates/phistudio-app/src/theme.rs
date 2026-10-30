use egui::{Color32, FontId, Rounding, Stroke, Style, TextStyle, Vec2, Visuals};

thread_local! {
    static COLORS: std::cell::Cell<phistudio_core::ThemeColors> = std::cell::Cell::new(phistudio_core::ThemeColors::default());
}
pub fn apply(ctx: &egui::Context, colors: &phistudio_core::ThemeColors) {
    COLORS.with(|current| current.set(*colors));
    let key = egui::Id::new("applied-theme");
    let changed = ctx.data_mut(|d| {
        let changed = d.get_temp::<phistudio_core::ThemeColors>(key) != Some(*colors);
        d.insert_temp(key, *colors);
        changed
    });
    if changed {
        ctx.set_style(style());
    }
}
pub fn base() -> Color32 {
    COLORS.with(|c| {
        let v = c.get().colors[0];
        Color32::from_rgba_unmultiplied(v[0], v[1], v[2], v[3])
    })
}
pub fn panel() -> Color32 {
    COLORS.with(|c| {
        let v = c.get().colors[1];
        Color32::from_rgba_unmultiplied(v[0], v[1], v[2], v[3])
    })
}
pub fn raised() -> Color32 {
    COLORS.with(|c| {
        let v = c.get().colors[2];
        Color32::from_rgba_unmultiplied(v[0], v[1], v[2], v[3])
    })
}
pub fn border() -> Color32 {
    COLORS.with(|c| {
        let v = c.get().colors[3];
        Color32::from_rgba_unmultiplied(v[0], v[1], v[2], v[3])
    })
}
pub fn muted() -> Color32 {
    COLORS.with(|c| {
        let v = c.get().colors[4];
        Color32::from_rgba_unmultiplied(v[0], v[1], v[2], v[3])
    })
}
pub fn text() -> Color32 {
    COLORS.with(|c| {
        let v = c.get().colors[5];
        Color32::from_rgba_unmultiplied(v[0], v[1], v[2], v[3])
    })
}
pub fn accent() -> Color32 {
    COLORS.with(|c| {
        let v = c.get().colors[6];
        Color32::from_rgba_unmultiplied(v[0], v[1], v[2], v[3])
    })
}
pub fn tap() -> Color32 {
    COLORS.with(|c| {
        let v = c.get().colors[7];
        Color32::from_rgba_unmultiplied(v[0], v[1], v[2], v[3])
    })
}
pub fn hold() -> Color32 {
    COLORS.with(|c| {
        let v = c.get().colors[8];
        Color32::from_rgba_unmultiplied(v[0], v[1], v[2], v[3])
    })
}
pub fn flick() -> Color32 {
    COLORS.with(|c| {
        let v = c.get().colors[9];
        Color32::from_rgba_unmultiplied(v[0], v[1], v[2], v[3])
    })
}
pub fn drag() -> Color32 {
    COLORS.with(|c| {
        let v = c.get().colors[10];
        Color32::from_rgba_unmultiplied(v[0], v[1], v[2], v[3])
    })
}

pub fn history_button(ui: &mut egui::Ui, redo: bool, enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(32.), egui::Sense::click());
    let color = if enabled {
        if response.hovered() { accent() } else { text() }
    } else {
        muted().gamma_multiply(0.4)
    };
    ui.painter().rect_filled(rect, 3., raised());
    let c = rect.center();
    let sign: f32 = if redo { -1. } else { 1. };
    let p = |x, y| c + egui::vec2(x * sign, y);
    ui.painter().add(egui::Shape::line(
        vec![
            p(-8., 4.),
            p(-5., 8.),
            p(3., 8.),
            p(8., 3.),
            p(8., -3.),
            p(3., -8.),
            p(-5., -8.),
            p(-9., -3.),
        ],
        Stroke::new(1.8_f32, color),
    ));
    ui.painter()
        .line_segment([p(-9., -3.), p(-9., -9.)], Stroke::new(1.8_f32, color));
    ui.painter()
        .line_segment([p(-9., -3.), p(-3., -3.)], Stroke::new(1.8_f32, color));
    response
}

pub fn style() -> Style {
    let mut style = Style {
        visuals: Visuals::dark(),
        ..Default::default()
    };
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(13.));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(13.));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(16.));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(11.));
    style.spacing.item_spacing = Vec2::new(5., 5.);
    style.spacing.button_padding = Vec2::new(7., 5.);
    style.spacing.interact_size = Vec2::new(32., 32.);
    style.spacing.scroll_bar_width = 6.;
    style.spacing.scroll_bar_inner_margin = 3.;
    style.visuals.override_text_color = Some(text());
    style.visuals.extreme_bg_color = base();
    style.visuals.faint_bg_color = blend(panel(), raised(), 0.5);
    style.visuals.code_bg_color = raised();
    style.visuals.hyperlink_color = accent();
    style.visuals.panel_fill = panel();
    style.visuals.window_fill = panel();
    style.visuals.window_rounding = Rounding::same(5.);
    style.visuals.window_stroke = Stroke::new(1_f32, border());
    style.visuals.widgets.noninteractive.bg_fill = panel();
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1_f32, border());
    style.visuals.widgets.inactive.bg_fill = raised();
    style.visuals.widgets.inactive.weak_bg_fill = raised();
    style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    style.visuals.widgets.inactive.rounding = Rounding::same(3.);
    style.visuals.widgets.hovered.bg_fill = blend(raised(), accent(), 0.14);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1_f32, accent());
    style.visuals.widgets.hovered.rounding = Rounding::same(3.);
    style.visuals.widgets.active.bg_fill = blend(raised(), accent(), 0.30);
    style.visuals.widgets.active.rounding = Rounding::same(3.);
    style.visuals.selection.bg_fill = blend(raised(), accent(), 0.27);
    style.visuals.selection.stroke = Stroke::new(1_f32, accent());
    for widget in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.fg_stroke = Stroke::new(1_f32, text());
    }
    style.visuals.widgets.open.bg_fill = raised();
    style.visuals.widgets.open.weak_bg_fill = raised();
    style.visuals.widgets.open.bg_stroke = Stroke::new(1_f32, accent());
    style
}

fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    Color32::from_rgb(
        (a.r() as f32 * (1. - t) + b.r() as f32 * t) as u8,
        (a.g() as f32 * (1. - t) + b.g() as f32 * t) as u8,
        (a.b() as f32 * (1. - t) + b.b() as f32 * t) as u8,
    )
}
