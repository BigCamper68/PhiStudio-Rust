use super::*;

pub(super) fn show_keyboard(show: bool) {
    #[cfg(target_os = "ios")]
    crate::ios::show_keyboard(show);
    #[cfg(not(target_os = "ios"))]
    unsafe { get_internal_gl() }
        .quad_context
        .show_keyboard(show);
}

pub(super) fn keyboard_text_ready(ctx: &egui::Context, cursor: bool, visible: bool) -> bool {
    let text = ctx
        .memory(|m| m.focus())
        .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
    // IME resize can clip the caret while the same field still owns input.
    // Keep an open keyboard alive; only a completed tap can open a closed one.
    text && (cursor || visible)
}

pub(super) fn chart_clipboard_shortcut(
    key: mq::KeyCode,
    mods: mq::KeyMods,
    text_input: bool,
) -> Option<egui::Event> {
    let command = if cfg!(target_os = "macos") {
        mods.logo
    } else {
        mods.ctrl
    };
    if text_input || !command {
        return None;
    }
    let key = match key {
        mq::KeyCode::C => egui::Key::C,
        mq::KeyCode::X => egui::Key::X,
        mq::KeyCode::V => egui::Key::V,
        _ => return None,
    };
    Some(egui::Event::Key {
        key,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers {
            alt: mods.alt,
            ctrl: mods.ctrl,
            shift: mods.shift,
            mac_cmd: mods.logo && cfg!(target_os = "macos"),
            command,
        },
    })
}

pub(super) fn collapse_focus_selection(
    ctx: &egui::Context,
    tap: bool,
    previous: &mut Option<egui::Id>,
) {
    let focus = ctx.memory(|m| m.focus());
    if tap
        && focus != *previous
        && let Some(id) = focus
        && let Some(mut state) = egui::TextEdit::load_state(ctx, id)
    {
        if let Some(range) = state.ccursor_range() {
            let caret = if range.primary.index >= range.secondary.index {
                range.primary
            } else {
                range.secondary
            };
            state.set_ccursor_range(Some(egui::text::CCursorRange::one(caret)));
            state.store(ctx, id);
        }
        *previous = focus;
    } else if focus != *previous {
        *previous = None;
    }
}
