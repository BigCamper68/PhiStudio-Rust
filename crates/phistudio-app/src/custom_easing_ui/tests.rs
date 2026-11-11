use super::*;
use egui::{Event, PointerButton, Pos2, TouchPhase};
use phistudio_core::{ChartDocument, Command, EventType};

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}
fn touch(id: u64, phase: TouchPhase, pos: Pos2) -> Event {
    Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(id),
        phase,
        pos,
        force: 0.,
    }
}
fn canvas(ctx: &egui::Context, state: &mut State, events: Vec<Event>) -> graph::View {
    let mut rect = egui::Rect::NOTHING;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(600., 480.),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| graph::canvas(ui, state, 400.));
            rect = ctx
                .data_mut(|d| d.get_temp(egui::Id::new("custom-easing-plot")))
                .unwrap();
        },
    );
    graph::View {
        rect,
        bounds: state.view,
    }
}
fn editor() -> (Workbench, DraftKey) {
    let doc = ChartDocument::empty("Curve UI", 120.).unwrap();
    let app = Workbench::new(doc, None);
    let id = EventId {
        line: 0,
        layer: 0,
        kind: EventType::MoveX,
        index: 0,
    };
    let event = app.editor.document().event(id).unwrap();
    let key = DraftKey {
        session: app.session,
        revision: app.editor.revision(),
        id,
        times: [event.start_time.to_string(), event.end_time.to_string()],
        event,
    };
    (app, key)
}

#[test]
fn ordered_tap_drag_and_local_history_do_not_mutate_the_chart() {
    let (mut app, key) = editor();
    let original = app.editor.document().as_json().clone();
    app.open_custom_easing(key);
    let ctx = egui::Context::default();
    let mut view = canvas(&ctx, &mut app.custom_easing, vec![]);
    app.custom_easing.tool = Tool::Add;
    let add = view.point([0.4, 0.9]);
    canvas(
        &ctx,
        &mut app.custom_easing,
        vec![button(add, true), button(add, false)],
    );
    assert_eq!(app.custom_easing.draft.curve.nodes.len(), 3);
    let added = app.custom_easing.draft.curve.clone();
    assert!(
        (added.nodes[1].y - 0.4).abs() < 1e-5,
        "Insertion preserves the original shape"
    );
    assert_eq!(app.custom_easing.undo.len(), 1);
    app.custom_easing.tool = Tool::Edit;
    view.bounds = app.custom_easing.view;
    let down = view.point(added.nodes[1].point());
    let up = view.point([0.6, 0.8]);
    canvas(&ctx, &mut app.custom_easing, vec![button(down, true)]);
    canvas(&ctx, &mut app.custom_easing, vec![Event::PointerMoved(up)]);
    canvas(&ctx, &mut app.custom_easing, vec![button(up, false)]);
    let moved = app.custom_easing.draft.curve.clone();
    assert!((moved.nodes[1].x - 0.6).abs() < 1e-5);
    assert!((moved.nodes[1].y - 0.8).abs() < 1e-5);
    assert_eq!(app.custom_easing.undo.len(), 2);
    app.custom_easing.history(false);
    assert_eq!(app.custom_easing.draft.curve, added);
    app.custom_easing.history(true);
    assert_eq!(app.custom_easing.draft.curve, moved);
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(!app.editor.is_dirty());
}

#[test]
fn pinch_restores_uncommitted_drag_and_retains_ownership_through_pointer_cancel() {
    let ctx = egui::Context::default();
    let mut state = State::default();
    state.insert(0.4);
    state.undo.clear();
    let view = canvas(&ctx, &mut state, vec![]);
    let before = state.draft.curve.clone();
    let a = view.point([0.4, 0.4]);
    let b = view.point([0.8, 0.7]);
    canvas(
        &ctx,
        &mut state,
        vec![touch(1, TouchPhase::Start, a), button(a, true)],
    );
    let moved = a + egui::vec2(20., -20.);
    canvas(
        &ctx,
        &mut state,
        vec![
            touch(1, TouchPhase::Move, moved),
            Event::PointerMoved(moved),
        ],
    );
    assert_ne!(state.draft.curve, before);
    canvas(
        &ctx,
        &mut state,
        vec![touch(2, TouchPhase::Start, b), Event::PointerGone],
    );
    assert_eq!(state.draft.curve, before);
    assert!(state.undo.is_empty());
    let bounds = state.view;
    canvas(
        &ctx,
        &mut state,
        vec![touch(2, TouchPhase::Move, b + egui::vec2(60., -20.))],
    );
    assert_ne!(state.view, bounds);
    canvas(
        &ctx,
        &mut state,
        vec![touch(2, TouchPhase::End, b), Event::PointerGone],
    );
    assert!(state.navigation.owned);
    canvas(
        &ctx,
        &mut state,
        vec![Event::PointerMoved(a), button(a, false)],
    );
    assert_eq!(state.draft.curve, before);
    canvas(&ctx, &mut state, vec![touch(1, TouchPhase::End, a)]);
    assert!(!state.navigation.owned);
    assert!(state.undo.is_empty());
}

#[test]
fn pointer_cancellation_restores_curve_and_keeps_endpoint_positions_locked() {
    let ctx = egui::Context::default();
    let mut state = State::default();
    let view = canvas(&ctx, &mut state, vec![]);
    let p = view.point([0., 0.]);
    canvas(
        &ctx,
        &mut state,
        vec![
            button(p, true),
            Event::PointerMoved(p + egui::vec2(30., -20.)),
            button(p + egui::vec2(30., -20.), false),
        ],
    );
    assert_eq!(state.draft.curve.nodes[0].point(), [0., 0.]);
    assert!(state.undo.is_empty());
    state.insert(0.4);
    let before = state.draft.curve.clone();
    let p = view.point(before.nodes[1].point());
    canvas(
        &ctx,
        &mut state,
        vec![
            button(p, true),
            Event::PointerMoved(p + egui::vec2(20., -30.)),
        ],
    );
    assert_ne!(state.draft.curve, before);
    canvas(&ctx, &mut state, vec![Event::PointerGone]);
    assert_eq!(state.draft.curve, before);
    assert_eq!(state.undo.len(), 1);
}

#[test]
fn worker_returns_one_logical_draft_and_owner_apply_has_exact_history() {
    let (mut app, mut key) = editor();
    key.event.end = 100.;
    let before = app.editor.document().as_json().clone();
    app.open_custom_easing(key.clone());
    app.custom_easing.insert(0.4);
    app.custom_easing
        .edit(app.custom_easing.draft.curve.move_nodes(&[1], 0., 0.8));
    app.custom_easing.prepare();
    for _ in 0..500 {
        app.custom_easing.poll();
        if !app.custom_easing.open {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(
        app.custom_easing.error.is_none(),
        "{:?}",
        app.custom_easing.error
    );
    let event = app.custom_easing.take_completed(&key).unwrap();
    assert_eq!(event.custom_easing.as_ref().unwrap().curve.nodes.len(), 3);
    assert_eq!(event.end, 100.);
    assert_eq!(app.editor.document().as_json(), &before);
    assert!(app.commit_authoring(Command::UpdateEvent { id: key.id, event }));
    let after = app.editor.document().as_json().clone();
    assert_eq!(
        after["judgeLineList"][0]["eventLayers"][0]["moveXEvents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(app.editor.undo());
    assert_eq!(app.editor.document().as_json(), &before);
    assert!(app.editor.redo());
    assert_eq!(app.editor.document().as_json(), &after);
}

#[test]
fn stale_project_and_draft_results_are_rejected() {
    let (mut app, key) = editor();
    app.open_custom_easing(key.clone());
    app.custom_easing.prepare();
    app.session += 1;
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        app.custom_easing_window(ctx)
    });
    assert!(!app.custom_easing.open);
    assert!(app.custom_easing.pending.is_none());
    assert!(app.custom_easing.completed.is_none());
    app.custom_easing.completed = Some((key.clone(), key.event.clone()));
    let mut other = key;
    other.times[1] = "9".into();
    assert!(app.custom_easing.take_completed(&other).is_none());
}

#[test]
fn graph_modal_does_not_repeatedly_cancel_the_platform_pointer() {
    let (mut app, key) = editor();
    let mut input = crate::runtime::ModalInput::default();
    assert!(!input.update(&app));
    app.open_custom_easing(key);
    assert!(app.modal_open());
    assert!(!input.update(&app));
    assert!(!input.update(&app));
    app.close_requested = true;
    assert!(input.update(&app));
    assert!(!input.update(&app));
}

#[test]
fn handles_move_locally_without_moving_locked_endpoints() {
    let ctx = egui::Context::default();
    let mut state = State::default();
    let view = canvas(&ctx, &mut state, vec![]);
    let handle = state.draft.curve.materialized().unwrap().nodes[0]
        .outgoing
        .unwrap();
    let p = view.point(handle);
    let up = p + egui::vec2(0., -70.);
    canvas(
        &ctx,
        &mut state,
        vec![button(p, true), Event::PointerMoved(up), button(up, false)],
    );
    assert_eq!(state.draft.curve.nodes[0].point(), [0., 0.]);
    assert_ne!(state.draft.curve.nodes[0].outgoing, None);
    assert!(state.draft.curve.nodes[0].outgoing.unwrap()[1] > handle[1]);
    assert_eq!(state.undo.len(), 1);
}

#[test]
fn cropped_curve_worker_retains_the_source_domain_and_unsaved_properties() {
    use phistudio_core::custom_easing::{Binding, Mode, Node};
    let (mut app, key) = editor();
    let curve = Curve {
        nodes: vec![
            Node::new(0., 0., Mode::Linear),
            Node::new(0.5, 1.5, Mode::Linear),
            Node::new(1., 1., Mode::Linear),
        ],
        ..Default::default()
    };
    let binding = Binding::new(curve).crop([0., 100.], 0.25, 0.75).unwrap();
    let mut event = key.event.clone();
    event.start = binding.scalar_value([0., 100.], 0.).unwrap();
    event.end = binding.scalar_value([0., 100.], 1.).unwrap();
    event.custom_easing = Some(Box::new(binding.clone()));
    assert!(app.commit_authoring(Command::UpdateEvent {
        id: key.id,
        event: event.clone()
    }));
    let key = DraftKey {
        revision: app.editor.revision(),
        event,
        ..key
    };
    let original = app.editor.document().as_json().clone();
    app.open_custom_easing(key.clone());
    app.custom_easing
        .edit(app.custom_easing.draft.curve.move_nodes(&[1], 0., 0.2));
    app.custom_easing.prepare();
    for _ in 0..500 {
        app.custom_easing.poll();
        if !app.custom_easing.open {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let event = app.custom_easing.take_completed(&key).unwrap();
    let next = event.custom_easing.as_ref().unwrap();
    assert_eq!(next.domain, binding.domain);
    assert_eq!(next.source, binding.source);
    assert_eq!(
        event.start,
        next.scalar_value([event.start, event.end], 0.).unwrap()
    );
    assert_eq!(
        event.end,
        next.scalar_value([event.start, event.end], 1.).unwrap()
    );
    assert_ne!(event.start, key.event.start);
    assert_eq!(app.editor.document().as_json(), &original);
}

#[test]
fn invalid_times_and_cancel_never_publish_a_curve_draft() {
    let (mut app, mut key) = editor();
    key.times[0] = "invalid".into();
    app.open_custom_easing(key.clone());
    assert!(!app.custom_easing.open);
    assert!(app.error.is_some());
    key.times[0] = "0".into();
    app.open_custom_easing(key.clone());
    app.custom_easing.insert(0.4);
    app.custom_easing.prepare();
    app.custom_easing.close();
    app.custom_easing.poll();
    assert!(app.custom_easing.take_completed(&key).is_none());
    assert!(!app.editor.is_dirty());
}

#[test]
fn graph_and_precision_views_fit_compact_screens_in_every_locale() {
    for size in [
        egui::vec2(582., 262.),
        egui::vec2(240., 520.),
        egui::vec2(360., 780.),
        egui::vec2(873., 393.),
    ] {
        for language in phistudio_core::Language::ALL {
            let (mut app, key) = editor();
            app.options.language = language;
            app.open_custom_easing(key);
            app.custom_easing.insert(0.4);
            let ctx = egui::Context::default();
            ctx.set_style(crate::theme::style());
            ctx.set_fonts(crate::fonts::definitions());
            let viewport = egui::Rect::from_min_size(Pos2::ZERO, size);
            for details in [false, true] {
                app.custom_easing.details = details;
                for _ in 0..5 {
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(viewport),
                            ..Default::default()
                        },
                        |ctx| app.ui(ctx, false),
                    );
                }
                let window =
                    crate::window::visible_rect(&ctx, egui::Id::new("Custom easing editor"))
                        .unwrap();
                let button: egui::Rect = ctx
                    .data_mut(|d| d.get_temp(egui::Id::new("custom-easing-use")))
                    .unwrap();
                assert!(
                    viewport.expand(2.).contains_rect(window),
                    "{language:?} {size:?} details={details}: window {window:?}"
                );
                assert!(
                    viewport.contains_rect(button),
                    "{language:?} {size:?} details={details}: button {button:?}"
                );
            }
        }
    }
}
