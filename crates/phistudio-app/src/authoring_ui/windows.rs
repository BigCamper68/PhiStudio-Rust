use super::*;

impl Workbench {
    pub fn authoring_windows(&mut self, ctx: &egui::Context) {
        if self.show_line_properties {
            let mut open = true;
            crate::i18n::window("Judge line properties")
                .open(&mut open)
                .default_width(520.)
                .min_width((ctx.screen_rect().width() - 24.).clamp(180., 500.))
                .default_height(560.)
                .resize(|r| {
                    r.max_size([
                        (ctx.screen_rect().width() - 24.).max(180.),
                        (ctx.screen_rect().height() - 80.).max(100.),
                    ])
                })
                .default_pos([56., 60.])
                .show(ctx, |ui| {
                    ui.set_enabled(!self.modal_open());
                    ui.style_mut().wrap = Some(true);
                    egui::ScrollArea::vertical()
                        .id_source("line-properties-scroll")
                        .max_height(ui.available_height())
                        .touch_show_y(ui, |ui| self.line_properties_ui(ui));
                });
            self.show_line_properties = open;
        }
        if self.show_bpm {
            let mut open = true;
            crate::i18n::window("BPM timeline")
                .open(&mut open)
                .default_width(330.)
                .default_pos([80., 66.])
                .show(ctx, |ui| {
                    ui.set_enabled(!self.modal_open());
                    egui::ScrollArea::vertical()
                        .id_source("bpm-window-scroll")
                        .max_height((ctx.screen_rect().height() - 135.).max(80.))
                        .touch_show(ui, |ui| self.bpm_ui(ui));
                });
            self.show_bpm = open;
        }
        self.line_texture_window(ctx);
    }
}
