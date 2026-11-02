use super::*;
use phistudio_prpr::{SkinStyle, SkinTexture};
fn style() -> SkinStyle {
    let texture = SkinTexture {
        id: 77,
        size: [100., 20.],
        uv: [0.1, 0.2, 0.6, 0.8],
    };
    SkinStyle {
        width_scale: 1.,
        tap: texture,
        hold: texture,
        flick: texture,
        drag: texture,
        head: [0., 0., 1., 0.1],
        body: [0., 0.1, 1., 0.9],
        tail: [0., 0.9, 1., 1.],
        body_texture: None,
        body_atlas: None,
    }
}
fn grid() -> Grid {
    Grid {
        rect: Rect::from_min_max(pos2(0., 0.), pos2(800., 600.)),
        event_rect: Rect::NOTHING,
        beat: 0.,
        zoom: 96.,
        division: 4,
        lanes: 11,
        snap_x_enabled: false,
        scroll_speed: 1.,
        note_width: 46.,
        camera_x: 0.,
    }
}
#[test]
fn dense_mixed_notes_form_one_ordered_mesh_and_split_safely_for_mobile() {
    let ctx = egui::Context::default();
    let skin = style();
    let grid = grid();
    let out = ctx.run(
        egui::RawInput {
            screen_rect: Some(grid.rect),
            ..Default::default()
        },
        |ctx| {
            let p = ctx.layer_painter(egui::LayerId::background());
            let mut batch = NoteBatch::new(&p);
            for i in 0..19000 {
                let start = phistudio_core::Beat::new(1, 0, 1).unwrap();
                let note = Note {
                    kind: [
                        NoteKind::Tap,
                        NoteKind::Drag,
                        NoteKind::Flick,
                        NoteKind::Hold,
                    ][i % 4],
                    start_time: start,
                    end_time: start.checked_add(start).unwrap(),
                    x: (i % 100) as f64,
                    ..Note::default()
                };
                draw_note(&mut batch, &grid, &note, false, false, Some(&skin));
            }
            batch.flush();
        },
    );
    assert_eq!(out.shapes.len(), 1);
    let egui::Shape::Mesh(mesh) = &out.shapes[0].1 else {
        panic!("Expected a note mesh")
    };
    assert_eq!(mesh.texture_id, egui::TextureId::User(77));
    assert_eq!(mesh.vertices.len(), 19000 * 6);
    // Input notes remain in paint order, rather than being sorted by type/texture.
    assert_eq!(mesh.vertices[0].pos.x, grid.x(0.) - 23.);
    assert_eq!(mesh.vertices[4].pos.x, grid.x(1.) - 23.);
    assert_eq!(mesh.vertices[0].uv, pos2(0.1, 0.2));
    assert_eq!(mesh.vertices[3].uv, pos2(0.6, 0.8));
    let primitives = ctx.tessellate(out.shapes);
    assert_eq!(primitives.len(), 1);
    let egui::epaint::Primitive::Mesh(mesh) = primitives.into_iter().next().unwrap().primitive
    else {
        panic!()
    };
    let chunks = mesh.split_to_u16();
    assert!(chunks.len() >= 2);
    assert!(chunks.iter().all(|m| m.is_valid()));
    assert_eq!(
        chunks.iter().map(|m| m.indices.len()).sum::<usize>(),
        19000 * 9
    );
}
#[test]
fn overlays_flush_images_without_changing_overlap_order() {
    let ctx = egui::Context::default();
    let skin = style();
    let grid = grid();
    let out = ctx.run(
        egui::RawInput {
            screen_rect: Some(grid.rect),
            ..Default::default()
        },
        |ctx| {
            let p = ctx.layer_painter(egui::LayerId::background());
            let mut batch = NoteBatch::new(&p);
            let note = Note {
                start_time: phistudio_core::Beat::new(1, 0, 1).unwrap(),
                ..Note::default()
            };
            draw_note(&mut batch, &grid, &note, true, false, Some(&skin));
            draw_note(&mut batch, &grid, &note, false, false, Some(&skin));
            batch.flush();
        },
    );
    let primitives = ctx.tessellate(out.shapes);
    let textures: Vec<_> = primitives
        .iter()
        .filter_map(|p| match &p.primitive {
            egui::epaint::Primitive::Mesh(m) => Some(m.texture_id),
            _ => None,
        })
        .collect();
    assert_eq!(
        textures,
        vec![
            egui::TextureId::User(77),
            egui::TextureId::default(),
            egui::TextureId::User(77)
        ]
    );
}
#[test]
fn repeating_hold_geometry_is_bounded_by_viewport_with_native_fallback() {
    let body = Rect::from_min_max(pos2(10., -10000.), pos2(40., 100.));
    let clip = Rect::from_min_max(pos2(0., 0.), pos2(100., 100.));
    assert_eq!(repeat_span(body, clip, 10.), Some(1000..1010));
    assert!(repeat_span(body, clip, 0.001).is_none());
    assert_eq!(
        repeat_span(body, clip.translate(vec2(100., 0.)), 10.),
        Some(0..0)
    );
    assert!(repeat_span(body, clip, f32::NAN).is_none());
}
#[test]
fn draft_moved_from_outside_viewport_is_painted_without_editing_the_chart() {
    use phistudio_core::{Beat, ChartDocument, Command, tools::Selection};
    let ctx = egui::Context::default();
    let mut app = Workbench::new(ChartDocument::empty("Draft", 120.).unwrap(), None);
    app.execute(Command::AddNote {
        line: 0,
        note: Note {
            start_time: Beat::new(1000, 0, 1).unwrap(),
            end_time: Beat::new(1000, 0, 1).unwrap(),
            ..Note::default()
        },
    });
    app.skin = Some(phistudio_prpr::EditorSkin {
        name: "Fixture".into(),
        author: String::new(),
        normal: style(),
        highlighted: style(),
        grid: None,
    });
    let render = |app: &mut Workbench| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1000., 700.))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| show(app, ui, false, false));
            },
        )
    };
    let first = render(&mut app);
    assert!(
        !first
            .shapes
            .iter()
            .any(|s| matches!(&s.1,Shape::Mesh(m) if m.texture_id==egui::TextureId::User(77)))
    );
    let grid = grid_input::geometry(app.timeline.area, &app);
    let selection = Selection {
        notes: vec![NoteId { line: 0, index: 0 }],
        events: vec![],
    };
    app.timeline.drag = Some(Gesture::SelectionMove {
        selection,
        stories: vec![],
        origin: pos2(0., 0.),
        grid,
        beats: Beat::new(-998, 0, 1).unwrap(),
        x: 0.,
        earliest: Beat::new(1000, 0, 1).unwrap(),
        x_range: [0., 0.],
        note_surface: true,
        moved: true,
    });
    let before = app.editor.document().as_json().clone();
    let revision = app.editor.revision();
    let draft = render(&mut app);
    assert!(
        draft
            .shapes
            .iter()
            .any(|s| matches!(&s.1,Shape::Mesh(m) if m.texture_id==egui::TextureId::User(77)))
    );
    assert_eq!(app.editor.revision(), revision);
    assert_eq!(app.editor.document().as_json(), &before);
    app.timeline.drag = None;
    assert!(
        !render(&mut app)
            .shapes
            .iter()
            .any(|s| matches!(&s.1,Shape::Mesh(m) if m.texture_id==egui::TextureId::User(77)))
    );
}
