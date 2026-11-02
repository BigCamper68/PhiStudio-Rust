mod animation;
mod editing;
mod layout;
mod navigation;
mod paired;
mod transport;
use super::*;
use crate::shader_ui::layout as surface;

use egui::pos2;

use phistudio_core::ChartDocument;

fn app() -> Workbench {
    let mut app = Workbench::new(
        ChartDocument::empty("Shader workspace", 120.).unwrap(),
        None,
    );
    app.advanced.right_mode = RightMode::Shaders;
    app.set_tool(crate::workbench::Tool::Select);
    app.refresh_shaders();
    app
}

fn frame(
    app: &mut Workbench,
    ctx: &egui::Context,
    size: egui::Vec2,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ctx| app.ui(ctx, false),
    )
}

fn text_position(out: &egui::FullOutput, text: &str) -> egui::Pos2 {
    out.shapes
        .iter()
        .find_map(|s| match &s.1 {
            egui::Shape::Text(t) if t.galley.job.text == text => {
                Some(t.pos + t.galley.size() * 0.5)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing UI text: {text}"))
}

fn click(app: &mut Workbench, ctx: &egui::Context, size: egui::Vec2, pos: egui::Pos2) {
    frame(
        app,
        ctx,
        size,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
    );
    frame(
        app,
        ctx,
        size,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    );
}

fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}

fn effect_x(app: &Workbench, grid: crate::grid_input::Grid, index: usize) -> f32 {
    surface::column(
        app,
        grid,
        false,
        surface::lane(&app.shaders.source["effects"][index], index),
    )
    .center()
    .x
}
fn parameter_x(app: &Workbench, grid: crate::grid_input::Grid, name: &str) -> f32 {
    surface::parameter_column(app, grid.events(), name)
        .unwrap()
        .center()
        .x
}
fn choose_page(app: &mut Workbench, ctx: &egui::Context, size: egui::Vec2, page: &str) {
    let label = match app.shaders.page {
        ShaderPage::Effect => "Effect",
        ShaderPage::Parameters => "Parameters",
        ShaderPage::Animation => "Animation",
        ShaderPage::Tempo => "FX BPM",
    };
    app.properties_scroll = true;
    for _ in 0..20 {
        frame(app, ctx, size, vec![]);
    }
    let out = frame(app, ctx, size, vec![]);
    click(app, ctx, size, text_position(&out, label));
    let out = frame(app, ctx, size, vec![]);
    click(app, ctx, size, text_position(&out, page));
}
