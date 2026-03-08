# SkyRoads Port Architecture

The target is not a "modern remake." The target is a deterministic reimplementation that reproduces the DOS build's behavior, then wraps that core in a platform layer for macOS and other hosts.

## Core Rule

The port must treat DOS behavior as the spec:

- same asset formats
- same simulation order
- same road descriptor semantics
- same renderer ordering
- same demo playback results
- same sound/music event timing

The platform layer should be replaceable. The game core should not know whether it is running on DOS, macOS, or anything else.

## Recommended Split

### `game_data`

Responsibilities:

- parse original `*.LZS`, `*.SND`, `*.DAT`, `DEMO.REC`, and `SKYROADS.EXE`-derived tables
- expose exact typed structures for roads, TREKDAT records, images, sound banks, music songs, and startup/runtime constants
- preserve raw values alongside interpreted fields where semantics are still incomplete

Why:

- this is the lowest-risk path to a future native loader
- it keeps reverse engineering and porting on the same data model

### `game_core`

Responsibilities:

- deterministic fixed-step simulation
- player motion and resource logic
- demo playback
- menu flow and state transitions
- runtime sequencing for intro, menu, and in-game states

Requirements:

- no direct rendering API access
- no direct audio API access
- all timing comes from explicit ticks, not wall clock

Current local status:

- `skyroads-core` already has deterministic demo sampling, road-row selection, renderer dispatch planning, a playable gameplay/session layer, and an explicit attract-mode app state machine
- the current session path is good enough to drive a native asset-backed host, a live gameplay loop, and shipped demo input as a regression fixture

### `game_renderer_ref`

Responsibilities:

- software renderer matching DOS ordering and composition
- TREKDAT span emission
- road cell dispatch by descriptor second-byte low nibble
- HUD fragment composition
- palette handling

Requirements:

- CPU-first reference implementation
- frame buffer output should be byte-comparable or CRC-comparable in tests

This reference renderer matters more than early GPU acceleration. A correct software path is the baseline; accelerated backends can come later.

### `game_audio_ref`

Responsibilities:

- sample playback for `INTRO.SND` and `SFX.SND`
- exact event scheduling for `MUZAX`
- eventual OPL-compatible synthesis path

Requirements:

- event order must be testable without an actual sound device
- "what was scheduled" should be inspectable in logs/tests

### `platform_host`

Responsibilities:

- window creation
- input collection
- audio device output
- file location and save/config paths
- macOS app packaging

Requirements:

- keep host I/O outside `game_core`
- feed the core explicit input/tick/audio commands

Current local status:

- [`crates/skyroads-sdl`](/Users/ammaar/Development/skyroads/crates/skyroads-sdl) is now the first native host
- it opens a local SDL window on macOS, runs the attract-mode app at a fixed `70 Hz`, uploads the reference framebuffer, outputs native audio, supports live gameplay controls, and follows the intro -> menu -> gameplay/demo loop
- this host is intentionally provisional: it now exercises original graphics and sound assets by default, but its road renderer is still not the final DOS-faithful TREKDAT span path

## Validation Harness

The port should ship with non-optional equivalence checks:

1. startup asset order:
   compare against the DOSBox-X startup trace
2. road descriptor coverage:
   assert all shipped descriptors are supported by the renderer
3. demo playback:
   verify player state over time against the DOS build
4. frame checks:
   capture reference frames for intro/menu/gameplay
5. audio checks:
   compare scheduled music/sfx events before worrying about waveform-perfect output

## Current Best Input Artifacts

- [`tools/skyroads_extract.py`](/Users/ammaar/Development/skyroads/tools/skyroads_extract.py)
- [`tools/skyroads_dosbox_trace.py`](/Users/ammaar/Development/skyroads/tools/skyroads_dosbox_trace.py)
- [`tools/skyroads_dos_oracle.py`](/Users/ammaar/Development/skyroads/tools/skyroads_dos_oracle.py)
- [`crates/skyroads-data`](/Users/ammaar/Development/skyroads/crates/skyroads-data)
- [`crates/skyroads-core`](/Users/ammaar/Development/skyroads/crates/skyroads-core)
- [`crates/skyroads-cli`](/Users/ammaar/Development/skyroads/crates/skyroads-cli)
- [`crates/skyroads-sdl`](/Users/ammaar/Development/skyroads/crates/skyroads-sdl)
- [`docs/reverse-engineering.md`](/Users/ammaar/Development/skyroads/docs/reverse-engineering.md)

Useful generated outputs:

- `/tmp/skyroads_extract_exe5/roads/descriptor_catalog.json`
- `/tmp/skyroads_extract_exe5/exe/runtime_tables.json`
- `/tmp/skyroads_trace_25s/summary.md`

Current status:

- the native data layer already covers roads, level semantics, demo input, TREKDAT records/shapes, MUZAX songs, and EXE runtime tables
- the native core layer already covers verified demo sampling, renderer row/slot selection, descriptor-to-dispatch mapping, gameplay-row planning, a deterministic gameplay session, and the attract-mode state machine, and it now exports exact ship simulation/control inputs to the renderer instead of guessed ship pose fields
- the live gameplay call-site disassembly now shows that DOS feeds the road renderer with an eighth-tile `current_row` counter, so the native port has started moving those inputs onto the same scale instead of using whole-tile row indices
- the first native host layers now exist via [`crates/skyroads-renderer-ref`](/Users/ammaar/Development/skyroads/crates/skyroads-renderer-ref), [`crates/skyroads-audio-ref`](/Users/ammaar/Development/skyroads/crates/skyroads-audio-ref), and [`crates/skyroads-sdl`](/Users/ammaar/Development/skyroads/crates/skyroads-sdl), which together make the current port path playable on macOS with original art/audio assets and a live gameplay path
- the new DOS oracle path can now capture Road-0 renderer/runtime checkpoints directly from the original executable, but the next missing layers are still the DOS-faithful TREKDAT road renderer, tighter MUZAX/OPL equivalence, and stronger frame/audio equivalence harnesses against the original executable

## Near-Term Milestones

1. Finish road descriptor semantics for the six live renderer kinds.
2. Map TREKDAT pointer-grid axes to exact on-screen primitive selection.
3. Replace the current interim road scene with the DOS-faithful TREKDAT software renderer.
4. Capture frame-accurate intro/menu/demo traces from DOSBox-X and compare them against the native session.
5. Tighten reference audio scheduling until MUZAX playback matches the original event timing closely enough for equivalence tests.
