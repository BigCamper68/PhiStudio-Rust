//! Native line-scoped controls, line groups and note sounds.
use crate::{
    files::FileRequest,
    media::{MediaFile, MediaKind},
    scroll::{TouchCombo as _, TouchScroll as _},
    workbench::Workbench,
};
use phistudio_core::{Command, ControlKind, ControlPoint, GroupAction, NoteId, NoteKind};
use std::collections::BTreeSet;
#[derive(Default)]
pub(crate) struct InspectorState {
    pub group_filter: Option<usize>,
    pub control: Option<ControlDraft>,
    pub groups: Option<GroupDraft>,
    pub sound: Option<SoundDraft>,
    pub pending_sound: Option<SoundTarget>,
    pub audition: Option<(Option<String>, NoteKind)>,
}
pub(crate) struct ControlDraft {
    pub line: usize,
    pub kind: ControlKind,
    pub points: Vec<ControlPoint>,
    pub revision: u64,
}
pub(crate) struct GroupDraft {
    pub revision: u64,
    pub selected: usize,
    pub name: String,
    pub members: BTreeSet<usize>,
}
pub(crate) struct SoundDraft {
    pub target: SoundTarget,
    pub reference: String,
    pub catalog: Vec<String>,
}
#[derive(Clone)]
pub(crate) struct SoundTarget {
    pub ids: Vec<NoteId>,
    pub session: u64,
    pub revision: u64,
}
impl InspectorState {
    pub fn close_top_window(&mut self) -> bool {
        self.control.take().is_some() || self.sound.take().is_some() || self.groups.take().is_some()
    }
}
fn window(title: &str, ctx: &egui::Context) -> crate::window::Window<'static> {
    crate::i18n::window(title)
        .default_pos([36., 48.])
        .default_width(420.)
        .resize(|r| {
            r.max_size([
                (ctx.screen_rect().width() - 32.).max(170.),
                (ctx.screen_rect().height() - 110.).max(110.),
            ])
        })
}
impl Workbench {
    pub(crate) fn set_note_volume(&mut self, id: NoteId, volume: f64) {
        if !volume.is_finite() || !(0. ..=1.).contains(&volume) {
            return;
        }
        let old = self.editor.document().note_sound_gain(id);
        if volume == old.as_ref().map_or(1., |g| g.volume) {
            return;
        }
        if volume == 1. {
            self.execute(Command::SetNoteVolume { id, gain: None });
            return;
        }
        let Some(root) = self.asset_root().map(ToOwned::to_owned) else {
            self.error =
                Some("Save this chart as a project before changing note sound volume".into());
            self.draft = None;
            return;
        };
        let Ok(note) = self.editor.document().note(id.line, id.index) else {
            return;
        };
        let source = old.map(|g| g.source).unwrap_or_else(|| {
            self.editor
                .document()
                .note_hitsound(id)
                .ok()
                .flatten()
                .map(str::to_owned)
        });
        let skin = self
            .options
            .skin
            .as_ref()
            .and_then(|id| self.skins.entries.iter().find(|s| &s.id == id))
            .map(|s| s.assets.clone());
        self.files.request = Some(FileRequest::NoteVolume {
            root,
            skin,
            source,
            kind: note.kind,
            volume,
            target: SoundTarget {
                ids: vec![id],
                session: self.session,
                revision: self.editor.revision(),
            },
        });
        self.transport.playing = false;
    }
    pub(crate) fn open_controls(&mut self, kind: ControlKind) {
        match self.editor.document().control_points(self.line, kind) {
            Ok(points) => {
                self.inspector.control = Some(ControlDraft {
                    line: self.line,
                    kind,
                    points,
                    revision: self.editor.revision(),
                })
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    pub(crate) fn open_groups(&mut self) {
        self.inspector.groups = Some(GroupDraft {
            revision: self.editor.revision(),
            selected: 0,
            name: "New group".into(),
            members: [self.line].into(),
        });
    }
    pub(crate) fn open_sounds(&mut self) {
        let ids = self.selection().notes;
        if ids.is_empty() {
            self.error = Some("Select notes first".into());
            return;
        }
        let reference = self
            .editor
            .document()
            .note_hitsound(ids[0])
            .ok()
            .flatten()
            .unwrap_or_default()
            .to_owned();
        let target = SoundTarget {
            ids,
            session: self.session,
            revision: self.editor.revision(),
        };
        self.inspector.sound = Some(SoundDraft {
            target: target.clone(),
            reference,
            catalog: vec![],
        });
        if let Some(root) = self.asset_root() {
            self.files.request = Some(FileRequest::SoundCatalog {
                root: root.to_owned(),
                exclude: self.project_options.music.clone(),
                target,
            });
        }
    }
    pub(crate) fn sound_target_valid(&self, target: &SoundTarget) -> bool {
        target.session == self.session && target.revision == self.editor.revision()
    }
    pub(crate) fn apply_sound(&mut self, target: SoundTarget, reference: Option<String>) {
        if !self.sound_target_valid(&target) {
            self.error = Some("The chart changed. Select notes and reopen Hitsounds.".into());
            return;
        }
        let ids = target.ids.clone();
        self.commit_authoring(Command::SetNoteHitsound {
            ids: ids.clone(),
            reference,
        });
        self.set_selection(phistudio_core::tools::Selection {
            notes: ids,
            events: vec![],
        });
        if let Some(draft) = &mut self.inspector.sound {
            draft.target.revision = self.editor.revision();
        }
    }
    pub(crate) fn accept_hitsound(&mut self, file: MediaFile) {
        let Some(target) = self.inspector.pending_sound.take() else {
            return;
        };
        if !self.sound_target_valid(&target) {
            self.error = Some("The chart changed while selecting a hitsound".into());
            return;
        }
        if let Some(root) = self.asset_root() {
            self.files.request = Some(FileRequest::ImportHitSound {
                root: root.to_owned(),
                target,
                file,
            });
        }
    }
    pub(crate) fn inspector_windows(&mut self, ctx: &egui::Context) {
        if self.show_projects {
            return;
        }
        if let Some(mut draft) = self.inspector.control.take() {
            let mut open = true;
            let mut action = None;
            let mut switch = None;
            window("Control curves",ctx).open(&mut open).show(ctx,|ui|{
                ui.set_enabled(!self.modal_open());
                egui::ScrollArea::vertical().id_source("control-scroll").max_height(ui.available_height().max(80.)).touch_show(ui,|ui|{
                    ui.label({ let __i18n_arg_0 = &(draft.line); crate::i18n::format("Line {0} · distance-based controls", &[__i18n_arg_0.to_string()]) });
                    egui::ComboBox::from_id_source("control-kind").selected_text(crate::i18n::tr(draft.kind.label())).touch_show_ui(ui,|ui|{for kind in ControlKind::ALL {if ui.selectable_label(draft.kind==kind,crate::i18n::tr(kind.label())).clicked(){switch=Some(kind);}}});
                    ui.small(crate::i18n::tr("Distance is in native RPE units, not beats. Easing belongs to the segment arriving at a point."));
                    draw_control(ui,&draft.points);
                    let mut remove=None;
                    for (i,point) in draft.points.iter_mut().enumerate(){ui.push_id(i,|ui|{
                        ui.horizontal_wrapped(|ui|{ui.label(format!("#{}",i+1));ui.label(crate::i18n::tr("Distance"));ui.add(crate::numeric::Value::new(&mut point.x).speed(5.));ui.label(crate::i18n::tr("Value"));ui.add(crate::numeric::Value::new(&mut point.value).speed(0.01));if ui.small_button(crate::i18n::tr("Remove")).clicked(){remove=Some(i);}});
                        egui::ComboBox::from_id_source("incoming-easing").selected_text({ let __i18n_arg_0 = &(crate::i18n::tr(crate::authoring_ui::EASINGS.get((point.easing-1).max(0) as usize).unwrap_or(&"Unknown"))); crate::i18n::format("Incoming: {0}", std::slice::from_ref(__i18n_arg_0)) }).touch_show_ui(ui,|ui|{for (i,label) in crate::authoring_ui::EASINGS.iter().enumerate(){ui.selectable_value(&mut point.easing,i as i32+1,crate::i18n::tr(*label));}});
                    });}
                    if let Some(i)=remove {draft.points.remove(i);}
                    ui.horizontal_wrapped(|ui|{
                        if ui.add_enabled(draft.points.len()<1024,egui::Button::new(crate::i18n::tr("Add point"))).clicked(){let x=draft.points.iter().map(|p|p.x).fold(-100.,f64::max)+100.;draft.points.push(ControlPoint{source:None,x,value:1.,easing:1});}
                        if ui.button(crate::i18n::tr("Apply curve")).clicked(){action=Some(Some(draft.points.clone()));}
                        if ui.button(crate::i18n::tr("Reset to identity")).clicked(){action=Some(None);}
                        if ui.button(crate::i18n::tr("Reload")).clicked(){switch=Some(draft.kind);}
                    });
                });
            });
            if open {
                if let Some(kind) = switch {
                    self.open_controls(kind);
                } else if let Some(points) = action {
                    if draft.revision != self.editor.revision() {
                        self.error = Some(
                            "The chart changed. Reopen Control curves before applying.".into(),
                        );
                        self.inspector.control = Some(draft);
                    } else {
                        self.commit_authoring(Command::SetControlCurve {
                            line: draft.line,
                            kind: draft.kind,
                            points,
                        });
                        let line = self.line;
                        self.line = draft.line;
                        self.open_controls(draft.kind);
                        self.line = line;
                    }
                } else {
                    self.inspector.control = Some(draft);
                }
            }
        }
        if let Some(mut draft) = self.inspector.groups.take() {
            if draft.revision != self.editor.revision() {
                draft.members.clear();
                draft.revision = self.editor.revision();
            }
            let mut open = true;
            let mut action = None;
            let names = match self.editor.document().group_names() {
                Ok(names) => names,
                Err(e) => {
                    self.error = Some(e.to_string());
                    return;
                }
            };
            draft.selected = draft.selected.min(names.len() - 1);
            draft
                .members
                .retain(|i| *i < self.editor.document().line_count());
            window("Judge line groups",ctx).open(&mut open).show(ctx,|ui|{
                ui.set_enabled(!self.modal_open());egui::ScrollArea::vertical().id_source("group-scroll").max_height(ui.available_height().max(80.)).touch_show(ui,|ui|{
                    for (i,name) in names.iter().enumerate(){if ui.selectable_label(draft.selected==i,format!("{i} · {name}")).clicked(){draft.selected=i;draft.name=name.clone();}}
                    ui.text_edit_singleline(&mut draft.name);
                    ui.horizontal_wrapped(|ui|{
                        if ui.button(crate::i18n::tr("Add group")).clicked(){action=Some(GroupAction::Add(draft.name.clone()));}
                        if ui.button(crate::i18n::tr("Rename")).clicked(){action=Some(GroupAction::Rename{index:draft.selected,name:draft.name.clone()});}
                        if ui.add_enabled(draft.selected>0,egui::Button::new(crate::i18n::tr("Delete group"))).clicked(){action=Some(GroupAction::Delete(draft.selected));}
                        if ui.add_enabled(draft.selected>1,egui::Button::new(crate::i18n::tr("Up"))).clicked(){action=Some(GroupAction::Move{from:draft.selected,to:draft.selected-1});draft.selected-=1;}
                        if ui.add_enabled(draft.selected>0&&draft.selected+1<names.len(),egui::Button::new(crate::i18n::tr("Down"))).clicked(){action=Some(GroupAction::Move{from:draft.selected,to:draft.selected+1});draft.selected+=1;}
                    });
                    ui.small(crate::i18n::tr("Deleting a group moves its lines to group 0. Lines and parent links keep their indices."));
                    ui.separator();ui.strong(crate::i18n::tr("Lines to move"));
                    for line in self.editor.document().lines(){let mut selected=draft.members.contains(&line.index);let group=self.editor.document().line_group(line.index);if ui.checkbox(&mut selected,{ let __i18n_arg_0 = &(line.index); let __i18n_arg_1 = &(line.name); let __i18n_arg_2 = &(group); crate::i18n::format("L{0} · {1} · group {2}", &[__i18n_arg_0.to_string(), __i18n_arg_1.to_string(), __i18n_arg_2.to_string()]) }).changed(){if selected{draft.members.insert(line.index);}else{draft.members.remove(&line.index);}}}
                    if ui.button(crate::i18n::tr("Move checked lines to selected group")).clicked(){action=Some(GroupAction::Assign{lines:draft.members.iter().copied().collect(),group:draft.selected});}
                });
            });
            if let Some(action) = action {
                self.commit_authoring(Command::EditGroups(action));
                draft.revision = self.editor.revision();
            }
            if open {
                self.inspector.groups = Some(draft);
            }
        }
        if let Some(mut draft) = self.inspector.sound.take() {
            let mut open = true;
            let mut apply = false;
            let mut import = false;
            let mut audition = false;
            window("Hitsounds",ctx).open(&mut open).show(ctx,|ui|{
                ui.set_enabled(!self.modal_open());egui::ScrollArea::vertical().id_source("sound-scroll").max_height(ui.available_height().max(80.)).touch_show(ui,|ui|{
                    ui.label({ let __i18n_arg_0 = &(draft.target.ids.len()); crate::i18n::format("{0} selected notes", &[__i18n_arg_0.to_string()]) });
                    egui::ComboBox::from_id_source("sound-choice").selected_text(if draft.reference.is_empty(){crate::i18n::tr("Default by note type")}else{draft.reference.clone()}).touch_show_ui(ui,|ui|{
                        ui.selectable_value(&mut draft.reference,String::new(),crate::i18n::tr("Default by note type"));
                        for (path,label) in [("tap.mp3","Tap / Hold"),("flick.mp3","Flick"),("drag.mp3","Drag")] {ui.selectable_value(&mut draft.reference,path.into(),crate::i18n::tr(label));}
                        for path in &draft.catalog {ui.selectable_value(&mut draft.reference,path.clone(),path);}
                    });
                    ui.add(egui::TextEdit::singleline(&mut draft.reference).hint_text(crate::i18n::tr("Project-relative audio path")));
                    ui.horizontal_wrapped(|ui|{import=ui.button(crate::i18n::tr("Import from files")).clicked();apply=ui.button(crate::i18n::tr("Apply hitsound")).clicked();audition=ui.button(crate::i18n::tr("Test applied sound")).clicked();});
                    ui.small(crate::i18n::tr("Custom clips: up to 8 MiB and 10 seconds. Imports are copied into the project and included in package exports."));
                });
            });
            if import {
                if self.asset_root().is_some() {
                    self.inspector.pending_sound = Some(draft.target.clone());
                    self.open_media_picker(MediaKind::HitSound);
                } else {
                    self.error =
                        Some("Save this chart as a project before importing sounds".into());
                }
            }
            if audition && let Some(id) = draft.target.ids.first() {
                match self.editor.document().note(id.line, id.index) {
                    Ok(note) => {
                        self.inspector.audition = Some((
                            self.editor
                                .document()
                                .note_hitsound(*id)
                                .ok()
                                .flatten()
                                .map(str::to_owned),
                            note.kind,
                        ))
                    }
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
            let target = draft.target.clone();
            let reference =
                (!draft.reference.trim().is_empty()).then(|| draft.reference.trim().to_owned());
            if open {
                self.inspector.sound = Some(draft);
            }
            if apply {
                if reference
                    .as_deref()
                    .is_none_or(phistudio_core::builtin_hitsound)
                {
                    self.apply_sound(target, reference);
                } else if let Some(root) = self.asset_root() {
                    self.files.request = Some(FileRequest::CheckHitSound {
                        root: root.to_owned(),
                        target,
                        reference: reference.unwrap(),
                    });
                }
            }
        }
    }
}
fn draw_control(ui: &mut egui::Ui, points: &[ControlPoint]) {
    let rect = crate::plot::square(ui);
    let mut points = points.to_vec();
    points.sort_by(|a, b| a.x.total_cmp(&b.x));
    if points.len() == 2
        && points[0].easing == 1
        && (points[0].value - 1.).abs() < 1e-4
        && points.iter().any(|p| p.value != 1.)
    {
        points[0].easing = 2;
    }
    let min = points.first().map_or(0., |p| p.x);
    let max = points.last().map_or(1000., |p| p.x).max(min + 1.);
    let mut curve = phistudio_prpr::ControlCurve::new(&points);
    let samples: Vec<_> = (0..=160)
        .map(|i| curve.sample(min + (max - min) * i as f64 / 160.))
        .collect();
    let low = samples.iter().copied().fold(0., f32::min);
    let high = samples.iter().copied().fold(1., f32::max).max(low + 0.1);
    ui.painter().rect_filled(rect, 2., crate::theme::base());
    ui.painter().add(egui::Shape::line(
        samples
            .into_iter()
            .enumerate()
            .map(|(i, y)| {
                egui::pos2(
                    rect.left() + rect.width() * i as f32 / 160.,
                    rect.bottom() - (y - low) / (high - low) * rect.height(),
                )
            })
            .collect(),
        egui::Stroke::new(1.5_f32, crate::theme::accent()),
    ));
}
