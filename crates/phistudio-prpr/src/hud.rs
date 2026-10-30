//! Gameplay HUD from prpr GameScene::ui, revision 0afd166 (GPL-3.0-only).
//! Draw geometry, fonts, sizes and attachUI calls are retained verbatim.
//! The editor supplies a steady playing state; its transport handles pause input.
use macroquad::prelude::*;
use prpr::{
    core::{Chart, PGR_FONT, Point, Resource, UIElement},
    ext::semi_white,
    judge::Judge,
    ui::Ui,
};

pub(crate) fn render(ui: &mut Ui, chart: &Chart, judge: &Judge, res: &Resource) {
    let p = 1.;
    let eps = 2e-2 / res.aspect_ratio;
    let top = -1. / res.aspect_ratio;
    let pause_w = 0.015;
    let pause_h = pause_w * 3.2;
    let pause_center = Point::new(
        pause_w * 4.0 - 1.,
        top + eps * 3.5 - (1. - p) * 0.4 + pause_h / 2.,
    );
    ui.alpha(res.alpha, |ui| {
        ui.text("MAGIC BUGFIX TEXT")
            .color(Color::new(0., 0., 0., 0.))
            .draw();

        let margin = 0.03;

        let legacy_aui = !res.info.use_attach_ui_fix.unwrap_or_default();
        let unit_h = if legacy_aui {
            ui.text("0").measure_using(&PGR_FONT).h
        } else {
            0.
        };

        // score
        let h = 0.07;
        let score_top = top + eps * 2.2 - (1. - p) * 0.4;
        let score_right = 1. - margin;
        let score = format!("{:07}", judge.score());
        let scale_point = legacy_aui.then(|| {
            let ct = ui.text(&score).size(0.8).measure_using(&PGR_FONT).center();
            (score_right - ct.x, score_top + ct.y)
        });
        chart.with_element(
            ui,
            res,
            UIElement::Score,
            scale_point,
            (score_right, score_top),
            |ui, c| {
                ui.text(&score)
                    .pos(score_right, score_top)
                    .anchor(1., 0.)
                    .size(0.8)
                    .color(c)
                    .draw_using(&PGR_FONT);
                if res.config.show_acc {
                    ui.text(format!("{:05.2}%", judge.real_time_accuracy() * 100.))
                        .pos(1. - margin, score_top + h)
                        .anchor(1., 0.)
                        .size(0.4)
                        .color(Color { a: c.a * 0.7, ..c })
                        .draw_using(&PGR_FONT);
                }
            },
        );

        chart.with_element(
            ui,
            res,
            UIElement::Pause,
            legacy_aui.then(|| (pause_center.x, pause_center.y)),
            (
                pause_center.x - pause_w * 1.5,
                pause_center.y - pause_h / 2.,
            ),
            |ui, c| {
                let mut r = Rect::new(
                    pause_center.x - pause_w * 1.5,
                    pause_center.y - pause_h / 2.,
                    pause_w,
                    pause_h,
                );
                ui.fill_rect(r, c);
                r.x += pause_w * 2.;
                ui.fill_rect(r, c);
            },
        );
        if judge.combo() >= 3 {
            if legacy_aui {
                let combo_top = top + eps * 2. - (1. - p) * 0.4;
                let btm = chart.with_element(
                    ui,
                    res,
                    UIElement::ComboNumber,
                    None,
                    (0., combo_top + unit_h / 2.),
                    |ui, c| {
                        ui.text(judge.combo().to_string())
                            .pos(0., combo_top)
                            .anchor(0.5, 0.)
                            .color(c)
                            .draw_using(&PGR_FONT)
                            .bottom()
                    },
                );
                let combo_top = btm + 0.01;
                chart.with_element(
                    ui,
                    res,
                    UIElement::Combo,
                    None,
                    (0., combo_top + unit_h * 0.2),
                    |ui, c| {
                        ui.text(if res.config.autoplay() {
                            "AUTOPLAY"
                        } else {
                            "COMBO"
                        })
                        .pos(0., combo_top)
                        .anchor(0.5, 0.)
                        .size(0.4)
                        .color(c)
                        .draw_using(&PGR_FONT);
                    },
                );
            } else {
                let combo = judge.combo().to_string();
                let ct = ui.text(&combo).size(1.0).measure().center();
                let combo_y = top + eps * 2. - (1. - p) * 0.4 + ct.y;
                let btm = chart.with_element(
                    ui,
                    res,
                    UIElement::ComboNumber,
                    None,
                    (0., combo_y),
                    |ui, c| {
                        ui.text(&combo)
                            .pos(0., combo_y)
                            .anchor(0.5, 0.5)
                            .size(1.0)
                            .color(c)
                            .draw_using(&PGR_FONT)
                            .bottom()
                    },
                );
                let ct = ui.text("COMBO").size(0.4).measure().center();
                let combo_top = btm + 0.01 + ct.y;
                chart.with_element(ui, res, UIElement::Combo, None, (0., combo_top), |ui, c| {
                    ui.text(if res.config.autoplay() {
                        "AUTOPLAY"
                    } else {
                        "COMBO"
                    })
                    .pos(0., combo_top)
                    .anchor(0.5, 0.5)
                    .size(0.4)
                    .color(c)
                    .draw_using(&PGR_FONT);
                });
            }
        }
        // magic to make score visible, refer to phira/src/rate.rs#L219
        ui.text("").draw_using(&PGR_FONT);
        let lf = -1. + margin;
        let bt = -top - eps * 2.8 + (1. - p) * 0.4;
        let scale_point = legacy_aui.then(|| {
            let ct = ui.text(&res.info.name).size(0.5).measure().center();
            (lf + ct.x, bt - ct.y)
        });
        chart.with_element(ui, res, UIElement::Name, scale_point, (lf, bt), |ui, c| {
            ui.text(&res.info.name)
                .pos(lf, bt)
                .anchor(0., 1.)
                .size(0.5)
                .color(c)
                .max_width(0.8)
                .draw();
        });

        let scale_point = legacy_aui.then(|| {
            let ct = ui.text(&res.info.level).size(0.5).measure().center();
            (-lf - ct.x, bt - ct.y)
        });
        chart.with_element(
            ui,
            res,
            UIElement::Level,
            scale_point,
            (-lf, bt),
            |ui, c| {
                ui.text(&res.info.level)
                    .pos(-lf, bt)
                    .anchor(1., 1.)
                    .size(0.5)
                    .color(c)
                    .draw();
            },
        );

        let hw = 0.003;
        let height = eps * 1.0;
        let dest = (2. * res.time / res.track_length).clamp(0., 2.) as f32;
        chart.with_element(
            ui,
            res,
            UIElement::Bar,
            Some((-1., top + height / 2.)),
            (-1., top + height / 2.),
            |ui, _color| {
                ui.fill_rect(Rect::new(-1., top, dest, height), semi_white(0.6));
                ui.fill_rect(Rect::new(-1. + dest - hw, top, hw * 2., height), WHITE);
            },
        );
    });
}

/// Static return target at the native HUD's default pause position.
/// Deliberately independent of chart attachUI animation and visibility.
/// Coordinates are normalized to the cropped preview, top-left origin.
pub(crate) fn pause_bounds(res: &Resource) -> [f32; 4] {
    let w = 0.015;
    let h = w * 3.2;
    let center = (
        w * 4. - 1.,
        -1. / res.aspect_ratio + 0.02 / res.aspect_ratio * 3.5 + h / 2.,
    );
    [
        (center.0 - w * 1.5 + 1.) / 2.,
        (center.1 - h / 2.) * res.aspect_ratio / 2. + 0.5,
        (center.0 + w * 1.5 + 1.) / 2.,
        (center.1 + h / 2.) * res.aspect_ratio / 2. + 0.5,
    ]
}
