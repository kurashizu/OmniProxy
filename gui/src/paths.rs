use std::path::PathBuf;

/// Resolve the base directory for config, client, proxy binaries.
///
/// Priority:
/// 1. $APPIMAGE parent dir  (AppImage — real filesystem, writable)
/// 2. current_exe() parent  (normal native binary)
/// 3. "."                   (fallback)
pub(crate) fn base_dir() -> PathBuf {
    // AppImage: $APPIMAGE points to the .AppImage file itself
    if let Ok(appimage) = std::env::var("APPIMAGE") {
        if let Some(dir) = PathBuf::from(&appimage).parent() {
            return dir.to_path_buf();
        }
    }
    std::env::current_exe().ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}
