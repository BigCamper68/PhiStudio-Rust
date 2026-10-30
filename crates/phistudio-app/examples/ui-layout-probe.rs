fn main() {
    let size = std::env::args()
        .nth(3)
        .and_then(|s| {
            s.split_once('x')
                .map(|(w, h)| (w.parse::<i32>().ok(), h.parse::<i32>().ok()))
        })
        .and_then(|(w, h)| Some((w?, h?)))
        .unwrap_or((873, 393));
    macroquad::Window::from_config(
        macroquad::prelude::Conf {
            window_title: "UI layout regression probe".into(),
            window_width: size.0,
            window_height: size.1,
            ..Default::default()
        },
        async {
            if let Err(e) = phistudio_app::ui_layout_probe::run().await {
                eprintln!("{e:#}");
                std::process::exit(1);
            }
            std::process::exit(0);
        },
    );
}
