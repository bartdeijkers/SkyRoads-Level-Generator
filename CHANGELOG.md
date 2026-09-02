# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The initial entry below records the fork baseline from `ammaarreshi/SkyRoads-Codex:main`
at commit `4c59173` on 2026-03-08. `Unreleased` tracks the current local worktree on top of
that baseline.

## [Unreleased]

### Added
- Added hardware-aware SDL display-mode discovery and compact in-game `DISPLAY`
  and `VIDEO MODE` selectors. Exclusive fullscreen choices are exact
  resolution/refresh tuples reported by the active display, including
  `3840x2160 @ 144 Hz` when the platform exposes it.
- Added native display-preference persistence without changing the DOS
  `SKYROADS.CFG` format; explicit launch flags remain one-run overrides.
- Added exact DOS whole-frame fixtures for every shipped road dispatch kind
  `0..5`, all five ship-shadow variants, fallen and explosion progression,
  fresh out-of-oxygen, delayed game over, and the final Road 1 tunnel exit.
- Added explicit gameplay renderer/dashboard context for the renderer counter,
  TREKDAT slot, ship sprite phase, latched gauges, resources, and Jump Master
  state.
- Added normalized DOS debugger memory writes, ordered command barriers, and
  deterministic warm-up checkpoints for resource-death and final-tunnel oracle
  captures.
- Added the recovered `0x32A5` 29-by-33 ship-mask builder and tests for
  unobstructed, left-boundary, right-boundary, and explosion-sentinel behavior.
- Added exact DOS whole-frame regression fixtures for five Road 0 gameplay states: steady neutral, sustained throttle, steady left, steady right, and first visible airborne. Renderer tests compare all 64,000 indexed pixels and 768 VGA palette components and validate fixture lengths before comparison.
- Added held-gameplay-key capture support to the DOS oracle using the recovered `DS:0BA2` key-state table, plus gameplay state/key-state dumps for diagnosing simulation mismatches.
- Added the first committed DOS oracle fixture bundle, `fixtures/dos-gameplay-renderer/road0-initial-frame/renderer_entry/fixture.json`, capturing the original `SKYROADS.EXE` renderer-entry state (registers, renderer tables, draw-dispatch targets, TREKDAT segments, and the VGA frame hash) for Road 0 frame 0 as the ground truth for native renderer equivalence.
- Added `skyroads_data::gameplay_palette(source_root, road_index)`, which assembles the exact 256-entry DOS gameplay palette from the selected road's 72-color bank, `CARS.LZS`'s 20 colors, `DASHBRD.LZS`'s 50 colors, and the selected `WORLD*.LZS` 114-color bank. Live Road 1 and Road 5 DAC captures plus an all-road source-bank test enforce the composition.
- Added a road-data equivalence test verifying the native road loader reproduces the exact DOS renderer road-descriptor window at gameplay start (native road index 1 row 0 == the captured `active_road_window` `[5,0,5,5,5,0,5]`), establishing that the menu "Road 1" maps to native road index 1.
- Added a TREKDAT equivalence test verifying the native TREKDAT expansion reproduces the DOS renderer's active pointer grid for slot 0 (312-word pointer table, head `[624,636,640,…]`, all-nonzero), locking the TREKDAT pipeline that feeds the road rasterizer.
- Added `docs/road-draw-routine.md`, an annotated disassembly of the DOS road draw routine (`0x2D03`): entry/setup, row-group + TREKDAT slot selection, the per-cell dispatch loop (confirming dispatch kind = `(descriptor >> 8) & 0x0F`), the depth-band loop, the ship/HUD composition copy, and draw kind 0. This is the reference for porting the road rasterizer to pixel-equivalence.
- Recovered and committed the DOS Road 0 gameplay VGA palette at `fixtures/dos-gameplay-renderer/road0-gameplay-palette.json` (256 6-bit RGB triples). The assembled 256-color DAC palette was located in the gameplay DS segment at offset `0x5182` (GOMENU base for indices `0..211`, starfield `212..251`), with WORLD0's CMAP overlaid at indices `0..113`; rendering the captured `frame_02` with it is pixel-complete. This resolves the palette half of the whole-frame equivalence blocker. Guarded by a well-formedness test.
- Added committed DOS gameplay-frame fixtures `fixtures/dos-gameplay-renderer/road0-first-five-renderer-frames/frame_00..frame_04`, including the first real, cross-run-deterministic Road 0 gameplay frame (`frame_02`/`frame_03`, hash `1194ff54…`) as the ground truth for a future native whole-frame equivalence test. Determinism was confirmed across two independent capture runs.
- Added `skyroads-data` equivalence tests (`tests/dos_oracle_equivalence.rs`) that read the committed fixtures and assert the native shipped runtime tables reproduce the DOS-captured `tile_class_by_low3` and `draw_dispatch_by_type` (renderer draw kinds `0..5` plus the shared noop) at both the renderer-entry checkpoint and a real gameplay frame, plus that the steady Road 0 frame is stable (`frame_02 == frame_03`) and distinct from the pre-draw menu frame.
- Added automatic DOSBox-X relaunch on a missed capture (`--max-attempts`, default `3`) to `tools/skyroads_dos_oracle.py`, so the wall-clock-timed menu launch no longer fails intermittently when it races emulator startup speed.

### Fixed
- Replaced the approximate eleven-channel music synthesizer with register-level
  YM3812/OPL2 rendering, restored the DOS `0x19E4` music timer, exact instrument
  and volume register writes, and hardware rhythm mode. Gameplay and demo music
  now select all twelve road songs without immediately repeating a song.
- Preserved menu key edges until the next 70 Hz simulation tick so short key
  presses sampled between high-refresh presentation frames are not lost.
- Restored the intro background palette when entering the main menu instead of
  treating `MAINMENU.LZS`'s three-color overlay palette as a full VGA palette.
- Matched the DOS MUZAX dispatcher for single-operator OPL rhythm tracks 7–10,
  including their instrument, key-on/key-off, and volume routing.
- Made borderless desktop fullscreen at the current screen resolution the
  normal launch mode, with safe fallback to windowed presentation when a
  requested mode disappears or cannot be applied.
- Fixed modern-display presentation by requesting vsync, using the renderer's
  high-DPI pixel output for letterboxing, keeping window coordinates for mouse
  mapping, refreshing modes after monitor changes, and recreating the streaming
  texture after SDL renderer resets or runtime mode switches.
- Corrected the hand-written `SDL_Event` FFI union alignment on 64-bit Windows
  and Linux while adding checked SDL display-mode bindings.
- Removed the permissive all-visible ship mask and the road-span sprite-clipping
  heuristic. The exact mask routine now includes the 24-row lift split,
  boundary-side selection, explosion `0x7FFF` sentinel, and DOS's 956-byte
  clear of a 957-byte buffer.
- Fixed terminal rendering so fallen and out-of-resource states retain the DOS
  shadow behavior, explosions use the exact sprite/lift progression, tiny
  positive resource latches keep the first gauge fragment, and empty gauge
  fragments restore the correct dashboard index.
- Fixed final-tunnel bounds handling and added a win-threshold regression for
  Road 1.
- Matched the DOS dashboard's one-frame-latched state, speed-gauge fragments, ship sprite phase and thrust colors, fixed-point horizontal placement, and constant-color airborne shadow masks. These fixes make the five committed Road 0 gameplay checkpoints byte-exact.
- Fixed the `road0-first-five-renderer-frames` oracle preset to use the reliable guest-ADDKEY launch instead of the host-key (powershell) backend, which never drove the DOSBox-X window under WSL and captured zero checkpoints. It now reliably captures five successive renderer frames, including the first real Road 0 gameplay frames (`frame_01`+).
- Documented that the `road0-initial-frame` `renderer_entry` capture's `frame_sha256` is the pre-draw framebuffer (the level-select menu still on screen), not a rendered gameplay frame: the `0824:2D03` breakpoint fires before the row-0 road is blitted. The captured renderer-input tables remain valid; only the frame hash is pre-draw.
- Added `--fullscreen` and `--borderless` launch flags to the SDL host, plus runtime window-mode switching between windowed, borderless, and desktop-fullscreen presentation.
- Added fullscreen and borderless toggles to the in-game controls/settings menu so display mode can be changed without restarting.
- Added a `Shift+Enter` hotkey in the SDL host to toggle fullscreen on and off without sending a normal `Enter` action into the app.

### Changed
- Wired the gameplay renderer to draw through the four exact DOS palette banks
  instead of a partial world CMAP. This restores the road, ship, dashboard, and
  world indices through the same active DAC used by the original.
- Raised the DOS oracle default timeouts (`--time-limit` `30`->`120`, `--checkpoint-timeout` `15`->`60`, `--startup-timeout` `15`->`30`) so the renderer presets, whose menu-launch keys are scheduled out to ~16s, reliably reach and capture the renderer breakpoint under WSL+DOSBox-X instead of silently expiring with zero checkpoints.
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
