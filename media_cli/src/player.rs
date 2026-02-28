use std::path::Path;

/// Open a video file in the system default player.
/// On non-tty stdout (pipe mode), prints the path instead.
pub fn open_or_print(path: &Path, path_only: bool) -> Result<(), String> {
    if path_only || !crate::output::is_tty() {
        println!("{}", path.display());
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &path.to_string_lossy()])
            .spawn()
            .map_err(|e| format!("failed to open player: {e}"))?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("failed to open player: {e}"))?;
    }

    Ok(())
}
