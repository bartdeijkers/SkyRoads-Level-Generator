# SkyRoads Reverse Engineering + Native Port

This repository is an active reverse-engineering and native-port effort for the original DOS game **SkyRoads**.

The goal is not to make a loose remake. The goal is a **true, deterministic port** that reproduces the shipped DOS build's data formats, simulation, rendering rules, demo playback, and audio timing as closely as possible, then runs that logic natively on Windows, Linux/WSL, macOS, and other modern platforms.

## Why This Is Not "Just DOSBox"

DOSBox already solves a different problem: it runs the original DOS executable through emulation.

This project is doing something deeper:

- unpacking and documenting the original resource formats
- disassembling the shipped DOS executable
- tracing the original renderer, gameplay, demo, and audio paths
- rebuilding those systems natively in Rust
- validating the native behavior against the DOS binary

That matters because it unlocks things emulation alone does not:

- native ports for Windows, Linux/WSL, macOS, and other platforms
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
- the native Rust runtime no longer requires `SKYROADS.EXE` to be present at startup
- live gameplay and recorded demo playback both run natively
- the SDL host has one-player logical gamepad navigation, hot-plug handling,
  deterministic gameplay normalization, and live mouse/controller sensitivity
  tuning; one SN30 Pro Bluetooth route has partial physical evidence, but no
  required controller acceptance row is complete
- visual debug modes exist for inspecting geometry, row state, and renderer behavior

What is still incomplete:

- the gameplay renderer now has DOS-exact fixtures for all six road dispatch
  kinds, all five shadow variants, terminal states, explosion progression,
  delayed game over, and the final tunnel exit, but equivalent coverage is not
  yet complete for every frame of every road
- the exact ship-mask routine is ported and its boundary branches are covered,
  while additional live obstruction captures would still broaden the oracle set
- some collision/death edge cases are still being tightened against DOS traces
- MUZAX now drives a real register-level OPL2 core at the recovered DOS timer
  rate, with exact instrument, volume, note, and rhythm register semantics;
  broader full-song audio-oracle comparisons and full-game frame traces remain
  in progress

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
- `skyroads-sdl`: cross-platform SDL2 host for Windows and Linux/WSL
- `skyroads-cli`: verification and inspection commands

## Repository Layout

- `crates/skyroads-data`: native loaders for original SkyRoads files plus built-in shipped runtime tables
- `crates/skyroads-core`: deterministic app/gameplay state
- `crates/skyroads-renderer-ref`: reference software renderer
- `crates/skyroads-audio-ref`: reference audio path
- `crates/skyroads-sdl`: SDL host used to run the native build
- `crates/skyroads-cli`: CLI inspection and validation tools
- `packaging`: canonical Windows and Linux release-archive instructions
- `tools/skyroads_extract.py`: Python extractor for original DOS assets and structures
- `tools/skyroads_dosbox_trace.py`: DOSBox-X startup/file-I/O tracer
- `tools/skyroads_dos_oracle.py`: DOSBox-X debugger harness for capturing runtime checkpoints
- `docs/reverse-engineering.md`: detailed reverse-engineering notes and verified findings
- `docs/port-architecture.md`: target native-port architecture

## Running It

Requirements:

- Rust toolchain
- SDL2 2.0.18 or newer available on your system
- the bundled SkyRoads DOS data files in this repo, or another equivalent data directory

For the native Rust app and CLI, `SKYROADS.EXE` is no longer required at runtime.

Useful commands:

```bash
cargo test
cargo test --workspace
cargo run -p skyroads-cli -- summary .
cargo run -p skyroads-cli -- demo-sim . 120
cargo run -p skyroads-cli -- render-capture . /tmp/skyroads-render-capture
cargo run -p skyroads-cli -- render-capture . /tmp/skyroads-render-capture --x265-video /tmp/skyroads-render-capture.mp4
cargo run -p skyroads-cli -- render-demo . /tmp/skyroads-render-demo --x265-video /tmp/skyroads-render-demo.mp4
cargo run -p skyroads-cli -- render-compare /tmp/skyroads-render-capture /tmp/skyroads-render-capture
cargo run -p skyroads-sdl -- .
cargo run -p skyroads-sdl -- --windowed .
cargo run -p skyroads-sdl -- --fullscreen .
cargo run -p skyroads-sdl -- --exclusive-fullscreen .
cargo run -p skyroads-sdl -- --smoke-gameplay .
cargo run -p skyroads-sdl -- --smoke-gamepad .
cargo run -p skyroads-sdl -- --controller-diagnostics
```

Notes:

- `cargo test` runs the portable workspace crates by default and does not require SDL2
- `cargo test --workspace` and `cargo run -p skyroads-sdl -- .` require native SDL2 development files
- `cargo run -p skyroads-cli -- render-capture . DIR` writes a deterministic ship-focused gameplay frame suite as `.ppm` files plus `manifest.tsv`
- `cargo run -p skyroads-cli -- render-capture . DIR --x265-video VIDEO.mp4` also streams the captured `.ppm` frames into `ffmpeg` and writes a single H.265/HEVC video with `libx265`; add `--video-fps FPS` to override the default `30`
- `cargo run -p skyroads-cli -- render-demo . DIR --x265-video VIDEO.mp4` renders the actual attract-mode gameplay demo as `.ppm` frames plus `manifest.tsv`, and can encode the same frame sequence to x265/HEVC in one run
- `cargo run -p skyroads-cli -- render-compare BEFORE AFTER` compares two capture manifests by labeled frame hash
- `cargo run -p skyroads-sdl -- --controller-diagnostics` initializes SDL input without loading game assets, lists each device's mapped/raw status, GUID, mapping, axis/button counts, and probe instance ID, reports the selected device, and prints normalized logical input changes; stop it with `Ctrl+C`
- `cargo run -p skyroads-sdl -- --smoke-gamepad .` injects a logical gamepad sequence through the menu latch and gameplay conversion path; it does not test SDL discovery, mappings, hot-plugging, or physical hardware
- if SDL2 lives outside standard search paths, set `SDL2_CONFIG` or `SDL2_LIBS` before building `skyroads-sdl`
- the optional x265 export path expects `ffmpeg` with `libx265` support on your `PATH`
- normal launches default to borderless desktop fullscreen at the screen's current resolution and refresh rate; `--fullscreen` and `--borderless` remain aliases for that mode
- `--exclusive-fullscreen` prefers the desktop resolution at its highest reported refresh rate, then falls back to a suitable hardware-reported mode
- `--windowed` uses a centered `1280x960` window
- the in-game `DISPLAY` and `VIDEO MODE` choices switch between windowed, borderless desktop, and exact hardware-reported resolution/refresh tuples such as `3840X2160 144HZ`
- the last successfully applied in-game display setting is restored from `SKYROADS-RS-DISPLAY.CFG`; command-line display flags are one-run overrides
- the deterministic game simulation remains at `70 Hz`; the selected display refresh rate and vsync control presentation, not gameplay speed

If you want to run against a different local SkyRoads data directory, you can pass that path instead of `.`.

### Testing from WSL

If WSLg input/audio is flaky, split testing into two layers:

- portable gameplay/data validation: `cargo test` and `cargo run -p skyroads-cli -- demo-sim . 120`
- SDL host smoke test without a visible window: `SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy cargo run -p skyroads-sdl -- --smoke-gameplay .` (smoke mode defaults to windowed so dummy drivers do not need fullscreen support)
- injected controller-flow test: `SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy cargo run -p skyroads-sdl -- --smoke-gamepad .`

The gameplay smoke test drives the native SDL host through intro -> menu ->
gameplay automatically, holds throttle for a few gameplay ticks, prints a final
gameplay summary, and exits non-zero if it never reaches gameplay.

The gamepad smoke separately drives intro -> menu -> level selection -> gameplay
with logical gamepad snapshots. It proves the device-independent edge/held input
flow, but a passing result says nothing about whether WSL can see a physical
controller.

For a real interactive WSLg session, this still works:

```bash
cargo run -p skyroads-sdl -- .
```

Then focus the WSLg window and use `Space` to skip the intro and `Enter` on `Start` to enter gameplay.

SDL controls:

- `Up / Down`: menu navigation, settings menu, keyboard throttle/brake
- `Left / Right`: steer, settings menu
- `Enter`: select, restart
- `Shift+Enter`: toggle windowed/the most recently used fullscreen mode
- `Space`: skip intro, jump, restart
- `Escape`: back to menu
- `Q`: quit; alternatively select `QUIT` from the main menu
- `Tab`: cycle visual debug modes

The `Controls` menu now follows the recovered DOS structure:

- top row selects `keyboard`, `joystick`, or `mouse` control mode
- bottom row toggles `sound effects` and `music`
- the native extension row selects `DISPLAY` (`WINDOWED`, `BORDERLESS`, or `EXCLUSIVE`) and an exact SDL-reported `VIDEO MODE`
- the native `INPUT` entry opens mouse/controller sensitivity controls and live activation previews
- the menu art is composed from `SETMENU` base frame `0`, white cursor overlays `1..5`, and orange selected-state overlays `6..10`

SDL only offers modes exposed by the active platform display driver. Native Windows can therefore expose a physical `3840x2160 @ 144 Hz` mode while WSLg may expose the same desktop at roughly `60 Hz`; the game never invents a mode the driver did not report.

When `mouse` control mode is selected in that menu, the SDL host follows the recovered DOS thresholds:

- move mouse left/right to steer
- move mouse up/down to throttle or brake
- use any mouse button to jump

### Controller controls

Gamepad menu navigation works in every control mode. Gameplay movement from a
gamepad is used only when `JOYSTICK` is selected in the Controls menu.

Controls are documented by position and intent because the printed face-button
letters vary between Xbox, Nintendo-style, and 8BitDo modes:

| Context | Physical control | Action |
| --- | --- | --- |
| Intro and menus | D-pad or left stick | Navigate |
| Intro and menus | South face button or Start | Confirm/select |
| Menus and gameplay | East face button or Back/View | Go back |
| Main menu | Select `QUIT`, then South or Start | Quit |
| Gameplay | D-pad left/right or left-stick X | Steer |
| Gameplay | D-pad/stick up or right trigger | Accelerate |
| Gameplay | D-pad/stick down or left trigger | Brake |
| Gameplay | South face button | Jump; retry after a crash; select after a win |

The DOS-exact terminal paths do not add separate retry or win prompts. After an
explosion or fall, fully release the south face button and press it again to
retry. After the final tunnel scene settles, release the button and press it
again to return to level selection.

The host selects one active input device. It prefers the first SDL-mapped game
controller and uses the first raw joystick only when no mapped controller is
available. The raw fallback retains axis `0` for steering, axis `1` for
acceleration/braking, and button `0` for jump; exact raw numbering remains
device-specific.

Additional devices are ignored while the selected device remains connected.
Unplugging the selected device produces neutral input before the next
simulation tick and triggers a rescan; plugging in while none is selected also
triggers a rescan. Startup and device changes print the selected name,
mapped/raw status, and stable instance ID. The game continues without a
controller and warns only upon entering gameplay with `JOYSTICK` selected.

### Input sensitivity

Open Controls -> `INPUT` to adjust `MOUSE SENSITIVITY` and `CONTROLLER
SENSITIVITY`. Each value ranges from `50%` through `200%` in `5%` steps:

- `100%` is the DOS-faithful default
- a higher percentage engages an axis or trigger closer to its center/rest position
- a lower percentage requires more travel, while full deflection remains reachable
- changes apply immediately, and the page previews mouse axes plus controller stick/trigger activation
- `RESET TO 100%` restores both defaults

Valid changes are stored in `SKYROADS-RS-INPUT.CFG` beside the game assets. The
file is a native-host preference and does not change the DOS-compatible
`SKYROADS.CFG`. A missing file uses `100% / 100%`; malformed, incomplete, or
out-of-range content emits one warning and atomically falls back to those
defaults.

### Controller setup and current validation boundary

The following routes remain implementation targets. The partial physical
evidence below is not a completed hardware-support claim:

- On native Windows 11, direct Xbox controllers should use SDL's mapped
  controller path over USB or Bluetooth.
- For an 8BitDo SN30 Pro connected directly to Windows, start it in X-input
  mode with `X + Start` before connecting by USB or Bluetooth. Interpret face
  buttons by position, not by their printed letters.
- For Steam Input, launch the native Windows build through the Windows Steam
  client and configure gamepad emulation/legacy mode. The intended result is one
  Xbox-style logical controller. The native Steam Input API, action manifests,
  and Steam-specific glyphs are not implemented.
- The SN30 Pro's Switch mode is targeted only through Steam Input emulation.
  Direct D-input or Switch mode without Steam is best effort and requires an SDL
  mapping; there is no device-name special case.

Steam's Windows virtual controller is not forwarded into WSL. WSLg provides
display and audio integration, but it does not make Windows-paired USB devices
available to Linux. For the WSL path, use a wired controller and
[`usbipd-win`](https://learn.microsoft.com/windows/wsl/connect-usb):

1. In an elevated Windows terminal, run `usbipd list`, note the controller's
   bus ID, then run `usbipd bind --busid <BUSID>` once.
2. Run `usbipd attach --wsl --busid <BUSID>` while the WSL distribution is
   running.
3. In WSL, verify the device with `lsusb`, then verify `/dev/input` exists and
   that the current user can read the controller device before starting the
   game or diagnostics command.

While a USB device is attached to WSL, Windows cannot use that same device.
Forwarding and Linux device permissions remain host setup responsibilities; the
game reports no detected controller and continues with keyboard input when the
guest cannot see one.

As of 2026-09-02, a Windows x86-64 release using SDL 2.32.10 passes native
startup diagnostics and both injected dummy-driver smoke paths on Windows build
10.0.26200.9278. With Steam stopped, Windows identified an 8BitDo SN30 Pro over
Bluetooth and SDL exposed one mapped `Xbox One S Controller` with GUID
`030044f05e040000e002000000007200`. An isolated run persisted `JOYSTICK`, the
user confirmed D-pad/left-stick menu navigation, confirm/back, steering, both
triggers, jump, and non-repeating held menu navigation, and mapped reselection
was observed after the device temporarily disappeared. Controller sensitivity
at `50%`, `100%`, and `200%`, restart restoration, and reset were also checked
live and against the isolated preference file. Powering off while steering and
accelerating released both inputs, and reconnecting restored control without a
game restart. A fresh South press after a crash restarted the selected road,
East/Back exited gameplay to level selection, and a fresh South press after the
final tunnel returned to level selection. Firmware and USB comparison remain
unverified, so this is partial evidence and no required row is complete. The
current Ubuntu 24.04 WSL2 environment also has no `/dev/input`,
so both WSL controller rows are environment-blocked; WSLg mouse tuning remains
unverified separately. No repository-owned controller mapping is shipped. Run
`--controller-diagnostics` and record the OS, SDL runtime, connection mode,
controller firmware, mapped/raw status, and selected instance ID before making
a device-support claim. Automated smoke tests do not replace this hardware
matrix.

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
