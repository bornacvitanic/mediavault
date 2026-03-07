pub mod scanner;
pub mod models;
pub mod sidecar;
pub mod tmdb;
mod player;
mod library;

pub use models::*;
pub use scanner::{scan_library, is_video};
pub use sidecar::{
    load_movie_state, save_movie_state,
    load_show_bookmarks, save_show_bookmarks,
    load_comments_from_path, save_comments_to_path,
};
pub use player::open_in_player;
pub use library::{resolve_library, looks_like_media_dir};
