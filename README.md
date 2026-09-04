# SkyRoads Level Generator

A native Rust/SDL2 port of **SkyRoads**, the DOS game by Bluemoon Interactive,
with deterministic procedural roads and shareable generation IDs. Play the
original campaign or create a new road on Easy, Classic, or Hard.

The port reconstructs the game's data formats and runtime behavior through
reverse engineering. Original graphics, sound effects, and OPL2 music run
natively; DOSBox and `SKYROADS.EXE` are not needed to play.

## Download and Play

Download a package from **[release v1.0.0](https://github.com/bartdeijkers/SkyRoads-Level-Generator/releases/tag/v1.0.0)**:

| Platform | Download | Runtime setup |
| --- | --- | --- |
| Windows x64 (Intel/AMD) | [ZIP](https://github.com/bartdeijkers/SkyRoads-Level-Generator/releases/download/v1.0.0/SkyRoads-Rust-1.0.0-windows-x64.zip) | SDL2 is included; keep `SDL2.dll` beside the executable. |
| Linux x64 (Intel/AMD) | [tar.gz](https://github.com/bartdeijkers/SkyRoads-Level-Generator/releases/download/v1.0.0/SkyRoads-Rust-1.0.0-linux-x64.tar.gz) | Ubuntu 24.04 or compatible, with the SDL2 runtime. |
| Linux ARM64 (AArch64) | [tar.gz](https://github.com/bartdeijkers/SkyRoads-Level-Generator/releases/download/v1.0.0/SkyRoads-Rust-1.0.0-linux-arm64.tar.gz) | Ubuntu 24.04 or compatible, with the SDL2 runtime. |

Packages include the native executable, setup notes, and dependency notices.
You do not need Rust to use them. The release also provides
[SHA-256 checksums](https://github.com/bartdeijkers/SkyRoads-Level-Generator/releases/download/v1.0.0/SHA256SUMS.txt).

**Original game data is required and is not included, even for procedural mode.**
Obtain the full original game separately and follow `GAME-DATA.txt` in the
package, also available in the [data setup guide][game-data]. Use the original
full edition, not the demo or Christmas edition, and preserve uppercase data
filenames on Linux.

Extract the native package, then open a terminal in its directory. Pass the
directory containing your original game data to the executable.

**Windows PowerShell:**

```powershell
.\skyroads-sdl.exe "C:\Games\SkyRoads-data"
```

**Linux:**

```bash
sudo apt install libsdl2-2.0-0
./skyroads-sdl "$HOME/Games/SkyRoads-data"
```

If you place the data beside the executable, you can launch from that directory
without a data-path argument; on Windows, double-click `skyroads-sdl.exe`.
The default display mode is borderless fullscreen. Use `--windowed` or
`--exclusive-fullscreen` to override the saved display mode for one run.

Under WSL2, WSLg provides the window and audio. Controllers connected to Windows
are not automatically visible to Linux. See the [Windows setup notes][windows]
or [Linux/WSL setup notes][linux] for controller setup and diagnostics.

## Controls

| Action | Keyboard | Controller |
| --- | --- | --- |
| Navigate | Arrow keys or WASD | D-pad or left stick |
| Steer | Left/Right or A/D | D-pad left/right or left-stick X |
| Accelerate | Up or W | Right trigger, D-pad up, or stick up |
| Brake | Down or S | Left trigger, D-pad down, or stick down |
| Confirm | Enter | South face button or Start |
| Back | Escape | East face button or Back/View |
| Previous/next display value in Controls | — | LB / RB |
| Jump | Space | South face button |
| Quit | Q | Select `QUIT` on the main menu |
| Toggle fullscreen | Shift+Enter | — |
| Cycle debug view | Tab | — |

Controller face buttons are described by position because Xbox,
Nintendo-style, and 8BitDo labels differ: south is the lower face button and east
is the right face button. Select `KEYBOARD`, `JOYSTICK`, or `MOUSE` in Controls
for gameplay. Gamepad menu navigation works in every control mode. In `MOUSE`
mode, move horizontally to steer, vertically to accelerate/brake, and press any
mouse button to jump.

To exit using only the controller, return from gameplay to level selection,
return again to the main menu, select `QUIT`, and confirm. Main-menu Back does
not exit the application. After a crash or win, release the south button and
press it again to retry or return to level selection; no separate prompt is drawn.

## Procedural Roads

Select `PROCEDURAL` on the main menu to create a colorful finite road on Easy,
Classic, or Hard. Current-generation roads draw 5–8 unique set pieces from a
library of 26. The library mixes decorative spectacles with tunnels, floating
islands, branching routes, vertical decks, staircase jumps, sparse flanking
paths, three-height checkerboard fields, hazards, rewards, and other gameplay
beats. Several set pieces deliberately remove the central road and lead the
player onto isolated side blocks. Six visual themes bias each draw and its
colors, but no named set piece—including tunnels or the staircase jump—is
guaranteed. Connectors provide readable recovery space between surprises,
using only the original SkyRoads tile, world, and palette vocabulary.

The setup screen displays a generation ID with an `SR4` prefix. Entering that
ID regenerates the same difficulty, cells, world, palette, and resources. IDs
can be typed directly or entered with the on-screen controller grid. `SR1`,
`SR2`, and `SR3` remain supported and regenerate their original roads exactly;
incompatible generator changes use a new version prefix instead of changing
saved roads.

The last valid ID is saved between launches, and procedural wins never change
campaign completion.

## Input and Display Settings

Controls → `INPUT` adjusts mouse and controller sensitivity from `50%` to
`200%`; `100%` preserves the DOS-derived defaults.

The display menu supports windowed, borderless, and SDL-reported exclusive
modes. With `DISPLAY` or `VIDEO MODE` selected in Controls, LB and RB cycle the
value backward and forward.

Settings are saved in the **game-data directory**, which must be writable:

| File | Contents |
| --- | --- |
| `SKYROADS.CFG` | DOS-compatible game settings and campaign progress |
| `SKYROADS-RS-INPUT.CFG` | Native mouse/controller sensitivity |
| `SKYROADS-RS-DISPLAY.CFG` | Last successfully applied display settings |
| `SKYROADS-RS-PROCEDURAL.CFG` | Last valid generation ID |

The native preference files are separate from the DOS-compatible configuration.

## Build from Source

Development takes place on Windows 11 with Ubuntu 24.04 under WSL. The release
workflow also builds and tests on native Windows and Linux ARM64 runners.

You need a Rust toolchain and SDL2 **development** files (SDL 2.0.18 or newer).
CI uses Rust 1.97.1. On Ubuntu, install the build dependencies, then run from
the repository root:

```bash
sudo apt install build-essential pkg-config libsdl2-dev
cargo run --locked -p skyroads-sdl -- "$HOME/Games/SkyRoads-data"
```

For a native Windows build, use the MSVC Rust toolchain and SDL2 VC development
libraries; the [release workflow][workflow] shows the linker setup. To assemble
or publish packages, follow the [release guide][releasing].

Original game files are not tracked in the current source tree and are not
needed to compile the native port. Supply game data separately to run it.
GitHub-generated source archives also exclude captured DOS fixtures, so tests
that use those fixtures require a full Git checkout.

## Development Commands

For the fixture-based Rust tests, extract your full original game into the
checkout root first, including `SKYROADS.EXE`. These local files are ignored by
Git. The release workflow downloads a checksum-verified copy from the original
developer for these tests; it builds the native executable before fetching data.

```bash
cargo test --locked
cargo test --locked --workspace
cargo run --locked -p skyroads-cli -- summary .
cargo run --locked -p skyroads-cli -- demo-sim . 120
cargo run --locked -p skyroads-cli -- render-capture . /tmp/skyroads-render-capture
cargo run --locked -p skyroads-sdl -- --smoke-gameplay .
cargo run --locked -p skyroads-sdl -- --smoke-procedural .
cargo run --locked -p skyroads-sdl -- --smoke-gamepad .
cargo run --locked -p skyroads-sdl -- --controller-diagnostics
```

Run these commands from that prepared development checkout. The `.` arguments
refer to your local game data; replace them with a different data path for CLI
and smoke commands as needed. The Rust tests locate their fixtures and original
game data in the checkout root. A clean checkout can compile, but those tests
need the separate data setup above.

`cargo test --locked` covers the portable default workspace members;
`--workspace` also tests the SDL host and requires SDL development files.
For headless smoke runs, set `SDL_VIDEODRIVER=dummy` and
`SDL_AUDIODRIVER=dummy`. Controller diagnostics load no game assets.

## Status and Validation

The native port is playable, with intro, menus, demo playback, campaign and
procedural gameplay, graphics, audio, and keyboard/mouse/controller input.
Controller hot-plug handling and sensitivity settings are implemented.

The v1.0.0 [release workflow passed on all three package targets][release-run],
including workspace tests and packaged gameplay, procedural, and injected
gamepad smoke tests. These checks do not certify physical controllers; see
[the controller support and validation plan][controllers] for that evidence.

Full DOS fidelity remains a work in progress. Renderer fixtures cover all
shipped road dispatch kinds, ship shadow variants, death states, and the final
tunnel exit. Broader collision, frame, and full-song audio comparisons remain
open. The [reverse-engineering baseline][baseline] records the detailed findings.

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

- [Original game data setup][game-data]
- [Windows setup][windows] and [Linux/WSL setup][linux]
- [Building and publishing release packages][releasing]
- [Reverse-engineering baseline][baseline]
- [Port architecture][architecture]
- [Road draw routine][road-drawing]
- [Executable component diagram][exe-diagram]
- [Controller support and validation][controllers]

## Legal Note

This repository supports compatibility, preservation, research, and native-port
work. The original freeware terms require intact distribution and restrict
reverse engineering. Release packages therefore omit the original game and
data. See [the redistribution research and packaging policy][redistribution].
This exclusion does not establish legal clearance for the port itself.

[game-data]: https://github.com/bartdeijkers/SkyRoads-Level-Generator/blob/main/packaging/GAME-DATA.txt
[windows]: https://github.com/bartdeijkers/SkyRoads-Level-Generator/blob/main/packaging/README-WINDOWS.txt
[linux]: https://github.com/bartdeijkers/SkyRoads-Level-Generator/blob/main/packaging/README-LINUX.txt
[releasing]: https://github.com/bartdeijkers/SkyRoads-Level-Generator/blob/main/packaging/README.md
[redistribution]: https://github.com/bartdeijkers/SkyRoads-Level-Generator/blob/main/packaging/README.md#original-game-redistribution
[workflow]: https://github.com/bartdeijkers/SkyRoads-Level-Generator/blob/main/.github/workflows/release.yml
[release-run]: https://github.com/bartdeijkers/SkyRoads-Level-Generator/actions/runs/33919507325
[baseline]: https://github.com/bartdeijkers/SkyRoads-Level-Generator/blob/main/docs/reverse-engineering.md
[architecture]: https://github.com/bartdeijkers/SkyRoads-Level-Generator/blob/main/docs/port-architecture.md
[road-drawing]: https://github.com/bartdeijkers/SkyRoads-Level-Generator/blob/main/docs/road-draw-routine.md
[exe-diagram]: https://github.com/bartdeijkers/SkyRoads-Level-Generator/blob/main/docs/skyroads-exe-component-diagram.md
[controllers]: https://github.com/bartdeijkers/SkyRoads-Level-Generator/blob/main/plans/expand-controller-support.md
