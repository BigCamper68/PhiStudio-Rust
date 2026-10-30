use super::*;

impl Workbench {
    pub(super) fn capture_sidebar_resize(&self, ctx: &egui::Context, canceled: bool) -> bool {
        if canceled || self.modal_open() {
            return false;
        }
        let id = egui::Id::new("context");
        let Some(panel) = egui::containers::panel::PanelState::load(ctx, id) else {
            return false;
        };
        let (presses, origin, first_release, last_release) = ctx.input(|i| {
            let mut presses = 0;
            let mut origin = None;
            let mut first_release = None;
            let mut last_release = None;
            for event in &i.events {
                if let egui::Event::PointerButton {
                    pos,
                    pressed,
                    button: egui::PointerButton::Primary,
                    ..
                } = event
                {
                    if *pressed {
                        presses += 1;
                        origin = Some(*pos);
                    } else {
                        first_release.get_or_insert(*pos);
                        last_release = Some(*pos);
                    }
                }
            }
            (presses, origin, first_release, last_release)
        });
        // egui 0.21 checks the latest position when capturing a resize. A MOVE
        // batched with DOWN can already be beyond its narrow resize strip.
        let capture = presses == 1
            && origin.is_some_and(|pos| {
                panel.rect.y_range().contains(&pos.y)
                    && (pos.x - panel.rect.left()).abs()
                        <= ctx.style().interaction.resize_grab_radius_side
                    && ctx.layer_id_at(pos) == Some(egui::LayerId::background())
            });
        let resize_id = id.with("__resize");
        let owned = ctx.memory(|m| m.is_being_dragged(resize_id));
        if !capture && !owned {
            return false;
        }
        let release = if owned && !capture {
            first_release
        } else {
            last_release
        };
        if let Some(pos) = release {
            // PointerGone on touch release removes egui's latest position.
            // Apply the actual UP coordinate before the panel reads its state.
            let available = ctx.available_rect();
            let limits = sidebar_widths(ctx.screen_rect().width());
            let width = (available.right() - pos.x)
                .clamp(*limits.start(), *limits.end())
                .min(available.width());
            let rect = egui::Rect::from_min_max(
                egui::pos2(available.right() - width, panel.rect.top()),
                egui::pos2(available.right(), panel.rect.bottom()),
            );
            ctx.data_mut(|d| d.insert_persisted(id, egui::containers::panel::PanelState { rect }));
        } else if capture {
            ctx.memory_mut(|m| m.set_dragged_id(resize_id));
        }
        true
    }
}
