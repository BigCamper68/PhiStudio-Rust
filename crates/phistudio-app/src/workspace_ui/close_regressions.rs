use super::*;
fn render(app: &mut Workbench, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(873., 393.),
            )),
            events,
            ..Default::default()
        },
        |ctx| app.ui(ctx, false),
    )
}
fn pointer(p: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}
#[test]
fn closing_empty_resources_with_the_titlebar_restores_file_menu_interaction() {
    let mut app = Workbench::new(ChartDocument::empty("Empty", 120.).unwrap(), None);
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::fonts::definitions());
    app.workspace.view = Some(View::Resources);
    for _ in 0..4 {
        render(&mut app, &ctx, vec![]);
    }
    let rect = app.workspace.rect;
    let at = egui::pos2(rect.right() - 14., rect.top() + 16.);
    render(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(at), pointer(at, true)],
    );
    render(&mut app, &ctx, vec![pointer(at, false)]);
    assert!(
        app.workspace.view.is_none(),
        "Close failed at {at:?} in {rect:?}"
    );
    assert!(!app.modal_open());
    for _ in 0..4 {
        render(&mut app, &ctx, vec![]);
    }
    let output = render(&mut app, &ctx, vec![]);
    let file = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.1 {
            egui::Shape::Text(t) if t.galley.job.text == "File" => {
                Some(t.pos + t.galley.size() * 0.5)
            }
            _ => None,
        })
        .unwrap();
    render(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(file), pointer(file, true)],
    );
    render(&mut app, &ctx, vec![pointer(file, false)]);
    let output = render(&mut app, &ctx, vec![]);
    let texts = output
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::Shape::Text(t) = &s.1 {
                Some(t.galley.job.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        texts.contains(&"Export MP4…"),
        "File is not clickable after resource close: {texts:?}"
    );
}
