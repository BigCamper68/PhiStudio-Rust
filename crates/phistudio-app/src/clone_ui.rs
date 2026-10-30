//! UI port of Java PhiStudio EventCloneDialog (bb2e9b3): named, ordered lines.
use super::*;
use phistudio_core::ChartError;
use phistudio_core::tools::{self, CloneSpec, Selection, ToolEdit, ValueProfile};
#[derive(Clone, Debug)]
struct Offset {
    mode: usize,
    from: f64,
    to: f64,
    easing: i32,
    advanced: bool,
    sequence: String,
    variation: f64,
}
impl Default for Offset {
    fn default() -> Self {
        Self {
            mode: 0,
            from: 0.,
            to: 0.,
            easing: 1,
            advanced: false,
            sequence: "1".into(),
            variation: 0.,
        }
    }
}
pub(crate) struct CloneDraft {
    selection: Selection,
    revision: u64,
    session: u64,
    targets: Vec<usize>,
    choose: bool,
    chosen: Vec<usize>,
    page: usize,
    show_all: bool,
    search: String,
    keep: bool,
    delay_enabled: bool,
    delay: String,
    customize: bool,
    offsets: [Offset; 4],
    seed: u64,
    key: String,
    plan: Option<ToolEdit>,
    error: Option<String>,
}
impl CloneDraft {
    pub(super) fn cancel_chooser(&mut self) -> bool {
        std::mem::take(&mut self.choose)
    }
    fn spec(&self) -> Result<CloneSpec, ChartError> {
        let mut offsets: [Vec<f64>; 4] = Default::default();
        for (i, d) in self.offsets.iter().enumerate() {
            let profile = if !self.customize || d.mode == 0 {
                ValueProfile::default()
            } else {
                ValueProfile {
                    from: d.from,
                    to: if d.mode == 1 { d.from } else { d.to },
                    easing: d.easing,
                    sequence: if d.advanced {
                        d.sequence
                            .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
                            .filter(|s| !s.is_empty())
                            .map(str::parse::<f64>)
                            .collect::<Result<_, _>>()
                            .map_err(|_| ChartError::Invalid("Enter numeric multipliers".into()))?
                    } else {
                        vec![1.]
                    },
                    disturbance: if d.advanced { d.variation } else { 0. },
                    seed: self.seed.wrapping_add(i as u64 + 1),
                }
            };
            offsets[i] = profile.java_values(self.targets.len())?;
        }
        Ok(CloneSpec {
            targets: self.targets.clone(),
            delay: if self.delay_enabled {
                self.delay.parse()?
            } else {
                Beat::ZERO
            },
            keep_source: self.keep,
            offsets,
        })
    }
}
impl Workbench {
    pub(crate) fn open_clone(&mut self) {
        match self.timing_selection() {
            Ok(mut selection) => {
                selection.notes.clear();
                if selection.events.is_empty() {
                    self.error = Some("Select events to clone".into());
                    return;
                }
                self.advanced.clone = Some(CloneDraft {
                    selection,
                    revision: self.editor.revision(),
                    session: self.session,
                    targets: vec![],
                    choose: false,
                    chosen: vec![],
                    page: 0,
                    show_all: false,
                    search: String::new(),
                    keep: true,
                    delay_enabled: false,
                    delay: "1".into(),
                    customize: false,
                    offsets: Default::default(),
                    seed: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(1, |d| d.as_nanos() as u64),
                    key: String::new(),
                    plan: None,
                    error: None,
                });
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    pub(crate) fn clone_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.advanced.clone.take() else {
            return;
        };
        let line_summaries = self.line_summaries();
        let mut open = true;
        let mut apply = false;
        window("Clone events",ctx).open(&mut open).show(ctx,|ui|{ui.set_enabled(!self.modal_open()&&!d.choose);egui::ScrollArea::vertical().id_source("clone-scroll").max_height(ui.available_height().max(90.)).touch_show(ui,|ui|{
            ui.strong(crate::i18n::tr("Source"));if let Some(id)=d.selection.events.first(){let name=&line_summaries[id.line.min(self.editor.document().line_count()-1)].name;ui.label({ let __i18n_arg_0 = &(d.selection.events.len()); let __i18n_arg_1 = &(id.line); let __i18n_arg_2 = &(name); let __i18n_arg_3 = &(id.layer); crate::i18n::format("{0} events · L{1} {2} · Layer {3}", &[__i18n_arg_0.to_string(), __i18n_arg_1.to_string(), __i18n_arg_2.to_string(), __i18n_arg_3.to_string()]) });}
            egui::ComboBox::from_id_source("clone-mode").selected_text(crate::i18n::tr(if d.keep{"Copy · keep source"}else{"Move · remove source"})).touch_show_ui(ui,|ui|{ui.selectable_value(&mut d.keep,true,crate::i18n::tr("Copy · keep source"));ui.selectable_value(&mut d.keep,false,crate::i18n::tr("Move · remove source"));});
            ui.separator();ui.strong(crate::i18n::tr("Destination lines"));if ui.button(crate::i18n::tr("Choose lines…")).clicked(){d.chosen=d.targets.clone();d.page=0;d.choose=true;}
            let mut action=None;for (i,line)in d.targets.iter().take(if d.show_all{256}else{6}).enumerate(){ui.horizontal_wrapped(|ui|{let name=line_summaries.get(*line).map_or_else(||crate::i18n::tr("Missing line"),|l|l.name.clone());ui.label({ let __i18n_arg_0 = &(i+1); let __i18n_arg_1 = &(line); let __i18n_arg_2 = &(name); crate::i18n::format("{0}. L{1} · {2}", &[__i18n_arg_0.to_string(), __i18n_arg_1.to_string(), __i18n_arg_2.to_string()]) });if ui.add_enabled(i>0,egui::Button::new(crate::i18n::tr("Up"))).clicked(){action=Some((i,0));}
if ui.add_enabled(i+1<d.targets.len(),egui::Button::new(crate::i18n::tr("Down"))).clicked(){action=Some((i,1));}
if ui.small_button(crate::i18n::tr("Remove")).clicked(){action=Some((i,2));}});}if let Some((i,action))=action{match action{0=>d.targets.swap(i,i-1),1=>d.targets.swap(i,i+1),_=>{d.targets.remove(i);}}}
            if d.targets.len()>6&&ui.button(crate::i18n::tr(if d.show_all{"Show less"}else{"Show all destinations"})).clicked(){d.show_all = !d.show_all;}
            ui.separator();ui.strong(crate::i18n::tr("Timing"));ui.checkbox(&mut d.delay_enabled,crate::i18n::tr("Delay each next destination"));if d.delay_enabled{ui.horizontal_wrapped(|ui|{ui.label(crate::i18n::tr("Delay (beats)"));ui.text_edit_singleline(&mut d.delay);});}ui.checkbox(&mut d.customize,crate::i18n::tr("Customize value offsets"));
            if d.customize{for (i,o)in d.offsets.iter_mut().enumerate(){ui.push_id(i,|ui|{ui.label(crate::i18n::tr(["Move X","Move Y","Rotate","Alpha"][i]));egui::ComboBox::from_id_source("offset-mode").selected_text(crate::i18n::tr(["Unchanged","Fixed offset","First → last offset"][o.mode])).touch_show_ui(ui,|ui|{for(m,label)in["Unchanged","Fixed offset","First → last offset"].iter().enumerate(){ui.selectable_value(&mut o.mode,m,crate::i18n::tr(*label));}});if o.mode>0{ui.horizontal_wrapped(|ui|{ui.label(crate::i18n::tr("First / fixed"));ui.add(crate::numeric::Value::new(&mut o.from).speed(1.));if o.mode==2{ui.label(crate::i18n::tr("Last"));ui.add(crate::numeric::Value::new(&mut o.to).speed(1.));}});if o.mode==2{egui::ComboBox::from_id_source("offset-easing").selected_text(crate::i18n::tr(crate::authoring_ui::EASINGS[(o.easing-1)as usize])).touch_show_ui(ui,|ui|{for (i,name)in crate::authoring_ui::EASINGS.iter().enumerate(){ui.selectable_value(&mut o.easing,i as i32+1,crate::i18n::tr(*name));}});}ui.checkbox(&mut o.advanced,crate::i18n::tr("Multipliers and variation"));if o.advanced{ui.text_edit_singleline(&mut o.sequence);ui.add(crate::numeric::Value::new(&mut o.variation).prefix(crate::i18n::tr("Variation ± ")));}}});}ui.small(crate::i18n::tr("Speed values are preserved. Offsets change both endpoints; easing and raw extension fields remain intact."));}
            let key=format!("{:?}{:?}{}{}{}{}{}",d.targets,d.offsets,d.keep,d.delay_enabled,d.delay,d.customize,self.editor.revision());if key!=d.key{d.key=key;let result=if d.revision!=self.editor.revision()||d.session!=self.session{Err(ChartError::Invalid("Source changed. Reopen Clone events.".into()))}else{d.spec().and_then(|s|tools::clone_events(self.editor.document(),&d.selection,&s)).and_then(|p|p.validate_preview(self.editor.document()).map(|()|p))};match result{Ok(plan)=>{d.plan=Some(plan);d.error=None;},Err(e)=>{d.plan=None;d.error=Some(e.to_string());}}}
            ui.separator();ui.strong(crate::i18n::tr("Preview"));if let Some(error)=&d.error{ui.colored_label(crate::theme::flick(),crate::i18n::tr(error));}else if let Ok(spec)=d.spec(){ui.label({ let __i18n_arg_0 = &(spec.targets.len()); let __i18n_arg_1 = &(spec.targets.len()*d.selection.events.len()); let __i18n_arg_2 = &(crate::i18n::tr(if d.keep{"Copy"}else{"Move"})); crate::i18n::format("{0} destinations · {1} events · {2}", &[__i18n_arg_0.to_string(), __i18n_arg_1.to_string(), __i18n_arg_2.to_string()]) });let mut shift=Beat::ZERO;for(i,line)in spec.targets.iter().take(12).enumerate(){ui.small({ let __i18n_arg_0 = &(i+1); let __i18n_arg_1 = &(line); let __i18n_arg_2 = &(shift); let __i18n_arg_3 = &(spec.offsets[0][i]); let __i18n_arg_4 = &(spec.offsets[1][i]); let __i18n_arg_5 = &(spec.offsets[2][i]); let __i18n_arg_6 = &(spec.offsets[3][i]); crate::i18n::format("{0}. L{1} · delay {2} · X {3}, Y {4}, Rotate {5}, Alpha {6}", &[__i18n_arg_0.to_string(), __i18n_arg_1.to_string(), __i18n_arg_2.to_string(), format!("{:+.3}", __i18n_arg_3), format!("{:+.3}", __i18n_arg_4), format!("{:+.3}", __i18n_arg_5), format!("{:+.3}", __i18n_arg_6)]) });shift=shift.checked_add(spec.delay).unwrap_or(shift);}}
            apply=ui.add_enabled(d.plan.is_some(),egui::Button::new(crate::i18n::tr(if d.keep{"Copy events"}else{"Move events"}))).clicked();
        });});
        if d.choose {
            let mut chooser_open = true;
            window("Choose destination lines", ctx)
                .id(egui::Id::new("clone-destinations"))
                .open(&mut chooser_open)
                .show(ctx, |ui| {
                    ui.set_enabled(!self.modal_open());
                    ui.add(
                        egui::TextEdit::singleline(&mut d.search)
                            .hint_text(crate::i18n::tr("Search line ID or name")),
                    );
                    ui.label({
                        let __i18n_arg_0 = &(d.chosen.len());
                        crate::i18n::format("{0} lines selected", &[__i18n_arg_0.to_string()])
                    });
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(crate::i18n::tr("All other lines")).clicked() {
                            let source = d.selection.events.first().map(|i| i.line);
                            d.chosen = (0..self.editor.document().line_count())
                                .filter(|i| Some(*i) != source)
                                .take(256)
                                .collect();
                        }
                        if ui.button(crate::i18n::tr("Clear")).clicked() {
                            d.chosen.clear();
                        }
                    });
                    let matches: Vec<_> = line_summaries
                        .iter()
                        .filter(|l| {
                            format!("L{} {}", l.index, l.name)
                                .to_ascii_lowercase()
                                .contains(&d.search.to_ascii_lowercase())
                        })
                        .collect();
                    let pages = matches.len().div_ceil(12).max(1);
                    d.page = d.page.min(pages - 1);
                    egui::ScrollArea::vertical()
                        .id_source("clone-chooser-scroll")
                        .max_height((ui.available_height() - 90.).max(90.))
                        .touch_show(ui, |ui| {
                            for line in matches.iter().skip(d.page * 12).take(12) {
                                let mut selected = d.chosen.contains(&line.index);
                                let source = d
                                    .selection
                                    .events
                                    .first()
                                    .is_some_and(|i| i.line == line.index);
                                if ui
                                    .checkbox(&mut selected, {
                                        let __i18n_arg_0 = &(line.index);
                                        let __i18n_arg_1 = &(line.name);
                                        let __i18n_arg_2 = &(crate::i18n::tr(if source {
                                            " · source"
                                        } else {
                                            ""
                                        }));
                                        crate::i18n::format(
                                            "L{0} · {1}{2}",
                                            &[
                                                __i18n_arg_0.to_string(),
                                                __i18n_arg_1.to_string(),
                                                __i18n_arg_2.to_string(),
                                            ],
                                        )
                                    })
                                    .changed()
                                {
                                    if selected {
                                        if d.chosen.len() < 256 {
                                            d.chosen.push(line.index);
                                        }
                                    } else {
                                        d.chosen.retain(|i| *i != line.index);
                                    }
                                }
                            }
                        });
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(d.page > 0, egui::Button::new(crate::i18n::tr("Previous")))
                            .clicked()
                        {
                            d.page -= 1;
                        }
                        ui.label(format!("{} / {}", d.page + 1, pages));
                        if ui
                            .add_enabled(
                                d.page + 1 < pages,
                                egui::Button::new(crate::i18n::tr("Next")),
                            )
                            .clicked()
                        {
                            d.page += 1;
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(crate::i18n::tr("Use selected lines")).clicked() {
                            d.targets = d.chosen.clone();
                            d.choose = false;
                        }
                        if ui.button(crate::i18n::tr("Cancel")).clicked() {
                            d.choose = false;
                        }
                    });
                });
            if !chooser_open {
                d.choose = false;
            }
        }
        if apply {
            if let Some(plan) = d.plan.take()
                && !self.apply_tool(Ok(plan))
            {
                d.key.clear();
                self.advanced.clone = Some(d);
            }
        } else if open {
            self.advanced.clone = Some(d);
        }
    }
}
