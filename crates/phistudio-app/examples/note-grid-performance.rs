fn main() {
    macroquad::Window::from_config(
        macroquad::prelude::Conf {
            window_title: "Note grid performance probe".into(),
            window_width: 1280,
            window_height: 800,
            ..Default::default()
        },
        async {
            if let Err(e) = phistudio_app::note_grid_probe::run().await {
                eprintln!("{e:#}");
                std::process::exit(1);
            }
            std::process::exit(0);
        },
    );
}
