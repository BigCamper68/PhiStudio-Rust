fn main() {
    let density = std::env::var("PHISTUDIO_TEST_DISPLAY_DENSITY")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(1.);
    macroquad::Window::from_config(
        macroquad::prelude::Conf {
            window_title: "Generated text note performance".into(),
            window_width: (1194. * density) as i32,
            window_height: (834. * density) as i32,
            ..Default::default()
        },
        async {
            if let Err(e) = phistudio_app::note_grid_probe::run_text().await {
                eprintln!("{e:#}");
                std::process::exit(1);
            }
            std::process::exit(0);
        },
    );
}
