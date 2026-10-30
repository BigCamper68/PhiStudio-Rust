//! Touch scrolling must win over buttons/text fields for vertical gestures.
//! Register actual viewport/layer geometry; the grid keeps its own gesture path.
use egui::{Context, Id, LayerId, Pos2, Rect, ScrollArea, Ui};

#[derive(Clone, Copy)]
pub struct Zone {
    pub id: Id,
    pub layer: LayerId,
    pub rect: Rect,
    pub max_offset: f32,
    pub axis: usize,
}
fn key() -> Id {
    Id::new("phistudio-scroll-zones")
}
pub fn begin_frame(ctx: &Context) {
    ctx.data_mut(|d| d.insert_temp(key(), Vec::<Zone>::new()));
}
pub fn at(ctx: &Context, pos: Pos2) -> Option<Zone> {
    let layer = ctx.layer_id_at(pos)?;
    ctx.data_mut(|d| d.get_temp::<Vec<Zone>>(key()))
        .unwrap_or_default()
        .into_iter()
        .filter(|z| z.layer == layer && z.rect.contains(pos))
        .min_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))
}
pub fn exists(ctx: &Context, zone: Zone, anchor: Pos2) -> bool {
    at(ctx, anchor).is_some_and(|z| z.id == zone.id && z.layer == zone.layer)
}
/// Apply a finger delta to its captured viewport. Synthetic wheel events can
/// escape into an ancestor when a nested list reaches its boundary.
pub fn move_by(ctx: &Context, zone: Zone, delta: f32) -> bool {
    if !delta.is_finite() {
        return false;
    }
    let live = ctx
        .data_mut(|d| d.get_temp::<Vec<Zone>>(key()))
        .unwrap_or_default()
        .into_iter()
        .find(|z| z.id == zone.id && z.layer == zone.layer);
    let Some(live) = live else {
        return false;
    };
    let Some(mut state) = egui::scroll_area::State::load(ctx, zone.id) else {
        return false;
    };
    let before = state.offset[live.axis];
    state.offset[live.axis] = (before - delta).clamp(0., live.max_offset);
    let moved = (state.offset[live.axis] - before).abs() > 0.001;
    state.store(ctx, zone.id);
    if moved {
        ctx.request_repaint();
    }
    moved
}
pub trait TouchScroll {
    fn touch_show_y<R>(
        self,
        ui: &mut Ui,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> egui::scroll_area::ScrollAreaOutput<R>;
    fn touch_show<R>(
        self,
        ui: &mut Ui,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> egui::scroll_area::ScrollAreaOutput<R>;
    fn touch_show_x<R>(
        self,
        ui: &mut Ui,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> egui::scroll_area::ScrollAreaOutput<R>;
}
impl TouchScroll for ScrollArea {
    fn touch_show_y<R>(
        self,
        ui: &mut Ui,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> egui::scroll_area::ScrollAreaOutput<R> {
        let layer = ui.layer_id();
        let output = self
            .hscroll(false)
            .auto_shrink([false, true])
            .show(ui, content);
        register(ui, layer, &output, 1);
        output
    }
    fn touch_show<R>(
        self,
        ui: &mut Ui,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> egui::scroll_area::ScrollAreaOutput<R> {
        touch_area(self, ui, content, 1)
    }
    fn touch_show_x<R>(
        self,
        ui: &mut Ui,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> egui::scroll_area::ScrollAreaOutput<R> {
        touch_area(self, ui, content, 0)
    }
}
fn touch_area<R>(
    area: ScrollArea,
    ui: &mut Ui,
    content: impl FnOnce(&mut Ui) -> R,
    axis: usize,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    let layer = ui.layer_id();
    // Non-scrollable X in egui 0.21 expands to its widest child even with
    // max_width. A bounded viewport keeps oversized fields reachable.
    let output = area
        .hscroll(true)
        .auto_shrink([false, true])
        .show(ui, content);
    register(ui, layer, &output, axis);
    output
}
fn register<R>(
    ui: &Ui,
    layer: LayerId,
    output: &egui::scroll_area::ScrollAreaOutput<R>,
    axis: usize,
) {
    if output.content_size[axis] > output.inner_rect.size()[axis] + 0.5 {
        let zone = Zone {
            id: output.id,
            layer,
            rect: output.inner_rect.intersect(ui.clip_rect()),
            max_offset: (output.content_size[axis] - output.inner_rect.size()[axis]).max(0.),
            axis,
        };
        ui.ctx()
            .data_mut(|d| d.get_temp_mut_or_default::<Vec<Zone>>(key()).push(zone));
    }
}

pub trait TouchCombo {
    fn touch_show_ui<R>(
        self,
        ui: &mut Ui,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> egui::InnerResponse<Option<R>>;
}
impl TouchCombo for egui::ComboBox {
    fn touch_show_ui<R>(
        self,
        ui: &mut Ui,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> egui::InnerResponse<Option<R>> {
        let height = (ui.ctx().screen_rect().height() - 96.).clamp(120., 360.);
        let previous = ui.spacing().combo_height;
        ui.spacing_mut().combo_height = height;
        let response = self
            .wrap(ui.style().wrap.unwrap_or(false))
            .show_ui(ui, |ui| {
                ScrollArea::vertical()
                    .id_source("combo-touch-list")
                    .max_height(height)
                    .touch_show(ui, content)
                    .inner
            });
        ui.spacing_mut().combo_height = previous;
        response
    }
}
