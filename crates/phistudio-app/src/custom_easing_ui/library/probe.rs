//! Exercise library controls through the actual Gui adapter and private file worker.
use super::*;
use crate::{
    files::{FileManager, FileRequest},
    gui::Gui,
    runtime::ModalInput,
};
use anyhow::{Result, ensure};
use egui::{Id, Pos2, Rect};
use macroquad::{
    miniquad::{EventHandler, TouchPhase},
    prelude::*,
};
use phistudio_core::{EventType, Language};
use phistudio_prpr::Preview;

#[derive(Clone, Copy)]
enum Aim {
    Templates,
    Name,
    Save,
    Row,
    Export,
    Actions,
    Use,
}
struct Frame {
    target: Rect,
    viewport: Rect,
    window: Rect,
    screen: Rect,
    scale: f32,
    image: Image,
}
struct Probe<'a> {
    gui: &'a mut Gui,
    app: &'a mut Workbench,
    preview: &'a mut Preview,
    files: FileManager,
    modal: ModalInput,
}
impl Probe<'_> {
    async fn frame(&mut self, aim: Aim) -> Frame {
        self.files.update(self.app);
        if self.modal.update(self.app) {
            self.gui.cancel_input();
        }
        self.gui.set_ui_scale(self.app.options.ui_scale);
        set_default_camera();
        clear_background(BLACK);
        let mut info = (
            Rect::NOTHING,
            Rect::NOTHING,
            Rect::NOTHING,
            Rect::NOTHING,
            1.,
        );
        self.gui.run(|ctx, canceled| {
            self.app.sound_ui(ctx, canceled);
            info.2 = crate::window::visible_rect(ctx, Id::new("Custom easing editor"))
                .unwrap_or(Rect::NOTHING);
            info.3 = ctx.screen_rect();
            info.4 = ctx.pixels_per_point();
            ctx.data_mut(|d| {
                let id = match aim {
                    Aim::Templates => Id::new("custom-easing-templates"),
                    Aim::Name => Id::new("curve-template-name"),
                    Aim::Save => Id::new("curve-template-save"),
                    Aim::Row => Id::new(("curve-template-row", "curve-1")),
                    Aim::Export => Id::new("curve-template-export"),
                    Aim::Actions => Id::new("curve-template-actions"),
                    Aim::Use => Id::new("custom-easing-use"),
                };
                info.0 = d.get_temp(id).unwrap_or(Rect::NOTHING);
                info.1 = d
                    .get_temp(Id::new("curve-template-viewport"))
                    .unwrap_or(Rect::NOTHING);
            });
        });
        self.preview.render(0., self.preview.target_viewport());
        self.gui.draw();
        unsafe { macroquad::miniquad::gl::glFinish() };
        let image = crate::ui_layout_probe::read_framebuffer();
        next_frame().await;
        Frame {
            target: info.0,
            viewport: info.1,
            window: info.2,
            screen: info.3,
            scale: info.4,
            image,
        }
    }
    fn touch(&mut self, phase: TouchPhase, pos: Pos2, scale: f32) {
        EventHandler::touch_event(
            self.gui,
            unsafe { get_internal_gl() }.quad_context,
            phase,
            1,
            pos.x * scale,
            pos.y * scale,
            get_time(),
        );
    }
    async fn tap(&mut self, aim: Aim) -> Result<Frame> {
        let f = self.frame(aim).await;
        ensure!(
            f.target.is_positive() && f.screen.contains_rect(f.target),
            "A library control is outside the viewport: {:?}",
            f.target
        );
        self.touch(TouchPhase::Started, f.target.center(), f.scale);
        self.frame(aim).await;
        self.touch(TouchPhase::Ended, f.target.center(), f.scale);
        Ok(self.frame(aim).await)
    }
    async fn reveal(&mut self, aim: Aim) -> Result<Frame> {
        for _ in 0..40 {
            let f = self.frame(aim).await;
            if f.target.is_positive() && f.viewport.contains(f.target.center()) {
                return Ok(f);
            }
            ensure!(
                f.viewport.is_positive(),
                "Template scroll viewport is missing"
            );
            let down = if f.target.center().y < f.viewport.top() {
                1.
            } else {
                -1.
            };
            let dy = f.viewport.height().min(180.) * 0.65 * down;
            let from = f.viewport.center() - egui::vec2(0., dy * 0.5);
            self.touch(TouchPhase::Started, from, f.scale);
            self.frame(aim).await;
            self.touch(TouchPhase::Moved, from + egui::vec2(0., dy), f.scale);
            self.frame(aim).await;
            self.touch(TouchPhase::Ended, from + egui::vec2(0., dy), f.scale);
            self.frame(aim).await;
        }
        anyhow::bail!("Could not reveal the template control with real touch scrolling")
    }
    async fn settle(&mut self) -> Result<()> {
        for _ in 0..300 {
            self.frame(Aim::Use).await;
            if self.app.curve_templates.pending.is_none() && self.app.curve_templates.ready {
                ensure!(
                    self.app.curve_templates.error.is_none(),
                    "{:?}",
                    self.app.curve_templates.error
                );
                return Ok(());
            }
        }
        anyhow::bail!(
            "Curve template worker did not finish: {:?}",
            self.app.curve_templates.error
        )
    }
}
pub(crate) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &Path,
) -> Result<()> {
    let root = tempfile::tempdir()?;
    let id = EventId {
        line: 0,
        layer: 0,
        kind: EventType::MoveX,
        index: 0,
    };
    app.select_event(Some(id));
    let event = app.editor.document().event(id)?;
    let key = DraftKey {
        session: app.session,
        revision: app.editor.revision(),
        id,
        times: [event.start_time.to_string(), event.end_time.to_string()],
        event,
    };
    let original = app.editor.document().as_json().clone();
    let revision = app.editor.revision();
    app.open_custom_easing(key);
    let saved_curve = app.custom_easing.draft.curve.clone();
    let mut p = Probe {
        gui,
        app,
        preview,
        files: FileManager::new(Some(root.path().to_owned())),
        modal: ModalInput::default(),
    };
    for _ in 0..5 {
        p.frame(Aim::Templates).await;
    }
    p.tap(Aim::Templates).await?;
    p.settle().await?;
    p.app.curve_templates.name.clear();
    p.reveal(Aim::Name).await?;
    p.tap(Aim::Name).await?;
    p.gui
        .probe_input(vec![egui::Event::Text("Touch motion".into())], false);
    p.frame(Aim::Save).await;
    ensure!(
        p.app.curve_templates.name == "Touch motion",
        "Template name did not accept focused input"
    );
    p.reveal(Aim::Save).await?;
    p.tap(Aim::Save).await?;
    p.settle().await?;
    ensure!(
        p.app
            .curve_templates
            .prepared
            .as_ref()
            .is_some_and(|p| p.library.templates.len() == 1),
        "A real touch did not save the curve template"
    );
    let f = p.frame(Aim::Use).await;
    f.image
        .export_png(&out.join("library-saved.png").to_string_lossy());
    p.reveal(Aim::Actions).await?;
    p.tap(Aim::Actions).await?;
    p.reveal(Aim::Export).await?;
    p.tap(Aim::Export).await?;
    for _ in 0..300 {
        p.frame(Aim::Use).await;
        if p.app.files.export_package.is_some() {
            break;
        }
    }
    ensure!(
        p.app.files.export_package.is_some(),
        "Template export did not reach the platform save workflow"
    );
    let destination = root.path().join("shared.rpencurve");
    p.app.files.request = Some(FileRequest::WriteExport(destination.clone()));
    for _ in 0..300 {
        p.frame(Aim::Use).await;
        if destination.exists() && !p.app.files.busy {
            break;
        }
    }
    let bytes = std::fs::read(&destination)?;
    ensure!(
        serde_json::from_slice::<serde_json::Value>(&bytes)?["format"] == "rpen-animation-curves",
        "Export is not directly compatible with rpe-next"
    );
    p.app.files.request = Some(FileRequest::MediaPath {
        kind: crate::media::MediaKind::CurveTemplates,
        path: destination.clone(),
    });
    p.frame(Aim::Use).await;
    p.settle().await?;
    ensure!(
        p.app
            .curve_templates
            .prepared
            .as_ref()
            .is_some_and(|p| p.library.templates.len() == 2),
        "Template import did not merge the portable file"
    );
    ensure!(
        destination.exists(),
        "Import removed the external template file"
    );
    p.reveal(Aim::Actions).await?;
    p.tap(Aim::Actions).await?;
    p.app.curve_templates.selected = None;
    p.reveal(Aim::Row).await?;
    p.tap(Aim::Row).await?;
    ensure!(
        p.app.curve_templates.selected.as_deref() == Some("curve-1"),
        "A real template row touch did not select the saved curve"
    );
    for language in Language::ALL {
        p.app.options.language = language;
        p.frame(Aim::Use).await;
        let f = p.frame(Aim::Use).await;
        f.image.export_png(
            &out.join(format!("library-{}.png", language.code()))
                .to_string_lossy(),
        );
        ensure!(
            f.screen.expand(2.).contains_rect(f.window) && f.screen.contains_rect(f.target),
            "Template library leaves viewport in {:?}: {:?}",
            language,
            f.window
        );
    }
    p.app.options.language = Language::English;
    p.frame(Aim::Use).await;
    p.tap(Aim::Use).await?;
    ensure!(
        !p.app.custom_easing.library_view,
        "Use template did not return to the graph"
    );
    ensure!(
        p.app.custom_easing.draft.curve.nodes == saved_curve.nodes
            && !p.app.custom_easing.draft.preserve_domain,
        "Loading a template lost geometry or kept a cropped mapping"
    );
    ensure!(
        p.app.editor.revision() == revision && p.app.editor.document().as_json() == &original,
        "Template management edited the chart or its history"
    );
    p.files
        .checkpoint(p.app, false)
        .map_err(anyhow::Error::msg)?;
    let persisted = CurveLibrary::parse(&std::fs::read(
        root.path().join("custom-easing/templates.json"),
    )?)?;
    ensure!(
        persisted.templates.len() == 2,
        "Template writes were not flushed"
    );
    p.frame(Aim::Use).await;
    p.tap(Aim::Use).await?;
    for _ in 0..300 {
        p.frame(Aim::Use).await;
        if !p.app.custom_easing.open {
            break;
        }
    }
    ensure!(
        !p.app.custom_easing.open && p.app.editor.revision() == revision,
        "Use curve bypassed the event inspector"
    );
    std::fs::write(
        out.join("template-result.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"pass":true,"real_gui_touch":true,"touch_scroll":true,"name_input":true,"private_worker":true,"portable_import_export":true,"chart_history_preserved":true,"flushed":true,"locales":7,"physical_android":false}),
        )?,
    )?;
    Ok(())
}
