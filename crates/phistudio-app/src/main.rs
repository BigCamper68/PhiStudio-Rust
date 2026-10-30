fn main() {
    #[cfg(target_os = "ios")]
    phistudio_app::ios_main();
    #[cfg(not(target_os = "ios"))]
    phistudio_app::desktop_main();
}
