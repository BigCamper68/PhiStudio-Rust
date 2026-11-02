//! Opt-in timings for representative editor hot paths, without a GL dependency.
use super::*;
use serde_json::json;
use std::{hint::black_box, time::Instant};
#[test]
#[ignore = "manual shader-grid timing"]
fn shader_grid_performance() {
    let mut value = ChartDocument::empty("Grid benchmark", 120.)
        .unwrap()
        .as_json()
        .clone();
    let keys = (0..100)
        .map(
            |n| json!({"startTime":[n,0,1],"endTime":[n+1,0,1],"start":0.,"end":1.,"easingType":1}),
        )
        .collect::<Vec<_>>();
    let effect = json!({"shader":"chromatic","start":[0,0,1],"end":[4,0,1],"vars":{"power":keys}});
    value[phistudio_core::shaders::EXTRA_KEY] =
        json!({"bpm":120.,"effects":(0..150).map(|_|effect.clone()).collect::<Vec<_>>()});
    let mut app = Workbench::new(ChartDocument::from_value(value).unwrap(), None);
    app.advanced.right_mode = crate::advanced_ui::RightMode::Shaders;
    app.shaders.selected = Some(0);
    app.refresh_shaders();
    let ctx = egui::Context::default();
    let draw = || {
        black_box(ctx.run(Default::default(), |ctx| {
            let grid = crate::grid_input::geometry(
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 900.)),
                &app,
            );
            crate::shader_ui::grid::paint(
                &app,
                &ctx.layer_painter(egui::LayerId::background()),
                grid,
            );
        }));
    };
    draw();
    let start = Instant::now();
    for _ in 0..40 {
        draw();
    }
    eprintln!(
        "PERF shader_grid effects=150 keys_per_effect=100 frames=40 ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.
    );
}
