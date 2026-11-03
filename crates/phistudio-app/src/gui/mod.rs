//! Owned egui integration for prpr's Miniquad version. Input and rendering use
//! the same context as prpr; a canceled Android touch always releases capture.
mod keyboard;
use keyboard::{
    chart_clipboard_shortcut, collapse_focus_selection, keyboard_text_ready, show_keyboard,
};
mod density;
use density::{display_density, scaled_display_density};
use egui_miniquad::EguiMq;
use macroquad::{input::utils, miniquad as mq, prelude::get_internal_gl};

// Exercise the exact adapter's CPU batching in the workspace suite.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../../vendor/egui-miniquad/src/batch.rs"]
mod render_batch_tests;

pub struct Gui {
    inner: EguiMq,
    subscriber: usize,
    touch: crate::touch::TouchRouter,
    canceled: bool,
    keyboard: ImeGate,
    text_focus: Option<egui::Id>,
}
impl Gui {
    pub fn new() -> Self {
        macroquad::input::simulate_mouse_with_touch(false);
        let inner = EguiMq::new(unsafe { get_internal_gl() }.quad_context);
        inner.egui_ctx().set_fonts(crate::fonts::definitions());
        inner.egui_ctx().set_style(super::theme::style());
        Self {
            inner,
            subscriber: utils::register_input_subscriber(),
            touch: crate::touch::TouchRouter::default(),
            canceled: false,
            keyboard: ImeGate::default(),
            text_focus: None,
        }
    }
    pub fn set_ui_scale(&mut self, scale: f32) {
        self.inner
            .set_pixels_per_point(scaled_display_density(display_density(), scale));
    }
    pub fn run(&mut self, f: impl FnOnce(&egui::Context, bool)) {
        #[cfg(target_os = "ios")]
        {
            self.inner
                .set_viewport_insets(crate::ios::viewport_insets());
            for event in crate::ios::input_events() {
                self.inner.push_input_event(event);
            }
        }
        for event in self.touch.frame_start() {
            self.inner.push_input_event(event);
        }
        utils::repeat_all_miniquad_input(self, self.subscriber);
        for event in self
            .touch
            .scroll_momentum(self.inner.egui_ctx(), macroquad::prelude::get_time())
        {
            self.inner.push_input_event(event);
        }
        let canceled = std::mem::take(&mut self.canceled);
        let mut text = false;
        let mut released = false;
        self.inner
            .run(unsafe { get_internal_gl() }.quad_context, |_, ctx| {
                f(ctx, canceled);
                // PlatformOutput is consumed by EguiMq at end_frame.
                text = ctx.output(|o| o.text_cursor_pos.is_some());
                #[cfg(target_os = "ios")]
                ctx.output(|o| {
                    if !o.copied_text.is_empty() {
                        crate::ios::set_clipboard(&o.copied_text);
                    }
                });
                released = ctx.input(|i| i.pointer.any_click());
            });
        collapse_focus_selection(
            self.inner.egui_ctx(),
            released || self.keyboard.pending > 0,
            &mut self.text_focus,
        );
        text = keyboard_text_ready(self.inner.egui_ctx(), text, self.keyboard.visible);
        if let Some(show) = self.keyboard.update(text, released, canceled) {
            show_keyboard(show);
        }
    }
    pub fn draw(&mut self) {
        let mut gl = unsafe { get_internal_gl() };
        gl.flush();
        self.inner.draw(gl.quad_context);
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub(crate) fn paint_stats(&self) -> egui_miniquad::PaintStats {
        self.inner.paint_stats()
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub fn probe_input(&mut self, events: Vec<egui::Event>, canceled: bool) {
        for event in events {
            self.inner.push_input_event(event);
        }
        self.canceled |= canceled;
    }
    pub fn cancel_input(&mut self) {
        self.text_focus = None;
        self.inner.egui_ctx().memory_mut(|m| {
            if let Some(id) = m.focus() {
                m.surrender_focus(id);
            }
        });
        if let Some(show) = self.keyboard.update(false, false, true) {
            show_keyboard(show);
        }

        for event in self.touch.cancel() {
            self.inner.push_input_event(event);
        }
        self.canceled = true;
    }
}

impl mq::EventHandler for Gui {
    fn update(&mut self, _: &mut mq::Context) {}
    fn draw(&mut self, _: &mut mq::Context) {}
    fn mouse_motion_event(&mut self, _: &mut mq::Context, x: f32, y: f32) {
        self.touch.stop_momentum();
        self.inner.mouse_motion_event(x, y);
    }
    fn mouse_wheel_event(&mut self, _: &mut mq::Context, x: f32, y: f32) {
        self.touch.stop_momentum();
        // This Miniquad version reports X11 wheel detents as ±1, while egui
        // consumes points. Touch scrolling already arrives in logical points.
        #[cfg(target_os = "linux")]
        let (x, y) = (x * 40., y * 40.);
        self.inner.mouse_wheel_event(x, y);
    }
    fn mouse_button_down_event(
        &mut self,
        ctx: &mut mq::Context,
        button: mq::MouseButton,
        x: f32,
        y: f32,
    ) {
        self.inner.mouse_button_down_event(ctx, button, x, y);
    }
    fn mouse_button_up_event(
        &mut self,
        ctx: &mut mq::Context,
        button: mq::MouseButton,
        x: f32,
        y: f32,
    ) {
        self.inner.mouse_button_up_event(ctx, button, x, y);
    }
    fn char_event(&mut self, _: &mut mq::Context, c: char, _: mq::KeyMods, _: bool) {
        self.inner.char_event(c);
    }
    fn key_down_event(
        &mut self,
        ctx: &mut mq::Context,
        key: mq::KeyCode,
        mods: mq::KeyMods,
        _: bool,
    ) {
        if let Some(event) =
            chart_clipboard_shortcut(key, mods, self.inner.egui_ctx().wants_keyboard_input())
        {
            self.inner.push_input_event(event);
            return;
        }
        self.inner.key_down_event(ctx, key, mods);
    }
    fn key_up_event(&mut self, _: &mut mq::Context, key: mq::KeyCode, mods: mq::KeyMods) {
        self.inner.key_up_event(key, mods);
    }
    fn touch_event(
        &mut self,
        _: &mut mq::Context,
        phase: mq::TouchPhase,
        id: u64,
        x: f32,
        y: f32,
        time: f64,
    ) {
        let phase = match phase {
            mq::TouchPhase::Started => egui::TouchPhase::Start,
            mq::TouchPhase::Moved => egui::TouchPhase::Move,
            mq::TouchPhase::Ended => egui::TouchPhase::End,
            mq::TouchPhase::Cancelled => egui::TouchPhase::Cancel,
        };
        let scale = self.inner.pixels_per_point();
        let (events, canceled) = self.touch.event_with_context(
            self.inner.egui_ctx(),
            phase,
            id,
            egui::pos2(x / scale, y / scale),
            (time, macroquad::prelude::get_time()),
        );
        self.canceled |= canceled;
        for event in events {
            self.inner.push_input_event(event);
        }
    }
}

// Read the platform value directly: egui's getter describes the previous frame.
// In particular, it is still 1.0 before the first Android frame.

// egui-miniquad turns C/X into text clipboard events and V into clipboard text.
// The grid owns an object clipboard; text fields continue through the adapter.

#[cfg(test)]
mod clipboard_tests;

#[cfg(test)]
mod density_tests;

// Compile the adapter's real, GL-independent input path in workspace tests.
#[cfg(test)]
#[path = "../../../../vendor/egui-miniquad/src/dpi.rs"]
mod adapter_density;

/// Only a completed text-field tap may request the software keyboard. Generic
/// widget focus, restored focus and lifecycle cancellation never open it.
#[derive(Default)]
struct ImeGate {
    visible: bool,
    pending: u8,
}
impl ImeGate {
    fn update(&mut self, text: bool, tap: bool, cancel: bool) -> Option<bool> {
        if cancel {
            self.pending = 0;
        } else if tap {
            // DragValue becomes a TextEdit on the next frame. A panel tap can
            // also deliver down/up in one batch, before text-cursor output exists.
            self.pending = 2;
        }
        let requested = self.pending > 0;
        self.pending = self.pending.saturating_sub(1);
        if !cancel && text && requested {
            self.pending = 0;
            self.visible = true;
            return Some(true);
        }
        if cancel || !text {
            if self.visible || cancel {
                self.visible = false;
                return Some(false);
            }
            return None;
        }
        None
    }
}

#[cfg(test)]
mod ime_tests;

#[cfg(test)]
mod text_ime_tests;

#[cfg(test)]
mod first_tap_tests;

#[cfg(test)]
mod event_value_ime_tests;
