# Changelog

All notable changes to this project will be documented in this file.

## [unreleased]

### Bug Fixes

- Apply cargo clippy fixes and cargo fmt

clippy:
- media_core: use Error::other(), strip_prefix(), eq_ignore_ascii_case(), redundant closures
- media_tui: replace manual .min().max() with .clamp()
- media_cli: #[allow(too_many_arguments)] on run(), fix redundant closures and lifetimes
- media_gui: remove redundant let rebind, #[allow] for type_complexity and too_many_arguments,
  change &mut Vec to &mut [_] in render_movie_detail / render_show_detail

fmt: reformat all crates to rustfmt style

- Fix all compiler warnings

- Remove unused ListState import in detail.rs
- Remove unreachable Char('k') match arm in input.rs (already handled above)
- Remove no-op drop() on reference in app.rs
- Add explicit '_ lifetime to make_list_item return type in library.rs
- Add '_ to items_slice vis parameter for clarity
- Add #[allow(dead_code)] to PosterLoaded.base_dir field (keyed by poster_path intentionally)
- Add #[allow(dead_code)] to ensure_poster, open_detail, find_movie_mut, find_show_mut (planned helpers)

- Rename binaries to mediavault-cli and mediavault-tui

Rename mv to mediavault-cli and mvt to mediavault-tui to avoid
shadowing Unix mv command and to follow consistent naming. Fix CLI
package name from mediavault to media_cli to avoid Cargo collision
with the GUI crate. Update all help text and hint messages.

- Update sidecar.rs to fix comments path inconsistency

Remove load_comments/save_comments (base_dir variants) and unused
comments_path_dir/comments_path_video helpers. All callers now use
MediaEntry::comments_path() with load/save_comments_from/to_path.

- Update main.rs to fix marking of movies as watched


### Features

- Add git-cliff changelog configuration

- Add GitHub Actions CI workflow

- Add package metadata to all workspace crates

- Add MIT license

- Add TestData

- Add media_tui

- Update media_cli to add querying capability for automation

- Add media_cli

- Update tmdb.rs to add optional poster showing and auto mark as watched

- Update main.rs to add more sort options

- Update main.rs to add zoom and tag extraction


### Moves

- Update media_core to de-duplicate shared logic from frontends

Move open_in_player and resolve_library/looks_like_media_dir into
media_core so all three frontends (GUI, CLI, TUI) share a single
implementation instead of maintaining separate copies.


### Removals

- Remove unused dead code in media_gui

- Remove base_dir field from PosterLoaded (textures are keyed by poster_path,
  base_dir was a leftover from an earlier design)
- Remove ensure_poster() method (poster fetching is done inline in update())
- Remove open_detail() method (detail opening is done inline in update())
- Remove find_movie_mut() and find_show_mut() helpers (never called; all
  mutation sites use inline iter_mut().find_map() already)
- Remove now-unused Movie and Show imports
- Simplify poster spawn loop to iterate by index since base_dirs is gone


### Testing

- Update media_cli adn media_core to add tests


### Updates

- Update RustRover meta files

- Update main.rs to update details panel

- Update tmdb.rs for better token recongition

- Update main.rs to redo visual language of the UI interface


