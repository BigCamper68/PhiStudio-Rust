use super::*;
use crate::workbench::Workbench;
use phistudio_core::{ChartDocument, EditorPreferences, Language};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
fn release() -> Release {
    Release {
        version: semver::Version::new(99, 0, 0),
        url: format!("{}/tag/v99.0.0", service::RELEASES),
        name: "99.0.0".into(),
        body: "- Improved the editor.".into(),
    }
}
fn app() -> Workbench {
    Workbench::new(ChartDocument::empty("Updates", 120.).unwrap(), None)
}
fn complete(state: &mut State) {
    let result = state
        .pending
        .take()
        .unwrap()
        .receiver
        .recv_timeout(Duration::from_secs(3))
        .unwrap();
    state.finish(result);
}
#[test]
fn only_one_check_runs_and_startup_is_not_repeated() {
    let mut state = State::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let (finish, wait) = mpsc::channel();
    state.start_with(true, move || {
        count.fetch_add(1, Ordering::Relaxed);
        wait.recv().unwrap();
        Ok(release())
    });
    state.tick(true);
    // A second manual request must not start another worker.
    state.start_with(false, || panic!("duplicate request"));
    finish.send(()).unwrap();
    if state.pending.is_some() {
        complete(&mut state);
    }
    state.tick(true);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(state.notification().is_some());
    state.dismissed = true;
    state.tick(true);
    assert!(state.notification().is_none());
}
#[test]
fn disabling_startup_checks_cancels_automatic_results_but_manual_checks_work() {
    let (sender, receiver) = mpsc::channel();
    let mut state = State {
        pending: Some(Pending {
            receiver,
            automatic: true,
        }),
        startup_checked: true,
        status: Status::Checking,
        ..Default::default()
    };
    state.poll(false);
    assert!(sender.send(Ok(release())).is_err());
    assert_eq!(state.status, Status::Idle);
    state.start_with(false, || Ok(release()));
    complete(&mut state);
    assert!(matches!(state.status, Status::Available(_)));
}
#[test]
fn failed_and_current_checks_never_show_a_new_version_notification() {
    let mut state = State::default();
    state.finish(Err(CheckError::Connection));
    assert_eq!(state.status, Status::Failed(CheckError::Connection));
    assert!(state.notification().is_none());
    let mut current = release();
    current.version = semver::Version::parse(service::installed_version()).unwrap();
    state.finish(Ok(current));
    assert!(matches!(state.status, Status::Current(_)));
    assert!(state.notification().is_none());
}
#[test]
fn update_preferences_and_pending_results_survive_project_switches_without_editing_charts() {
    let mut app = app();
    let mut preferences: EditorPreferences = serde_json::from_str("{}").unwrap();
    assert!(preferences.options.check_updates_on_start);
    preferences.options.check_updates_on_start = false;
    let root = tempfile::tempdir().unwrap();
    phistudio_core::save_editor_preferences(root.path(), &preferences).unwrap();
    app.apply_preferences(&phistudio_core::load_editor_preferences(root.path()).unwrap());
    let (sender, receiver) = mpsc::channel();
    app.updates.pending = Some(Pending {
        receiver,
        automatic: false,
    });
    app.updates.startup_checked = true;
    app.replace_document(ChartDocument::empty("Next", 130.).unwrap(), None, None);
    assert!(!app.options.check_updates_on_start);
    sender.send(Ok(release())).unwrap();
    app.updates.poll(false);
    assert!(app.updates.notification().is_some());
    assert!(!app.editor.is_dirty());
    assert!(!app.editor.can_undo());
}
#[test]
fn update_settings_fit_a_phone_and_the_banner_does_not_interrupt_playback() {
    for language in [Language::English, Language::Russian] {
        let ctx = egui::Context::default();
        let mut app = app();
        app.options.language = language;
        app.updates.finish(Ok(release()));
        app.transport.playing = true;
        let document = app.editor.document().as_json().clone();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(320., 720.),
                )),
                ..Default::default()
            },
            |ctx| {
                let _language = crate::i18n::Scope::enter(language);
                app.update_banner(ctx);
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.style_mut().wrap = Some(true);
                    let available = ui.available_width();
                    app.updates_ui(ui);
                    assert!(ui.min_rect().width() <= available + 1.);
                });
            },
        );
        assert!(app.transport.playing);
        assert!(!app.editor.is_dirty());
        assert_eq!(app.editor.document().as_json(), &document);
    }
}

#[test]
fn first_install_and_upgrade_show_notes_once_and_save_acknowledgement() {
    for seen in [None, Some("0.0.1".into())] {
        let mut app = app();
        app.options.release_notes_seen = seen;
        let mut current = release();
        current.version = semver::Version::parse(service::installed_version()).unwrap();
        app.updates.notes.content = notes::Content::Ready(current);
        app.managed_storage = true;
        app.poll_release_notes();
        assert!(app.updates.notes.requested);
        let before = app.editor.document().as_json().clone();
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.ui(ctx, false));
        assert!(app.release_notes_open());
        assert!(app.modal_open());
        app.back();
        assert!(!app.release_notes_open());
        assert_eq!(
            app.options.release_notes_seen.as_deref(),
            Some(service::installed_build())
        );
        assert_eq!(app.editor.document().as_json(), &before);
        assert!(!app.editor.is_dirty());
        let root = tempfile::tempdir().unwrap();
        phistudio_core::save_editor_preferences(root.path(), &app.preferences()).unwrap();
        let mut next = super::tests::app();
        next.apply_preferences(&phistudio_core::load_editor_preferences(root.path()).unwrap());
        next.managed_storage = true;
        next.poll_release_notes();
        assert!(!next.updates.notes.requested);
        assert!(!next.release_notes_open());
    }
}
#[test]
fn release_notes_wait_for_recovery_and_offline_close_does_not_acknowledge() {
    let mut app = app();
    app.updates.notes.content = notes::Content::Failed(CheckError::Connection);
    app.updates.notes.requested = true;
    app.files.busy = true;
    let ctx = egui::Context::default();
    let _ = ctx.run(Default::default(), |ctx| app.release_notes_window(ctx));
    assert!(!app.release_notes_open());
    assert!(app.updates.notes.requested);
    app.files.busy = false;
    let _ = ctx.run(Default::default(), |ctx| app.release_notes_window(ctx));
    assert!(app.release_notes_open());
    app.back();
    assert_eq!(app.options.release_notes_seen, None);
}
#[test]
fn about_cards_fit_phone_width_and_load_the_supplied_avatar() {
    let image = about::decode_avatar().unwrap();
    assert_eq!(image.size, [128, 128]);
    for language in [Language::English, Language::Russian] {
        let mut app = app();
        let mut current = release();
        current.version = semver::Version::parse(service::installed_version()).unwrap();
        app.updates.notes.content = notes::Content::Ready(current);
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(320., 720.),
                )),
                ..Default::default()
            },
            |ctx| {
                let _language = crate::i18n::Scope::enter(language);
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.style_mut().wrap = Some(true);
                    let width = ui.available_width();
                    app.about_ui(ui);
                    assert!(ui.min_rect().width() <= width + 1.);
                });
            },
        );
        assert!(!app.editor.is_dirty());
    }
}
