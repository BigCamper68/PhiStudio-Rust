//! Audition scope is independent of visual line filters and resets with a project.
use crate::{i18n::tr, scroll::TouchScroll as _, workbench::Workbench};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Scope {
    #[default]
    All,
    Active,
    Groups,
}
#[derive(Default)]
pub(crate) struct HitsoundAudition {
    session: Option<u64>,
    scope: Scope,
    groups: BTreeSet<usize>,
    key: Option<(u64, usize)>,
    lines: Option<Vec<usize>>,
}
impl HitsoundAudition {
    fn session(&mut self, session: u64) {
        if self.session != Some(session) {
            *self = Self {
                session: Some(session),
                ..Default::default()
            };
        }
    }
}
impl Workbench {
    pub(crate) fn hitsound_lines(&mut self) -> Option<&[usize]> {
        self.hitsound_audition.session(self.session);
        let state = &mut self.hitsound_audition;
        let key = (self.editor.revision(), self.line);
        if state.key != Some(key) {
            state.lines = match state.scope {
                Scope::All => None,
                Scope::Active => Some(vec![self.line]),
                Scope::Groups => Some(
                    (0..self.editor.document().line_count())
                        .filter(|&line| {
                            state
                                .groups
                                .contains(&self.editor.document().line_group(line))
                        })
                        .collect(),
                ),
            };
            state.key = Some(key);
        }
        state.lines.as_deref()
    }

    pub(crate) fn hitsound_menu(&mut self, ui: &mut egui::Ui) {
        self.hitsound_audition.session(self.session);
        let color = if self.hitsound_audition.scope == Scope::All {
            crate::theme::muted()
        } else {
            crate::theme::accent()
        };
        let padding = ui.spacing().button_padding;
        ui.spacing_mut().button_padding.y = 0.;
        let response = ui.button(egui::RichText::new("♪").size(12.).color(color));
        ui.spacing_mut().button_padding = padding;
        let popup = ui.make_persistent_id("hitsound-audition-popup");
        if response.clicked() {
            ui.memory_mut(|m| m.toggle_popup(popup));
        }
        egui::popup::popup_below_widget(ui, popup, &response, |ui| {
            ui.set_width((ui.ctx().screen_rect().width() - 32.).min(280.));
            ui.style_mut().wrap = Some(true);
            let spacing = ui.ctx().style().spacing.clone();
            ui.spacing_mut().button_padding = spacing.button_padding;
            ui.spacing_mut().interact_size = spacing.interact_size;
            ui.strong(tr("Hitsound audition"));
            ui.small(tr(
                "Changes hitsound audio only. Note visibility and judgment stay unchanged.",
            ));
            let state = &mut self.hitsound_audition;
            let old = state.scope;
            for (scope, label) in [
                (Scope::All, "All lines"),
                (Scope::Active, "Current line"),
                (Scope::Groups, "Line groups"),
            ] {
                ui.radio_value(&mut state.scope, scope, tr(label));
            }
            if state.scope != old {
                state.key = None;
                if state.scope == Scope::Groups && state.groups.is_empty() {
                    state
                        .groups
                        .insert(self.editor.document().line_group(self.line));
                }
            }
            if state.scope == Scope::Groups {
                let groups = self.editor.document().group_names().unwrap_or_default();
                egui::ScrollArea::vertical()
                    .id_source("hitsound-groups")
                    .max_height((ui.ctx().screen_rect().height() - 220.).clamp(64., 220.))
                    .touch_show_y(ui, |ui| {
                        for (index, name) in groups.iter().enumerate() {
                            let mut checked = state.groups.contains(&index);
                            if ui.checkbox(&mut checked, name).changed() {
                                if checked {
                                    state.groups.insert(index);
                                } else {
                                    state.groups.remove(&index);
                                }
                                state.key = None;
                            }
                        }
                    });
                if state.groups.is_empty() {
                    ui.small(tr("No groups selected: all hitsounds are muted."));
                }
            }
        });
        response.on_hover_text(tr(
            "Audition hitsounds on the current line or selected line groups",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phistudio_core::{ChartDocument, Command, GroupAction};
    #[test]
    fn sidebar_audition_menu_accepts_routed_phone_taps() {
        let mut app = Workbench::new(ChartDocument::empty("Phone audio", 120.).unwrap(), None);
        let original = app.editor.document().as_json().clone();
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::definitions());
        let mut router = crate::touch::TouchRouter::default();
        let frame = |app: &mut Workbench, events, canceled| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(360., 780.),
                    )),
                    pixels_per_point: Some(2.75),
                    events,
                    ..Default::default()
                },
                |ctx| app.sound_ui(ctx, canceled),
            )
        };
        let mut output = frame(&mut app, vec![], false);
        for _ in 0..3 {
            output = frame(&mut app, vec![], false);
        }
        for label in ["♪", "Current line"] {
            let pos = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.1 {
                    egui::Shape::Text(text) if text.galley.job.text == label => {
                        let p = text.pos + text.galley.size() * 0.5;
                        shape.0.contains(p).then_some(p)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("Missing visible audio control: {label}"));
            for phase in [egui::TouchPhase::Start, egui::TouchPhase::End] {
                let time = ctx.input(|i| i.time) + 0.02;
                let (events, canceled) = router.event_with_context(&ctx, phase, 1, pos, time);
                output = frame(&mut app, events, canceled);
            }
            for _ in 0..3 {
                output = frame(&mut app, vec![], false);
            }
        }
        assert_eq!(app.hitsound_lines(), Some([0].as_slice()));
        assert_eq!(app.editor.document().as_json(), &original);
        assert!(!app.editor.is_dirty());
    }
    #[test]
    fn audition_follows_line_groups_without_changing_the_chart_and_resets_between_projects() {
        let mut app = Workbench::new(ChartDocument::empty("Audio scope", 120.).unwrap(), None);
        app.execute(Command::AddLine {
            name: "Second".into(),
        });
        app.editor
            .execute(Command::EditGroups(GroupAction::Add("Other".into())))
            .unwrap();
        app.editor
            .execute(Command::EditGroups(GroupAction::Assign {
                lines: vec![1],
                group: 1,
            }))
            .unwrap();
        let original = app.editor.document().as_json().clone();
        assert!(app.hitsound_lines().is_none());
        app.hitsound_audition.scope = Scope::Active;
        app.hitsound_audition.key = None;
        assert_eq!(app.hitsound_lines(), Some([0].as_slice()));
        app.line = 1;
        assert_eq!(app.hitsound_lines(), Some([1].as_slice()));
        app.hitsound_audition.scope = Scope::Groups;
        app.hitsound_audition.groups.insert(0);
        app.hitsound_audition.key = None;
        assert_eq!(app.hitsound_lines(), Some([0].as_slice()));
        assert_eq!(app.editor.document().as_json(), &original);
        app.replace_document(ChartDocument::empty("Next", 120.).unwrap(), None, None);
        assert!(app.hitsound_lines().is_none());
    }
}
