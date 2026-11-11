use super::*;
use phistudio_core::{ChartDocument, Command, Note};
fn app() -> Workbench {
    let mut raw = ChartDocument::empty("Trace", 120.)
        .unwrap()
        .as_json()
        .clone();
    raw["judgeLineList"][0]["notes"] = serde_json::json!([Note {
        start_time: "4".parse().unwrap(),
        end_time: "4".parse().unwrap(),
        ..Default::default()
    }]);
    let mut app = Workbench::new(ChartDocument::from_value(raw).unwrap(), None);
    app.select(Some(NoteId { line: 0, index: 0 }));
    app.select_trace_source();
    app
}
#[test]
fn source_range_and_cache_key_follow_native_inputs_and_project_ownership() {
    let mut app = app();
    let key = app.tracing_key().unwrap();
    assert_eq!(key.spec.range, [Beat::ZERO, "4".parse().unwrap()]);
    app.asset_revision += 1;
    assert_ne!(key, app.tracing_key().unwrap());
    app.asset_revision -= 1;
    app.project_options.use_rpe_170_speed = !app.project_options.use_rpe_170_speed;
    assert_ne!(key, app.tracing_key().unwrap());
    app.note_tracing.source = Some((app.session + 1, key.spec.source));
    assert!(app.tracing_key().is_err());
}
#[test]
fn changed_chart_and_fields_cannot_apply_prepared_events() {
    for edit_chart in [true, false] {
        let mut app = app();
        let key = app.tracing_key().unwrap();
        let points = [
            TracePoint {
                seconds: 0.,
                values: [0.; 3],
                break_before: false,
            },
            TracePoint {
                seconds: 2.,
                values: [1.; 3],
                break_before: false,
            },
        ];
        let edit = Snapshot::capture(app.editor.document(), key.spec.clone())
            .unwrap()
            .prepare(key.revision, &points, |_, e, _, _| Ok(e.clone()), || false)
            .unwrap();
        app.note_tracing.prepared = Some((
            key,
            Prepared {
                edit: Ok(edit),
                path: vec![],
                points: vec![],
                samples: 2,
            },
        ));
        if edit_chart {
            app.editor
                .execute(Command::RenameChart("Changed".into()))
                .unwrap();
        } else {
            app.note_tracing.layer = 3;
        }
        let before = app.editor.document().as_json().clone();
        let revision = app.editor.revision();
        app.apply_note_trace_preview();
        assert_eq!(app.editor.document().as_json(), &before);
        assert_eq!(app.editor.revision(), revision);
        assert!(app.note_tracing.error.is_some());
    }
}
#[test]
fn cancellation_signals_the_worker_and_closing_state_cancels_too() {
    let mut app = app();
    app.begin_note_trace();
    let cancel = app.note_tracing.pending.as_ref().unwrap().cancel.clone();
    app.note_tracing.cancel();
    assert!(cancel.load(Ordering::Relaxed));
    app.begin_note_trace();
    let cancel = app.note_tracing.pending.as_ref().unwrap().cancel.clone();
    drop(app);
    assert!(cancel.load(Ordering::Relaxed));
}
#[test]
fn line_creation_settings_invalidate_only_new_destination_previews() {
    let mut app = app();
    let before = app.tracing_key().unwrap();
    app.options.new_line_defaults.x = 123.;
    assert_ne!(before, app.tracing_key().unwrap());
    app.note_tracing.destination = Some(0);
    let before = app.tracing_key().unwrap();
    app.options.new_line_defaults.y = 456.;
    assert_eq!(before, app.tracing_key().unwrap());
}
