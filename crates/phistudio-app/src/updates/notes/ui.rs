use super::*;
use crate::{
    i18n::{format, tr},
    scroll::TouchScroll as _,
    workbench::Workbench,
};

impl Workbench {
    pub(crate) fn release_notes_open(&self) -> bool {
        self.updates.notes.open
    }
    pub(crate) fn poll_release_notes(&mut self) {
        self.updates.notes.tick(
            self.options.release_notes_seen.as_deref(),
            self.managed_storage,
        );
    }
    pub(crate) fn release_notes_back(&mut self) -> bool {
        if !self.updates.notes.open {
            return false;
        }
        if let Some(version) = self.updates.notes.close() {
            self.options.release_notes_seen = Some(version);
        }
        true
    }
    pub(crate) fn release_notes_window(&mut self, ctx: &egui::Context) {
        let blocked = self.playtest.active()
            || self.preview.fullscreen
            || self.modal_open_except_release_notes();
        if blocked {
            if self.updates.notes.open {
                self.updates.notes.open = false;
                self.updates.notes.requested = true;
            }
            return;
        }
        if self.updates.notes.requested {
            self.updates.notes.requested = false;
            self.updates.notes.open = true;
            self.transport.playing = false;
            self.timeline.cancel_gesture();
        }
        if !self.updates.notes.open {
            return;
        }
        let mut open = true;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let mut retry = false;
        crate::i18n::window("Release notes")
            .open(&mut open)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .default_width((ctx.screen_rect().width() - 48.).clamp(160., 560.))
            .min_width(120.)
            .resize(|r| {
                r.max_size(egui::vec2(
                    (ctx.screen_rect().width() - 32.).max(160.),
                    (ctx.screen_rect().height() - 56.).max(120.),
                ))
            })
            .show(ctx, |ui| {
                ui.style_mut().wrap = Some(true);
                ui.strong(format(
                    "PhiStudio Rust {0}",
                    &[service::installed_version().into()],
                ));
                egui::ScrollArea::vertical()
                    .id_source("release-notes-body")
                    .max_height((ctx.screen_rect().height() - 200.).max(80.))
                    .touch_show_y(ui, |ui| match &self.updates.notes.content {
                        Content::Idle | Content::Loading => {
                            ui.spinner();
                            ui.label(tr("Loading release notes…"));
                        }
                        Content::Ready(release) => {
                            if release.name != release.version.to_string() {
                                ui.heading(&release.name);
                            }
                            if release.body.trim().is_empty() {
                                ui.label(tr("This release has no release notes."));
                            } else {
                                release_text(ui, &release.body);
                            }
                        }
                        Content::Failed(error) => {
                            ui.label(tr(if *error == CheckError::NoRelease {
                                "Release notes for this version have not been published yet."
                            } else {
                                error.message()
                            }));
                        }
                    });
                if matches!(self.updates.notes.content, Content::Ready(_)) {
                    self.updates.notes.presented();
                }
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    close |= ui.button(tr("Close")).clicked();
                    if !matches!(self.updates.notes.content, Content::Loading) {
                        retry = ui.button(tr("Refresh release notes")).clicked();
                    }
                    if let Some(release) = self.updates.notes.release() {
                        ui.hyperlink_to(tr("Open release"), &release.url);
                    } else {
                        ui.hyperlink_to(tr("All releases"), service::RELEASES);
                    }
                });
            });
        ctx.move_to_top(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("Release notes"),
        ));
        if close || !open {
            self.release_notes_back();
        } else if retry {
            self.updates.notes.request(false, true);
        }
    }
}
// GitHub's Markdown stays data: no remote images, HTML or embedded scripts are executed.
fn release_text(ui: &mut egui::Ui, body: &str) {
    for paragraph in body.split("\n\n") {
        for line in paragraph.lines() {
            let text = line.trim();
            if let Some(title) = text
                .strip_prefix("### ")
                .or_else(|| text.strip_prefix("## "))
                .or_else(|| text.strip_prefix("# "))
            {
                ui.strong(title);
            } else if let Some(item) = text.strip_prefix("- ").or_else(|| text.strip_prefix("* ")) {
                ui.horizontal(|ui| {
                    ui.label("•");
                    ui.add(egui::Label::new(item).wrap(true));
                });
            } else {
                ui.add(egui::Label::new(text).wrap(true));
            }
        }
        ui.add_space(6.);
    }
}
