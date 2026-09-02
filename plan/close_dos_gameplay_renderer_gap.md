# Close the Remaining DOS Gameplay Renderer Gap

## Goal

Make the indexed native gameplay framebuffer match the shipped DOS executable,
using committed DOSBox-X captures as the acceptance contract. Native captures
remain an iteration aid; only DOS fixtures establish parity.

This plan covers gameplay and demo presentation. Physics or input changes belong
here only when they are required to reach equivalent renderer state.

## Completed

- The normal gameplay path uses the TREKDAT road renderer and no longer calls the
  fallback road projection renderer.
- `FrameBuffer320x200` renders through indexed pixels plus a 256-entry palette.
  SDL and PPM conversion happen only at the output boundary.
- The oracle emits bundle-version-2 fixtures containing the raw 64,000-byte VGA
  frame, 768-byte 6-bit palette, runtime dumps, source fingerprints, and hashes.
- Gameplay capture writes held controls to the recovered key-state table at
  `DS:0BA2`; BIOS-buffer arrow events are no longer mistaken for gameplay input.
- The world bitmap, road colors, dashboard, oxygen/fuel/speed gauges, trail
  meter, Jump Master display, ship sprites, thrust colors, placement, and shadow
  masks use DOS indexed behavior.
- The native capture suite accounts for the DOS scheduling ratio: 24 renderer
  hits correspond to 28 physics updates in the throttle and steering scenarios.
- Five committed Road 0 checkpoints match exactly:

  | Checkpoint | Pixels | Palette components |
  | --- | ---: | ---: |
  | steady neutral | 64,000 / 64,000 | 768 / 768 |
  | sustained throttle | 64,000 / 64,000 | 768 / 768 |
  | steady left | 64,000 / 64,000 | 768 / 768 |
  | steady right | 64,000 / 64,000 | 768 / 768 |
  | first visible airborne frame | 64,000 / 64,000 | 768 / 768 |

- `skyroads-renderer-ref` enforces those frames in normal workspace tests.
- The CLI writes `.indices` and `.vga6` files and reports exact mismatch counts,
  bounds, regions, common index pairs, and an optional visual diff.
- The shipped-level inventory identifies the earliest playable occurrence of
  every dispatch kind:

  | Kind | Road | Row | Column | Descriptor |
  | ---: | ---: | ---: | ---: | ---: |
  | 0 | 1 | 0 | 0 | `0x0005` |
  | 1 | 20 | 0 | 3 | `0x010F` |
  | 2 | 5 | 0 | 0 | `0x0207` |
  | 3 | 26 | 12 | 3 | `0x031F` |
  | 4 | 5 | 1 | 0 | `0x0407` |
  | 5 | 9 | 7 | 0 | `0x0507` |

## Completed implementation

### Renderer breadth

- Committed exact live-level fixtures cover every shipped dispatch kind:
  Road 1 for kind 0, Road 20 for kind 1, Road 5 for kinds 2 and 4, Road 26 for
  kind 3, and Road 9 for kind 5.
- Grounded plus later airborne fixtures cover shadow variants `0..4`.
- The permissive ship-mask placeholder and the old road-span clipping heuristic
  are gone. The native renderer ports the recovered `0x32A5` algorithm:
  33 mask rows, the split-row lift adjustment, left/right boundary selection,
  screen bounds, the explosion sentinel, and DOS's 956-byte clear of the
  957-byte mask. Tests cover unobstructed, obstructed-left, obstructed-right,
  and explosion-sentinel masks.
- Road 2 fixtures cover the first fallen frame and continued fall. Road 30
  fixtures cover the fresh explosion and three later sprite stages. Road 0
  fixtures cover fresh out-of-oxygen and the last animated frame before the
  delayed game-over freeze.
- A final Road 1 tunnel-exit framebuffer matches DOS exactly. A separate core
  regression reaches `level.length() - 0.5` and proves the final tunnel row
  latches the win without an out-of-range row lookup.

### Palette and render context

- The active 256-entry palette is assembled from exact DOS sources: 72 colors
  from the selected road, 20 from `CARS.LZS`, 50 from `DASHBRD.LZS`, and 114
  from the selected `WORLD*.LZS`.
- Live DAC fixtures independently prove the composition for Road 1/world 0 and
  Road 5/world 1. A structural test verifies all source-bank boundaries and
  world mappings across every shipped road, including worlds `0..9`.
- `GameplayRenderContext` now carries the renderer counter, active TREKDAT slot,
  and ship sprite phase. `DashboardRenderState` carries the DOS-latched speed,
  lift, resource, and Jump Master inputs, so fixture tests and the live renderer
  use the same explicit state.
- Oracle presets can write normalized `segment:offset` memory safely after a
  deterministic renderer warm-up. This enables exact out-of-resource and
  final-tunnel captures without timing-dependent keyboard tricks.

### Regression harness

- Exact mismatch failures report a count, bounding box, most common palette
  index pairs, and the first mismatches instead of printing two complete
  framebuffers.
- Fixture lengths and manifest hashes are validated before comparison.
- The native capture set contains 141 labeled frames and compares deterministically
  across independent runs.
- The headless SDL gameplay smoke still reaches live gameplay.

## Implementation order used

1. Inventoried all shipped dispatch values and added the earliest deterministic
   live fixture for each kind.
2. Localized and fixed road ordering, palette, dashboard, ship, and shadow
   mismatches one fixture at a time.
3. Ported the recovered ship-mask routine and covered all shadow states.
4. Added fallen, explosion, resource-death, delayed game-over, tunnel-exit, and
   win regressions.
5. Replaced inferred palette tails with the original road/art/world palette
   banks and verified every shipped mapping.
6. Made renderer-only and dashboard-latched state explicit.
7. Closed with workspace tests, strict Clippy, Python oracle tests, repeated
   native capture comparison, and headless SDL smoke.

## Acceptance

- No normal gameplay code calls fallback road or fallback placement paths.
- Exact whole-frame and palette comparisons pass for the five Road 0 movement
  checkpoints, every shipped dispatch kind `0..5`, all shadow variants, death,
  game-over, and win.
- Every world palette is DOS-captured or proven byte-for-byte from source data.
- Repeated native captures are deterministic.
- Workspace tests, strict Clippy, Python oracle tests, and SDL smoke all pass.

All acceptance criteria above are satisfied locally. Audio/OPL equivalence and
non-gameplay presentation parity remain separate porting work; they are not
renderer regressions hidden by this plan.
