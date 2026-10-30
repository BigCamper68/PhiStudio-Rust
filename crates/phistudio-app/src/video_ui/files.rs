#[cfg(target_os = "android")]
pub(crate) fn clear_stale_movies(root: &std::path::Path) {
    if let Ok(entries) = std::fs::read_dir(root.join("export-cache")) {
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with("phistudio-video-")
                && entry.file_type().is_ok_and(|t| t.is_dir())
            {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
}

pub(super) fn movie_name(name: &str) -> String {
    let name = name.trim().strip_suffix(".mp4").unwrap_or(name.trim());
    let clean: String = name
        .chars()
        .filter(|c| !c.is_control() && !"/\\:*?\"<>|".contains(*c))
        .take(120)
        .collect();
    format!(
        "{}.mp4",
        if clean.trim().is_empty() {
            "chart"
        } else {
            clean.trim()
        }
    )
}
