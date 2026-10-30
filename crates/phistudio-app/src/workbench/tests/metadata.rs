use super::*;

#[test]
fn chart_properties_apply_offset_as_integer_with_one_undo() {
    let mut app = Workbench::new(ChartDocument::empty("Offset", 120.).unwrap(), None);
    let original = app.editor.document().as_json().clone();
    app.open_chart_properties();
    app.advanced.metadata.as_mut().unwrap().1.offset_ms = 150.;
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    let render = |app: &mut Workbench, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(873., 393.),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.ui(ctx, false),
        )
    };
    for _ in 0..5 {
        render(&mut app, vec![]);
        let zones = ctx
            .data_mut(|d| {
                d.get_temp::<Vec<crate::scroll::Zone>>(egui::Id::new("phistudio-scroll-zones"))
            })
            .unwrap();
        for z in zones
            .into_iter()
            .filter(|z| z.layer != egui::LayerId::background() && z.axis == 1)
        {
            let mut state = egui::scroll_area::State::load(&ctx, z.id).unwrap();
            state.offset.y = z.max_offset;
            state.store(&ctx, z.id);
        }
    }
    let out = render(&mut app, vec![]);
    let point = out
        .shapes
        .iter()
        .find_map(|s| match &s.1 {
            egui::Shape::Text(t) if t.galley.job.text == "Apply chart properties" => {
                let p = t.pos + t.galley.size() * 0.5;
                s.0.contains(p).then_some(p)
            }
            _ => None,
        })
        .expect("Reachable Apply chart properties");
    for pressed in [true, false] {
        render(
            &mut app,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    pressed,
                    button: egui::PointerButton::Primary,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.advanced.metadata.is_none());
    assert_eq!(
        app.editor.document().as_json()["META"]["offset"].as_i64(),
        Some(150)
    );
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &original);
    app.history_step(true);
    assert_eq!(app.editor.document().offset_seconds(), 0.15);
}
