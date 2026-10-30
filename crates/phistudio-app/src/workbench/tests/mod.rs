mod clipboard;
mod events;
mod fullscreen;
mod gesture_scroll;
mod layout;
mod localization;
mod metadata;
mod notes;
mod preview;
mod projects;
mod properties;
mod property_layout;
mod storyboard;
mod transport;
use super::*;

fn frame(
    app: &mut Workbench,
    ctx: &egui::Context,
    size: Vec2,
    events: Vec<egui::Event>,
    canceled: bool,
) {
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ctx| app.ui(ctx, canceled),
    );
}

fn shortcut(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
    [true, false]
        .map(|pressed| egui::Event::Key {
            key,
            pressed,
            repeat: false,
            modifiers,
        })
        .into()
}

fn test_grid(app: &Workbench) -> crate::grid_input::Grid {
    crate::grid_input::geometry(
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(700., 400.)),
        app,
    )
}

fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn grid_events(app: &mut Workbench, events: &[egui::Event], canceled: bool) {
    let grid = test_grid(app);
    crate::grid_input::handle(
        app,
        events,
        grid,
        egui::Rect::EVERYTHING,
        |_| true,
        canceled,
    );
}

fn seek_pointer_button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        pressed,
        button: egui::PointerButton::Primary,
        modifiers: egui::Modifiers::NONE,
    }
}

fn group_app() -> Workbench {
    let mut app = Workbench::new(ChartDocument::empty("Group gestures", 120.).unwrap(), None);
    for (x, start) in [(-270., 1), (135., 2), (540., 4)] {
        app.execute(Command::AddNote {
            line: 0,
            note: Note {
                x,
                start_time: Beat::new(start, 0, 1).unwrap(),
                end_time: Beat::new(start, 0, 1).unwrap(),
                ..Default::default()
            },
        });
    }
    app
}

fn group_input(
    app: &mut Workbench,
    grid: crate::grid_input::Grid,
    events: Vec<egui::Event>,
    canceled: bool,
) {
    crate::grid_input::handle(app, &events, grid, grid.full_rect(), |_| true, canceled);
}

mod formulas;
