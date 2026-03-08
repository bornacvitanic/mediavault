use std::path::Path;

/// Open a video file in the system default player, or print its path when
/// in pipe mode (non-TTY stdout or explicit --path-only).
pub fn open_or_print(path: &Path, path_only: bool) -> Result<(), String> {
    if path_only || !crate::output::is_tty() {
        println!("{}", path.display());
        return Ok(());
    }

    mediavault_core::open_in_player(path);
    Ok(())
}
