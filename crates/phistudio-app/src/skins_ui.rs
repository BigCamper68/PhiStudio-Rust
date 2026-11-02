use crate::{
    files::FileRequest,
    media::{MediaFile, MediaKind},
    scroll::TouchCombo as _,
    workbench::Workbench,
};
#[derive(Default)]
pub(crate) struct SkinsState {
    pub root: Option<std::path::PathBuf>,
    pub entries: Vec<phistudio_core::skins::SkinEntry>,
    pub loaded: bool,
    delete: Option<String>,
}
impl Workbench {
    pub(crate) fn accept_skin_file(&mut self, file: MediaFile) {
        self.files.request = Some(FileRequest::ImportSkin(file));
    }
    pub(crate) fn skins_ui(&mut self, ui: &mut egui::Ui) {
        ui.strong(crate::i18n::tr("Resource skins"));
        let selected = self
            .options
            .skin
            .as_ref()
            .and_then(|id| self.skins.entries.iter().find(|e| &e.id == id));
        egui::ComboBox::from_id_source("resource-skin")
            .wrap(true)
            .width(ui.available_width().min(330.))
            .selected_text(selected.map_or_else(
                || crate::i18n::tr("Phigros Official · Default"),
                |e| e.name.clone(),
            ))
            .touch_show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.options.skin,
                    None,
                    crate::i18n::tr("Phigros Official · Default"),
                );
                for skin in &self.skins.entries {
                    ui.selectable_value(
                        &mut self.options.skin,
                        Some(skin.id.clone()),
                        format!("{} · {}", skin.name, skin.author),
                    );
                }
            });
        if let Some(skin) = self
            .options
            .skin
            .as_ref()
            .and_then(|id| self.skins.entries.iter().find(|e| &e.id == id))
        {
            ui.label(format!("{} · {}", skin.name, skin.author));
            if !skin.description.is_empty() {
                ui.small(&skin.description);
            }
        } else {
            ui.small(crate::i18n::tr("Phigros Official by PigeonGames"));
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Import skin ZIP…")).clicked() {
                self.open_media_picker(MediaKind::Skin);
            }
            if ui
                .add_enabled(
                    self.options.skin.is_some(),
                    egui::Button::new(crate::i18n::tr("Delete skin…")),
                )
                .clicked()
            {
                self.skins.delete = self.options.skin.clone();
            }
        });
        if let Some(id) = self.skins.delete.clone() {
            ui.label(crate::i18n::tr(
                "Delete this imported skin from the library?",
            ));
            ui.horizontal_wrapped(|ui| {
                if ui.button(crate::i18n::tr("Delete skin")).clicked() {
                    self.files.request = Some(FileRequest::DeleteSkin(id));
                    self.skins.delete = None;
                }
                if ui.button(crate::i18n::tr("Cancel")).clicked() {
                    self.skins.delete = None;
                }
            });
        }
        ui.small(crate::i18n::tr("One skin supplies Preview and the note grid: normal/highlighted notes, Hold atlases, hitsounds, hit effects and line colors. Imported files remain unchanged."));
    }
}
