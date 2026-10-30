fn main() {
    macroquad::Window::from_config(
        macroquad::prelude::Conf {
            window_title: "MP4 export probe".into(),
            window_width: std::env::var("PHISTUDIO_VIDEO_PROBE_WIDTH")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(960),
            window_height: std::env::var("PHISTUDIO_VIDEO_PROBE_HEIGHT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(720),
            ..Default::default()
        },
        async {
            if let Err(e) = phistudio_app::video_export_probe::run().await {
                eprintln!("{e:#}");
                std::process::exit(1);
            }
            std::process::exit(0);
        },
    );
}
