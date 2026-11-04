use super::*;
use crate::scroll::TouchScroll as _;
use phistudio_sdk::{Manifest, PermissionDecision, PermissionRequest};

pub(super) struct Review {
    manifest: Manifest,
    decision: PermissionDecision,
}

impl Workbench {
    pub(super) fn review_plugin_permissions(&mut self, info: &phistudio_plugins::PluginInfo) {
        let requested = PermissionRequest::from_manifest(&info.manifest);
        let granted = self
            .plugins
            .registry
            .permissions
            .get(&info.manifest.id)
            .map_or_else(
                || PermissionRequest {
                    capabilities: requested.capabilities.clone(),
                    network: phistudio_sdk::NetworkPermissions {
                        origins: requested.network.origins.clone(),
                        local_network: false,
                    },
                    native: false,
                },
                |previous| PermissionRequest {
                    capabilities: previous.effective(&info.manifest),
                    network: phistudio_sdk::NetworkPermissions {
                        origins: previous
                            .granted
                            .network
                            .origins
                            .intersection(&requested.network.origins)
                            .cloned()
                            .collect(),
                        local_network: previous.granted.network.local_network
                            && requested.network.local_network,
                    },
                    native: previous.granted.native && requested.native,
                },
            );
        self.plugins.permission_review = Some(Review {
            manifest: info.manifest.clone(),
            decision: PermissionDecision { requested, granted },
        });
    }

    pub(super) fn plugin_permission_window(&mut self, ctx: &egui::Context) {
        let Some(review) = &mut self.plugins.permission_review else {
            return;
        };
        let mut open = true;
        let mut apply = None;
        let mut revoke = false;
        crate::i18n::window("Plugin permissions")
            .id(egui::Id::new("plugin-permissions"))
            .open(&mut open).collapsible(false)
            .default_width(430.)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().id_source("permission-list")
                    .max_height((ctx.screen_rect().height() - 150.).max(100.))
                    .touch_show(ui, |ui| {
                        ui.strong(&review.manifest.name);
                        ui.small(&review.manifest.id);
                        for capability in &review.decision.requested.capabilities {
                            let mut granted = review.decision.granted.capabilities.contains(capability);
                            if ui.checkbox(&mut granted, crate::i18n::tr(capability.title())).changed() {
                                if granted { review.decision.granted.capabilities.insert(capability.clone()); }
                                else { review.decision.granted.capabilities.remove(capability); }
                            }
                        }
                        if review.decision.requested.capabilities.contains(&Capability::Network) {
                            ui.separator();
                            ui.label(crate::i18n::tr("Allowed servers"));
                            ui.small(crate::i18n::tr("Network requests can send data read by the plugin."));
                            for origin in &review.decision.requested.network.origins {
                                let mut allowed = review.decision.granted.network.origins.contains(origin);
                                if ui.checkbox(&mut allowed, origin).changed() {
                                    if allowed { review.decision.granted.network.origins.insert(origin.clone()); }
                                    else { review.decision.granted.network.origins.remove(origin); }
                                }
                            }
                            if review.decision.requested.network.local_network {
                                ui.checkbox(&mut review.decision.granted.network.local_network, crate::i18n::tr("Allow local network"));
                            }
                        }
                        if review.decision.requested.native {
                            ui.separator();
                            ui.label(crate::i18n::tr("Native code can access the editor process. SDK permissions do not sandbox it."));
                            ui.checkbox(&mut review.decision.granted.native, crate::i18n::tr("Trust this native plugin"));
                        }
                        ui.separator();
                        ui.horizontal_wrapped(|ui| {
                            let trusted = !review.decision.requested.native || review.decision.granted.native;
                            if ui.add_enabled(trusted, egui::Button::new(crate::i18n::tr("Apply and enable"))).clicked() { apply = Some(true); }
                            if ui.button(crate::i18n::tr("Apply permissions")).clicked() { apply = Some(false); }
                            if ui.button(crate::i18n::tr("Revoke permissions")).clicked() { revoke = true; }
                        });
                    });
            });
        if apply.is_some() || revoke {
            let Some(mut review) = self.plugins.permission_review.take() else {
                return;
            };
            if !review
                .decision
                .granted
                .capabilities
                .contains(&Capability::Network)
            {
                review.decision.granted.network = Default::default();
            }
            if !review.decision.granted.network.local_network {
                review.decision.granted.network.origins.retain(|origin| {
                    origin.starts_with("https://") || origin.starts_with("wss://")
                });
            }
            let id = review.manifest.id;
            self.plugin_action(Action::Permissions {
                plugin: id.clone(),
                decision: (!revoke).then_some(review.decision),
            });
            if apply == Some(true) {
                self.plugin_action(Action::Enable(id));
            }
        } else if !open {
            self.plugins.permission_review = None;
        }
    }
}
