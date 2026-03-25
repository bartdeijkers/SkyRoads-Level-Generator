# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The initial entry below records the fork baseline from `ammaarreshi/SkyRoads-Codex:main`
at commit `4c59173` on 2026-03-08. `Unreleased` tracks the current local worktree on top of
that baseline.

## [Unreleased]

### Added
- Added `--fullscreen` and `--borderless` launch flags to the SDL host, plus runtime window-mode switching between windowed, borderless, and desktop-fullscreen presentation.
- Added fullscreen and borderless toggles to the in-game controls/settings menu so display mode can be changed without restarting.

### Changed
- Updated the settings-menu renderer to extend the recovered `SETMENU` layout with native fullscreen and borderless toggle widgets that follow the existing white-cursor/orange-selected visual language.
- Updated SDL presentation and mouse-coordinate mapping to preserve the `320x200` framebuffer aspect ratio correctly when fullscreen or borderless modes change the window size.

## [0.2.0] - 2026-03-24

### Added
- Added reverse-engineered DOS mouse and joystick input decoders in `skyroads-core`, plus a gameplay control override path for hosts that need to inject recovered control state.
- Added live settings-menu state for keyboard, joystick, and mouse selection, along with sound effects and music toggles that gate emitted audio commands.
- Added a `--smoke-gameplay` SDL automation mode for intro-to-gameplay smoke testing, plus DOS-style mouse recentering and mouse-control support during gameplay.
- Added reverse-engineering notes for the DOS mouse gameplay path, including the recovered absolute-coordinate thresholds and cursor recenter behavior.

### Changed
- Set the portable crates as the default workspace members so `cargo test` stays SDL-free unless `--workspace` is requested.
- Updated the settings menu renderer to draw the active control-mode and audio-toggle overlays from the original assets instead of a fixed frame.
- Documented the split between portable tests and SDL-dependent runs, including WSL-oriented smoke-test guidance and DOS mouse controls.

### Fixed
- Fixed gameplay ship and shadow placement so steering produces visible on-screen movement while grounded throttle frames stay visually stable.
- Fixed the fallback gameplay projection to stay centered until the exact DOS camera path is ported, avoiding guessed camera motion that canceled visible steering feedback.
- Fixed crash presentation so the game-over overlay waits briefly before covering gameplay, leaving the initial death/explosion frames visible.
- Fixed SDL2 build detection to fail early with clearer installation and environment-override guidance instead of silently falling back to a likely broken link step.

## [0.1.0] - 2026-03-08

### Added
- Forked `ammaarreshi/SkyRoads-Codex:main` as the baseline for a DOS-faithful SkyRoads reverse-engineering and native-port effort.
- Added native loaders and extractors for the original SkyRoads data formats, including `.LZS` archives, `TREKDAT.LZS`, `MUZAX.LZS`, dashboard `*.DAT` packs, `DEMO.REC`, and EXE-embedded HUD assets.
- Added a deterministic Rust core for intro, menu, demo, and gameplay flow, with fixed-step simulation and demo playback aimed at reproducing the shipped DOS behavior.
- Added a CPU reference renderer, native audio path, SDL host, and CLI tooling so original assets can run natively while renderer and audio validation against DOS remains in progress.
- Added reverse-engineering documentation, port architecture notes, DOS capture/extraction tools, and the prompt log that records the project's 1:1 port goals and milestones.
