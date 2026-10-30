//! App windows retain their geometry across temporary software-keyboard insets.
use egui::{Context, Id, InnerResponse, Pos2, Rect, Resize, Ui, Vec2};

#[derive(Clone, Copy)]
struct Geometry {
    viewport: Rect,
    scale: f32,
    outer: Rect,
    content: Vec2,
}

#[derive(Clone, Default)]
struct GeometryMemory {
    last: Option<Geometry>,
    before_keyboard: Option<Geometry>,
}

/// The ordinary egui builder with one geometry safeguard at show().
pub(crate) struct Window<'a> {
    inner: egui::Window<'a>,
    id: Id,
    anchored: bool,
}

macro_rules! builder {
    ($($method:ident($argument:ident: $ty:ty);)*) => {$ (
        pub(crate) fn $method(mut self, $argument: $ty) -> Self {
            self.inner = self.inner.$method($argument);
            self
        }
    )*};
}

impl<'a> Window<'a> {
    pub(crate) fn new(title: impl Into<egui::WidgetText>, id: Id) -> Self {
        Self {
            inner: egui::Window::new(title).id(id),
            id,
            anchored: false,
        }
    }

    pub(crate) fn id(mut self, id: Id) -> Self {
        self.id = id;
        self.inner = self.inner.id(id);
        self
    }

    builder! {
        open(open: &'a mut bool);
        default_pos(position: impl Into<Pos2>);
        default_size(size: impl Into<Vec2>);
        default_width(width: f32);
        default_height(height: f32);
        min_width(width: f32);
        resizable(resizable: bool);
        collapsible(collapsible: bool);
    }

    pub(crate) fn resize(mut self, resize: impl Fn(Resize) -> Resize) -> Self {
        self.inner = self.inner.resize(resize);
        self
    }

    pub(crate) fn anchor(mut self, align: egui::Align2, offset: impl Into<Vec2>) -> Self {
        self.anchored = true;
        self.inner = self.inner.anchor(align, offset);
        self
    }

    pub(crate) fn show<R>(
        mut self,
        ctx: &Context,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> Option<InnerResponse<Option<R>>> {
        let key = self.id.with("phistudio-window-geometry");
        let mut memory = ctx
            .data_mut(|data| data.get_temp::<GeometryMemory>(key))
            .unwrap_or_default();
        let viewport = ctx.screen_rect();
        let scale = ctx.pixels_per_point();
        let compatible = |geometry: Geometry| {
            (viewport.width() - geometry.viewport.width()).abs() < 1.
                && (viewport.min - geometry.viewport.min).length() < 1.
                && (scale - geometry.scale).abs() < 0.001
        };
        if memory.before_keyboard.is_none()
            && let Some(last) = memory.last
            && compatible(last)
            && viewport.height() < last.viewport.height() - 0.01
            && ctx.wants_keyboard_input()
        {
            // Save the first full-size frame, including during animated insets.
            memory.before_keyboard = Some(last);
        }
        if let Some(saved) = memory.before_keyboard {
            if !compatible(saved) {
                // Rotation, a different display or UI scale is a new layout.
                memory.before_keyboard = None;
            } else if viewport.height() >= saved.viewport.height() - 0.01 {
                self.inner = self.inner.resize(|resize| resize.fixed_size(saved.content));
                if !self.anchored {
                    self.inner = self.inner.current_pos(saved.outer.min);
                }
                memory.before_keyboard = None;
            }
        }
        let mut size = None;
        let response = self.inner.show(ctx, |ui| {
            // This is Resize's content rectangle, before any child scroll area.
            size = Some(ui.max_rect().size());
            content(ui)
        });
        if let (Some(response), Some(content)) = (&response, size) {
            memory.last = Some(Geometry {
                viewport,
                scale,
                outer: response.response.rect,
                content,
            });
        }
        ctx.data_mut(|data| data.insert_temp(key, memory));
        response
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn visible_rect(ctx: &Context, id: Id) -> Option<Rect> {
    ctx.data_mut(|data| data.get_temp::<GeometryMemory>(id.with("phistudio-window-geometry")))
        .and_then(|memory| memory.last.map(|geometry| geometry.outer))
}

#[cfg(test)]
mod tests;
