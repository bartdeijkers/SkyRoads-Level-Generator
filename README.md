# SkyRoads Reverse Engineering + Native Port

This repository is an active reverse-engineering and native-port effort for the original DOS game **SkyRoads**.

The goal is not to make a loose remake. The goal is a **true, deterministic port** that reproduces the shipped DOS build's data formats, simulation, rendering rules, demo playback, and audio timing as closely as possible, then runs that logic natively on macOS and other modern platforms.

## Why This Is Not "Just DOSBox"

DOSBox already solves a different problem: it runs the original DOS executable through emulation.

This project is doing something deeper:

- unpacking and documenting the original resource formats
- disassembling the shipped DOS executable
- tracing the original renderer, gameplay, demo, and audio paths
- rebuilding those systems natively in Rust
- validating the native behavior against the DOS binary

That matters because it unlocks things emulation alone does not:

- native ports for macOS and other platforms
- deterministic tests and regression fixtures
- renderer and gameplay validation against the original binary
- better tooling for modding and inspection
- long-term preservation beyond "hope an emulator still works"

## Current Status

This is **real and playable**, but **not finished** and **not yet fully 1:1**.

Current native build status:

- original game assets are loaded directly from the bundled SkyRoads data files in this workspace
- the shared SkyRoads compression formats are implemented natively
- intro/menu/demo/gameplay flow exists in a native SDL host
- original art, HUD assets, sound effects, and music data are wired into the native app
- live gameplay and recorded demo playback both run natively
- visual debug modes exist for inspecting geometry, row state, and renderer behavior

What is still incomplete:

- the road renderer is much closer now, but it is still being replaced with the exact DOS `TREKDAT` span pipeline
- some ship/foreground clipping and tunnel composition are still being ported from the DOS draw helpers
- some collision/death/audio edge cases are still being tightened against DOS traces
- full frame-accurate and audio-accurate equivalence against the DOS binary is still in progress

In short: this is already past "concept demo" territory, but it is not honest to call it finished or pixel-perfect yet.

## What Has Been Reverse Engineered So Far

### Asset and file formats

The project now has working native parsers or extractors for:

- image `.LZS` archives with `CMAP` and `PICT` chunks
- `ANIM.LZS` wrapper resources
- `ROADS.LZS` road data
- `TREKDAT.LZS` renderer data
- `MUZAX.LZS` music data
- `INTRO.SND` and `SFX.SND`
- `DEMO.REC`
- dashboard `*.DAT` HUD fragment packs
- embedded HUD image data inside `SKYROADS.EXE`

### DOS runtime behavior

The current reverse-engineering baseline includes verified findings for:

- the DOS road draw entrypoint and row selection path
- the 8-slot `TREKDAT` ring buffer setup
- road descriptor dispatch kinds and renderer tables
- demo input decoding and tile-position indexing
- executable startup asset order
- EXE-embedded HUD assets and runtime tables
- death/fall thresholds and core simulation constants
- the DOS ship draw helper chain used for exact sprite/lane selection

### Native port foundation

The Rust workspace now contains:

- `skyroads-data`: exact loaders and binary format parsing
- `skyroads-core`: deterministic gameplay, demo playback, and app state
- `skyroads-renderer-ref`: CPU reference renderer under active DOS-faithful porting
- `skyroads-audio-ref`: native audio path for intro/sample/music scheduling
- `skyroads-sdl`: macOS-first native host
- `skyroads-cli`: verification and inspection commands

## Repository Layout

- `crates/skyroads-data`: native loaders for original SkyRoads files and EXE-derived tables
- `crates/skyroads-core`: deterministic app/gameplay state
- `crates/skyroads-renderer-ref`: reference software renderer
- `crates/skyroads-audio-ref`: reference audio path
- `crates/skyroads-sdl`: SDL host used to run the native build
- `crates/skyroads-cli`: CLI inspection and validation tools
- `tools/skyroads_extract.py`: Python extractor for original DOS assets and structures
- `tools/skyroads_dosbox_trace.py`: DOSBox-X startup/file-I/O tracer
- `tools/skyroads_dos_oracle.py`: DOSBox-X debugger harness for capturing runtime checkpoints
- `docs/reverse-engineering.md`: detailed reverse-engineering notes and verified findings
- `docs/port-architecture.md`: target native-port architecture

## Running It

Requirements:

- Rust toolchain
- SDL2 available on your system
- the bundled SkyRoads DOS data files in this repo, or another equivalent data directory

Useful commands:

```bash
cargo test
cargo test --workspace
cargo run -p skyroads-cli -- summary .
cargo run -p skyroads-cli -- demo-sim . 120
cargo run -p skyroads-sdl -- .
cargo run -p skyroads-sdl -- --smoke-gameplay .
```

Notes:

- `cargo test` runs the portable workspace crates by default and does not require SDL2
- `cargo test --workspace` and `cargo run -p skyroads-sdl -- .` require native SDL2 development files
- if SDL2 lives outside standard search paths, set `SDL2_CONFIG` or `SDL2_LIBS` before building `skyroads-sdl`

If you want to run against a different local SkyRoads data directory, you can pass that path instead of `.`.

### Testing from WSL

If WSLg input/audio is flaky, split testing into two layers:

- portable gameplay/data validation: `cargo test` and `cargo run -p skyroads-cli -- demo-sim . 120`
- SDL host smoke test without a visible window: `SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy cargo run -p skyroads-sdl -- --smoke-gameplay .`

The smoke test drives the native SDL host through intro -> menu -> gameplay automatically, holds throttle for a few gameplay ticks, prints a final gameplay summary, and exits non-zero if it never reaches gameplay.

For a real interactive WSLg session, this still works:

```bash
cargo run -p skyroads-sdl -- .
```

Then focus the WSLg window and use `Space` to skip the intro and `Enter` on `Start` to enter gameplay.

SDL controls:

- `Up / Down`: menu navigation, settings menu, keyboard throttle/brake
- `Left / Right`: steer, settings menu
- `Enter`: select, restart
- `Space`: skip intro, jump, restart
- `Escape`: back to menu
- `Q`: quit
- `Tab`: cycle visual debug modes

The `Controls` menu now follows the recovered DOS structure:

- top row selects `keyboard`, `joystick`, or `mouse` control mode
- bottom row toggles `sound effects` and `music`
- the menu art is composed from `SETMENU` base frame `0`, white cursor overlays `1..5`, and orange selected-state overlays `6..10`

When `mouse` control mode is selected in that menu, the SDL host follows the recovered DOS thresholds:

- move mouse left/right to steer
- move mouse up/down to throttle or brake
- use any mouse button to jump

When `joystick` control mode is selected, the SDL host reads the first SDL joystick/gamepad:

- axis `0`: left/right
- axis `1`: throttle/brake
- button `0`: jump

## Documentation

For the deeper technical record:

- [`docs/reverse-engineering.md`](docs/reverse-engineering.md)
- [`docs/port-architecture.md`](docs/port-architecture.md)
- [`prompt.md`](prompt.md)

## Reverse Engineering / Responsibility

This repository is intended for compatibility, preservation, research, and native-port work on SkyRoads.

Please use it responsibly:

- respect the rights and terms attached to the original game and its assets
- do not assume this repository grants blanket rights beyond what the original distribution allows
- verify redistribution, packaging, and commercial-use rights for your own use case
- treat the reverse-engineering notes and tools here as technical documentation, not legal advice

## Honesty Check

This repository is intentionally documenting the work as it actually happened:

- what has been proven
- what is inferred
- what is implemented natively
- what still has to be ported exactly from the DOS binary

That distinction matters. The interesting part of this project is not "an old game runs." The interesting part is that the original binary is being understood deeply enough to rebuild it natively.
