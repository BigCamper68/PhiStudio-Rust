mod animation;
mod editing;
mod layout;
mod navigation;
mod transport;
use super::*;

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
