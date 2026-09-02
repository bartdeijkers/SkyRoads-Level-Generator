# SkyRoads Reverse Engineering + Native Port

A deterministic Rust port of the original DOS game **SkyRoads**, built from
reverse-engineered data formats and runtime behavior rather than source code.
The project runs natively through SDL2 and validates behavior against the
shipped DOS build.

## Status

The native port is playable but not yet fully 1:1.

- Intro, menus, demo playback, gameplay, graphics, sound effects, and OPL2
  music run natively.
- The runtime loads the original game assets but does not require
  `SKYROADS.EXE` at startup.
- Keyboard, mouse, and one-player controller input are implemented, including
  hot-plug handling and sensitivity settings.
- Renderer coverage includes all road dispatch kinds, ship shadow variants,
  death states, and the final tunnel exit.
- Some collision edge cases and broad frame/audio oracle coverage remain in
  progress.

See [the reverse-engineering baseline](docs/reverse-engineering.md) for verified
findings and [the port architecture](docs/port-architecture.md) for design
boundaries.

## Run the Game

Requirements:

- a Rust toolchain;
- SDL2 2.0.18 or newer, including development files;
- the original SkyRoads data files, which are present in this workspace.

From the repository root:

```bash
cargo run -p skyroads-sdl -- .
```

Normal launches use borderless desktop fullscreen. Use `--windowed` or
`--exclusive-fullscreen` to override that for one run. Pass a different asset
directory instead of `.` when needed.

Packaged-build setup is documented separately for
[Windows](packaging/README-WINDOWS.txt) and
[Linux/WSL](packaging/README-LINUX.txt).

## Controls

| Action | Keyboard | Controller |
| --- | --- | --- |
| Navigate | Arrow keys or WASD | D-pad or left stick |
| Confirm | Enter | South face button or Start |
| Back | Escape | East face button or Back/View |
| Previous/next display value in Controls | — | LB / RB |
| Jump | Space | South face button |
| Quit | Q | Select `QUIT` on the main menu |
| Toggle fullscreen | Shift+Enter | — |
| Cycle debug view | Tab | — |

Controller face buttons are described by position because Xbox,
Nintendo-style, and 8BitDo labels differ. Gamepad navigation works in every
control mode; gamepad movement during gameplay requires `JOYSTICK` in Controls.

To exit using only the controller, return from gameplay to level selection,
return again to the main menu, select `QUIT`, and confirm. Main-menu Back does
not exit the application.

## Input and Display Settings

Controls → `INPUT` adjusts mouse and controller sensitivity from `50%` to
`200%`; `100%` preserves the DOS-derived defaults. Native input preferences are
stored in `SKYROADS-RS-INPUT.CFG` without changing the DOS-compatible
`SKYROADS.CFG`.

The display menu supports windowed, borderless, and SDL-reported exclusive
modes. With `DISPLAY` or `VIDEO MODE` selected in Controls, LB and RB cycle the
value backward and forward. The last successful choice is stored in
`SKYROADS-RS-DISPLAY.CFG`.

Controller support and its current hardware-validation limits are tracked in
[the controller plan](plans/expand-controller-support.md).

## Development Commands

```bash
cargo test
cargo test --workspace
cargo run -p skyroads-cli -- summary .
cargo run -p skyroads-cli -- demo-sim . 120
cargo run -p skyroads-cli -- render-capture . /tmp/skyroads-render-capture
cargo run -p skyroads-sdl -- --smoke-gameplay .
cargo run -p skyroads-sdl -- --smoke-gamepad .
cargo run -p skyroads-sdl -- --controller-diagnostics
```

`cargo test` covers the portable default workspace members. SDL development
files are also required for `cargo test --workspace`. The smoke-gamepad command
injects logical input; it does not prove that SDL can detect physical hardware.

## Repository Layout

- `crates/skyroads-data`: original file formats and recovered runtime tables
- `crates/skyroads-core`: deterministic app and gameplay state
- `crates/skyroads-renderer-ref`: CPU reference renderer
- `crates/skyroads-audio-ref`: sound and OPL2 music playback
- `crates/skyroads-sdl`: native SDL2 host
- `crates/skyroads-cli`: inspection and verification tools
- `docs/`: architecture and reverse-engineering records
- `plans/`: scoped implementation and validation plans
- `packaging/`: platform-specific release instructions

## Documentation

- [Reverse-engineering baseline](docs/reverse-engineering.md)
- [Port architecture](docs/port-architecture.md)
- [Road draw routine](docs/road-draw-routine.md)
- [Executable component diagram](docs/skyroads-exe-component-diagram.md)
- [Controller support and validation](plans/expand-controller-support.md)

## Legal Note

This repository supports compatibility, preservation, research, and native-port
work. The original assets remain subject to their own rights and terms. Verify
redistribution and commercial-use rights independently; these technical notes
are not legal advice.
