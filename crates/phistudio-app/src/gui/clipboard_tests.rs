use super::*;
#[test]
fn object_shortcuts_reach_the_grid_without_interfering_with_text_fields() {
    let mods = mq::KeyMods {
        ctrl: true,
        logo: true,
        shift: true,
        ..Default::default()
    };
    for key in [mq::KeyCode::C, mq::KeyCode::X, mq::KeyCode::V] {
        assert!(matches!(
            chart_clipboard_shortcut(key, mods, false),
            Some(egui::Event::Key {
                pressed: true,
                modifiers: egui::Modifiers {
                    command: true,
                    shift: true,
                    ..
                },
                ..
            })
        ));
        assert!(chart_clipboard_shortcut(key, mods, true).is_none());
    }
    assert!(chart_clipboard_shortcut(mq::KeyCode::V, mq::KeyMods::default(), false).is_none());
}
