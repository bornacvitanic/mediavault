# Changelog

All notable changes to this project will be documented in this file.

## [unreleased]

### Features

- Add OpenSubtitles integration for subtitle fetching

Search and download subtitles from OpenSubtitles.com across all interfaces.
Uses file hash matching with title+year fallback for accurate results.

- core: new opensubtitles module with hash computation, search, and download;
  add opensubtitles_api_key to AppConfig
- cli: new fetch-subs command with --lang, --list, --auto, --episode flags;
  interactive numbered pick when no flags given
- tui: F key fetches subs for selected movie or episode, auto-downloads best
  match with status feedback
- gui: Fetch Subtitles button in movie and show detail panels with background
  thread downloading; OpenSubtitles API key field in Settings; clear status
  on entry switch

- Add subtitle detection for embedded MKV tracks and external files

Detect embedded subtitle tracks in MKV/WebM containers using the
matroska
crate and scan for external subtitle files (.srt, .ass, .ssa, .sub,
.idx,
.vtt) alongside video files. Expose subtitle data across all interfaces:

- core: new subtitles module with extract_subtitle_tracks and
  find_external_subtitles; SubtitleTrack and ExternalSubtitle models
- cli: new `subs` command with per-title and JSON output; subtitle
  info in `get` output
- tui: subtitle count per movie and per episode in detail view
- gui: collapsible subtitle section for movies (collapsed by default),
  per-episode subtitle count badge for shows


## [0.1.3] - 2026-03-08

### Documentation

- Bump all crates to v0.1.3, add Linux support and update roadmap

- Add Linux CI testing alongside Windows in rust.yml
- Add Linux release binaries (tar.gz) alongside Windows (zip) in release.yml
- Update README: remove cross-platform from roadmap (Linux works),
  add subtitle detection/fetching to roadmap, document extras detection,
  show Linux config path
- Bump all four crates from 0.1.2 to 0.1.3
- Update changelog for v0.1.3


### Updates

- Improve movie vs show classification with extras detection

Movies bundled with featurettes/bonus content were incorrectly
classified as shows because the folder contained multiple video files.

Now the scanner:
- Skips known extras folders (Featurettes/, Extras/, Deleted Scenes/,
  etc.) when counting videos for classification
- Falls back to episode-pattern detection (S01E01) when multiple
  non-extras videos exist — no patterns means movie, not show
- Picks the largest file as the main feature when extras aren't in
  a subfolder
- Filters extras folders from season building so they don't appear
  as phantom seasons


## [0.1.2] - 2026-03-08

### Features

- Add release workflow for automated binary distribution


## [0.1.1] - 2026-03-08

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


### Documentation

- Add readme to all crates and bump to v0.1.1

Point all four crates to the workspace root README.md for crates.io
display. Bump version to 0.1.1 to publish the update.

- Update README.md to add images

### Features

- Add crates.io version specs and badge for publishing

- Add generated changelog

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


### Renames

- Rename all crates from media_* to mediavault-* for crates.io publishing

media_core was already taken on crates.io. Rename all four crates
to mediavault-core, mediavault-gui, mediavault-cli, mediavault-tui
for consistent naming. Update all source imports, README, CHANGELOG,
and IDE config.


### Testing

- Update media_cli adn media_core to add tests


### Updates

- Update changelog for v0.1.1 release

- Update README to match project conventions

- Update RustRover meta files

- Update main.rs to update details panel

- Update tmdb.rs for better token recongition

- Update main.rs to redo visual language of the UI interface


