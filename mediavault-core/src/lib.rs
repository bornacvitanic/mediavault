mod library;
pub mod models;
pub mod opensubtitles;
mod player;
pub mod scanner;
pub mod sidecar;
pub mod subtitles;
pub mod tmdb;

pub use library::{looks_like_media_dir, resolve_library};
pub use models::*;
pub use player::open_in_player;
pub use scanner::{is_video, scan_library};
pub use sidecar::{
    load_comments_from_path, load_movie_state, load_show_bookmarks, save_comments_to_path,
    save_movie_state, save_show_bookmarks,
};
pub use subtitles::{extract_subtitle_tracks, find_external_subtitles};
