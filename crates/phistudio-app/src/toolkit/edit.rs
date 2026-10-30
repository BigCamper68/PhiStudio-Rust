use super::*;

impl Workbench {
    pub(crate) fn edit_window(&mut self, ctx: &egui::Context) {
        if !self.show_edit || self.show_projects {
            return;
        }
        let mut open = true;
        crate::i18n::window("Edit")
            .id(egui::Id::new("edit-actions"))
            .open(&mut open)
            .default_width(290.)
            .default_pos([48., 52.])
            .default_height((ctx.screen_rect().height() - 150.).clamp(130., 430.))
            .resize(|r| {
                r.max_size([
                    (ctx.screen_rect().width() - 32.).max(200.),
                    (ctx.screen_rect().height() - 110.).max(140.),
                ])
            })
            .show(ctx, |ui| {
                use crate::scroll::TouchScroll as _;
                ui.set_enabled(!self.modal_open());
                egui::ScrollArea::vertical()
                    .id_source("edit-actions-scroll")
                    .max_height(
                        ui.available_height()
                            .min((ctx.screen_rect().height() - 160.).max(90.))
                            .max(70.),
                    )
                    .touch_show(ui, |ui| self.edit_actions(ui));
            });
        self.show_edit = open;
    }

    pub(super) fn edit_actions(&mut self, ui: &mut egui::Ui) {
        let selected = !self.selection().is_empty() || !self.story_selection().is_empty();
        let paste = self.tools.clipboard.is_some();
        for (label, action, enabled) in [
            ("Cut · Ctrl+X", 0, selected),
            ("Copy · Ctrl+C", 1, selected),
            ("Paste at cursor · Ctrl+V", 2, paste),
            ("Paste mirrored · Ctrl+Shift+V", 3, paste),
            ("Paste at original time", 4, paste),
            ("Duplicate · Ctrl+D", 5, selected),
            ("Delete · Del", 6, selected),
        ] {
            if ui
                .add_enabled(enabled, egui::Button::new(crate::i18n::tr(label)))
                .clicked()
            {
                match action {
                    0 => self.copy_selection(true),
                    1 => self.copy_selection(false),
                    2 => self.paste_selection(false, false),
                    3 => self.paste_selection(true, false),
                    4 => self.paste_selection(false, true),
                    5 => self.duplicate_selection(),
                    _ => self.delete_selected(),
                };
            }
        }
        ui.separator();
        for (label, scope) in [
            ("Select all notes", Scope::Notes),
            ("Select all events", Scope::Events),
            ("Select notes and events", Scope::Both),
        ] {
            if ui.button(crate::i18n::tr(label)).clicked() {
                self.select_scope(scope, false);
            }
        }
        if ui
            .button(crate::i18n::tr("Invert selection · Ctrl+I"))
            .clicked()
        {
            self.select_scope(self.tools.scope, true);
        }
        if ui
            .button(crate::i18n::tr("Clear selection · Esc"))
            .clicked()
        {
            self.select(None);
        }
        ui.separator();
        if ui.button(crate::i18n::tr("Tools…")).clicked() {
            self.tools.show = true;
        }
    }

    pub(crate) fn selection_properties(&mut self, ui: &mut egui::Ui) {
        if !self.selection().notes.is_empty() && ui.button(crate::i18n::tr("Hitsounds…")).clicked()
        {
            self.open_sounds();
        }

        let selected = self.selection();
        ui.label(RichText::new(crate::i18n::tr("Selection properties")).strong());
        ui.small({
            let __i18n_arg_0 = &(selected.notes.len());
            let __i18n_arg_1 = &(selected.events.len());
            crate::i18n::format(
                "{0} notes · {1} events",
                &[__i18n_arg_0.to_string(), __i18n_arg_1.to_string()],
            )
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Cut")).clicked() {
                self.copy_selection(true);
            }
            if ui.button(crate::i18n::tr("Copy")).clicked() {
                self.copy_selection(false);
            }
            if ui.button(crate::i18n::tr("Duplicate")).clicked() {
                self.duplicate_selection();
            }
            if ui.button(crate::i18n::tr("Delete")).clicked() {
                self.delete_selected();
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Mirror X")).clicked() {
                self.transform_selection(Transform::MirrorX(0.));
            }
            if ui.button(crate::i18n::tr("Reverse time")).clicked() {
                self.transform_selection(Transform::ReverseTime);
            }
            if ui.button(crate::i18n::tr("Quantize")).clicked() {
                self.transform_selection(Transform::Quantize(self.division));
            }
        });
        if ui
            .button(crate::i18n::tr("Transform and batch tools…"))
            .clicked()
        {
            self.tools.show = true;
            self.tools.page = Page::Transform;
        }
        ui.small(RichText::new(crate::i18n::tr("Drag a selected object to move the whole selection. Box selects an area; Add toggles objects with a tap.")).color(muted()));
    }
}
