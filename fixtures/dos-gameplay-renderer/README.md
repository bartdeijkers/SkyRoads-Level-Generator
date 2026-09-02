# DOS Gameplay Renderer Fixtures

These bundles are normalized captures from the shipped `SKYROADS.EXE`, taken
with DOSBox-X by `tools/skyroads_dos_oracle.py`. They are the source of truth for
indexed gameplay rendering and palette equivalence.

## Exact frame bundles

Fixture bundle version 2 stores the complete `320 x 200` indexed framebuffer,
the 256-entry 6-bit VGA palette, renderer inputs, ship state, and the recovered
gameplay key-state table:

```text
fixtures/dos-gameplay-renderer/<preset>/<checkpoint>/
  fixture.json
  frame.indices
  palette.vga6
  dumps/*.bin
```

The following Road 0 checkpoints are exact and enforced by
`skyroads-renderer-ref` tests:

- `road0-steady-neutral/steady-neutral`
- `road0-sustained-throttle/sustained-throttle`
- `road0-steady-left/steady-left`
- `road0-steady-right/steady-right`
- `road0-first-airborne/first-airborne`

Each comparison covers all 64,000 palette indices and all 768 VGA palette
components. Together these checkpoints exercise neutral rendering, acceleration
and speed HUD updates, both steering directions, the thrust animation, the first
visible jump frame, and the airborne shadow.

The closeout matrix adds:

- dispatch kinds `0..5` through Road 1, Road 5, Road 9, Road 20, and Road 26
- shadow variants `2..4`, combined with the grounded and first-airborne Road 0
  fixtures for the complete `0..4` set
- Road 2 fallen progression
- Road 30 fresh and later explosion frames
- fresh out-of-oxygen and the delayed game-over boundary
- the final Road 1 tunnel-exit frame

Fixture manifests include the renderer-only context needed to reconstruct a
presented frame, such as the speed/lift visible-count latches and ship sprite
phase. The resource and tunnel presets use debugger memory writes only after
eight neutral renderer hits, so they modify live gameplay state rather than the
initialization call.

The gameplay scenarios write held controls to SkyRoads' own keyboard-state table
at `DS:0BA2`. BIOS keyboard-buffer events are suitable for menus but are not a
valid gameplay-input oracle because the original gameplay loop polls held-key
state.

The DOS renderer and physics loop are not one-to-one. The 24-hit throttle and
steering checkpoints contain 28 physics updates. The native capture suite uses
28 updates for those scenarios so it compares equivalent simulation state.

## Legacy diagnostic bundles

`road0-initial-frame` and `road0-first-five-renderer-frames` predate bundle
version 2. Their renderer-entry dumps remain useful diagnostics, but their first
frame is the level-select framebuffer left in VGA memory before the first road
draw. Do not use that pre-draw frame as gameplay acceptance evidence.

## Regenerating

DOSBox-X must be a debugger-enabled build. Under WSL, pass the executable path
explicitly if it is not on `PATH`:

```bash
python3 tools/skyroads_dos_oracle.py \
  --source . \
  --output /tmp/skyroads-oracle \
  --dosbox /path/to/dosbox-x \
  --preset road0-steady-neutral \
  --capture-vga-frame \
  --write-fixtures
```

The oracle controls the DOSBox-X launch and installs the renderer breakpoint at
the verified shipped load address `0824:2D03` before execution. This avoids a
startup race where an EXE-relative breakpoint was not installed unless the
first warm-up retrace happened to stop inside SkyRoads.

The menu launch uses timed DOSBox `ADDKEY` commands and can race emulator
startup. The oracle retries runs that produce no checkpoint; `--max-attempts 3`
is the default.
