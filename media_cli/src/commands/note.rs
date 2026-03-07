use crate::fuzzy::{match_entry, print_ambiguous, print_not_found, MatchResult};
use crate::output::Style;
use media_core::sidecar::load_comments_from_path;
use media_core::MediaEntry;
use std::path::Path;

pub fn run(entries: &[MediaEntry], query: &str, show_only: bool) -> Result<(), String> {
    let st = Style::new();

    let entry = match match_entry(entries, query) {
        MatchResult::One(e) => e,
        MatchResult::Many(candidates) => {
            print_ambiguous(query, &candidates);
            return Err("ambiguous title".into());
        }
        MatchResult::None => {
            print_not_found(query);
            return Err("no match".into());
        }
    };

    let comments_path = entry.comments_path();

    if show_only {
        let comments = load_comments_from_path(&comments_path);
        if comments.markdown.trim().is_empty() {
            println!("  {}", st.dim("no notes yet"));
            println!("  hint: run `mediavault-cli note {}` to add some", query);
        } else {
            println!("{}", comments.markdown);
        }
        return Ok(());
    }

    // Open in $EDITOR
    open_in_editor(&comments_path)?;

    println!("  {} notes saved", st.green("✓"));
    Ok(())
}

fn open_in_editor(path: &Path) -> Result<(), String> {
    // Create the file if it doesn't exist so the editor opens cleanly
    if !path.exists() {
        std::fs::write(path, "").map_err(|e| format!("failed to create notes file: {e}"))?;
    }

    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| {
            #[cfg(target_os = "windows")]
            {
                "notepad".to_string()
            }
            #[cfg(not(target_os = "windows"))]
            {
                "nano".to_string()
            }
        });

    let status = std::process::Command::new(&editor)
        .arg(path)
        .status()
        .map_err(|e| format!("failed to launch editor \"{editor}\": {e}\n  hint: set $EDITOR to your preferred editor"))?;

    if !status.success() {
        return Err(format!("editor exited with status {status}"));
    }

    Ok(())
}
