# Close the Remaining DOS Gameplay Renderer Gap

## Summary
- Target the strict end-state: gameplay presentation must use the DOS road/span path and DOS view-placement path, with no live fallback renderer or fallback ship/shadow anchor in the normal gameplay render flow.
- Define success as oracle-backed equivalence, not “looks close”: representative gameplay checkpoints must match DOS frame output and placement behavior.
- Keep scope limited to gameplay/demo presentation. Do not widen this plan into physics, audio, or menu work except where they are needed to drive deterministic renderer captures.

## Status Snapshot
- Done locally:
  - The normal gameplay renderer no longer calls `draw_demo_rows_fallback()` in the shipped gameplay/demo path.
  - Removing that fallback call produced no `render-demo` diff across the full attract-mode demo: `1707` unchanged frames in before/after native capture compares.
  - The native capture workflow is deterministic for the current scenario set: repeated `render-capture` runs compare as `105` unchanged labels with no changed, added, or removed frames.
  - The DOS oracle tool now runs successfully on Windows 11 with this repo inside WSL Ubuntu 24.04 and captures the Road 0 `renderer_entry` checkpoint under DOSBox-X.
  - Current smoke coverage remains green, including the SDL gameplay smoke path.
- Still open:
  - Oracle-backed frame fixtures are not committed yet, so final sign-off still is not automated.
  - Ship/shadow placement, ship mask generation, and frame-level equivalence still need broader DOS-backed checkpoint coverage.
  - Draw dispatch kinds `1..5` still need curated oracle checkpoints beyond the opening Road 0 calibration path.

## Current Findings
- A native bug clipped ship pixels to the visible road span. That was not DOS-faithful: the DOS build gates ship writes through a dedicated `29 x 33` ship mask buffer at `SS:0x0E92`, built by the ship path around `0x32A5`, not from a road-coverage clip window. The current local mitigation is to keep ship pixels unmasked until that DOS mask path is ported exactly.
- A second native bug mis-indexed the exact `63`-frame ship run extracted from `CARS.LZS`. The full-width split contains four extra fragments before the real ship frames, so the native atlas must start at the first full ship frame, not merely at the first split after the explosion strip. This removed the false mini/explosion-like frame visible in `render-demo` around `24s..27s`.
- The normal gameplay renderer no longer falls back to `draw_demo_rows_fallback()`. Removing that call produced no visual/native hash change in the full `render-demo` capture, which strongly suggests the DOS road/span path already covers the current long-form demo frames. `project_road_slices()` remains useful for debug views, but not for the normal gameplay render path.
- The native capture path is stable enough to support local regression checks while oracle fixtures grow. Repeated `render-capture` runs on the current labeled suite produced identical manifests.
- A WSL-friendly DOS oracle pass now works. `road0-initial-frame` reaches Road 0, hits `renderer_entry` at `0824:2D03`, and writes dump artifacts for `renderer_state`, `tile_class_by_low3`, `draw_dispatch_by_type`, and `trekdat_segment_table`. The current captured checkpoint reports `current_row 24`, road-row group `3`, and TREKDAT slot `0`.
- On WSL, screenshot capture is currently unavailable by default and is not required for oracle data capture. The useful oracle artifacts are the debugger dumps and metadata, not host screenshots.
- These local fixes reduced obvious visual regressions, but they are not sign-off. Ship masking, sprite ordering, and placement still need oracle-backed DOS equivalence before this plan can be considered complete.

## Implementation Changes
- Expand the DOS oracle flow in `tools/skyroads_dos_oracle.py` from a single `renderer_entry` hit into a named gameplay-render capture sequence:
  - The tool is now host-portable enough to run under Windows 11 + WSL Ubuntu 24.04 with DOSBox-X discovered via `PATH` or `--dosbox`, with host-aware key injection and non-fatal screenshot skipping when no supported screenshot backend exists.
  - `road0-initial-frame` now succeeds in this environment and captures the first gameplay-side `renderer_entry` checkpoint into portable dump files plus machine-readable summary metadata.
  - Add a preset that reaches gameplay and captures checkpoints for first frame, steady neutral, steady left, steady right, sustained throttle, first airborne/jump, fresh death, and delayed game-over.
  - For each checkpoint, dump renderer state, the active road-byte window used by the `0x1638 + row_group * 0x0E + 0x62` path, the active TREKDAT slot/segment data, the runtime words that feed ship/view placement, and the post-render VGA frame data.
  - Normalize each capture into a committed fixture bundle under a single repo fixture location, with machine-readable metadata plus a canonical frame hash; keep raw oracle artifacts local-only.
- Add a native before/after capture workflow in `crates/skyroads-cli`:
  - `render-capture` and `render-compare` now exist and are repeatable enough to baseline renderer edits locally before promoting changes to oracle-backed fixtures.
  - Add `render-capture` to render a deterministic gameplay ship suite into portable frame dumps plus a manifest of hashes and ship/row metadata.
  - Add `render-compare` to diff two native capture manifests by labeled frame so renderer edits can be checked locally before promoting changes to oracle-backed fixtures.
  - Treat this native capture path as a local regression aid, not as the final source of truth; it should speed up iteration without weakening the DOS-equivalence bar.
- Finish the DOS road/span renderer in `crates/skyroads-renderer-ref/src/lib.rs`:
  - Treat `draw_dos_trekdat_pass()` as the only gameplay road path. This is now true for the normal gameplay/demo render path.
  - `draw_demo_rows_fallback()` has been removed from the normal gameplay render path. `project_road_slices()` can remain only in debug/inspection code until DOS-equivalent placement work is complete.
  - Validate and, where needed, correct `row_sequence`, left/right cell-column mapping, draw-type `0..5` helpers, backend-normalization behavior, tunnel/cube ordering, and color override behavior against oracle captures instead of visual judgment.
  - Add one curated checkpoint per shipped dispatch kind `0..5` from real levels, so completion is not based on Road 0 alone.
- Replace fallback view placement with DOS-derived placement:
  - `ship_screen_placement_from_slices()` already delegates to the DOS-oriented ship pipeline instead of a live road-slice fallback. Remaining work is to prove and complete the exact DOS formulas/tables with oracle captures, not to preserve any guessed fallback placement.
  - Recover and port the exact inputs and formulas/tables for camera centering, road centering, ship sprite X/Y, shadow X/Y, grounded vs airborne offsets, and before-ship/after-ship draw ordering.
  - Port the DOS ship-mask builder around `0x32A5` and retire the current temporary “all ship pixels visible” behavior; do not reintroduce a road-span-derived sprite clip heuristic.
  - Validate `CARS.LZS` exact-ship frame extraction against DOS frame selection, including the explosion-strip boundary and the start of the `63`-frame ship run, before trusting native frame indices.
  - If placement depends on additional DOS runtime words not currently dumped, add those addresses to the oracle capture first, then port the formula from those values; do not hardcode another “stable fallback.”
- Introduce one internal render-context model for gameplay equivalence:
  - Add a dedicated internal context/fixture type that contains only DOS-relevant render inputs for one checkpoint: row state, road bytes, active TREKDAT slot, ship simulation state, and captured placement-driving words.
  - Use that context both for fixture-driven tests and for the live renderer path so the tested path and shipped path are identical.

## Tests and Acceptance
- Replace the current fallback-based placement tests with oracle-based equivalence tests:
  - Remove tests that assert fallback anchor behavior.
  - Add fixture tests that render native frames for the named checkpoints and compare them against committed DOS oracle hashes.
  - Add focused structural tests for row-group/slot selection, pointer-row dispatch order, and draw-type `0..5` behavior so failures localize before full-frame diffs.
  - Keep native regression tests for the two concrete failures already found locally: early Road 0 steering must not clip the ship to the flat road span, and the demo segment around `24s..27s` must keep visible airborne left-edge ship pixels with no false mini/explosion-like frame.
- Keep a native capture smoke path available while the oracle fixture set grows:
  - `render-capture` must emit the same labeled frame set on repeat runs with identical inputs.
  - `render-compare` must report changed, added, and removed labels so before/after renderer runs are inspectable without manual screenshot bookkeeping.
  - `render-demo` remains a useful local probe for long-form airborne and edge-of-screen ship issues, but it does not replace oracle-backed acceptance.
- Local validation completed so far:
  - `cargo test -p skyroads-renderer-ref` passes.
  - `cargo test -p skyroads-cli` passes.
  - `SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy cargo run -p skyroads-sdl -- --smoke-gameplay .` passes.
  - `render-demo` before/after the fallback-road removal compares as `1707` unchanged frames.
- Acceptance criteria:
  - The normal gameplay renderer never calls the fallback road projection path.
  - The normal gameplay renderer never returns a fallback ship/shadow placement.
  - The normal gameplay renderer never derives ship sprite clipping directly from projected/native road spans.
  - Whole-frame equivalence passes for the named Road 0 checkpoints.
  - At least one checkpoint per draw dispatch kind `0..5` also passes, proving the renderer is not only correct for the opening road.
  - The native capture workflow is good enough to baseline renderer edits locally, but final sign-off still comes from oracle-backed equivalence.
  - Existing smoke coverage stays green so the exact renderer still works in the SDL host.
- Current acceptance status:
  - Completed locally:
    - The normal gameplay renderer never calls the fallback road projection path.
    - The native capture workflow is good enough to baseline renderer edits locally.
    - Existing smoke coverage stays green so the exact renderer still works in the SDL host.
  - Still open:
    - The normal gameplay renderer never returns a fallback ship/shadow placement.
    - The normal gameplay renderer never derives ship sprite clipping directly from projected/native road spans.
    - Whole-frame equivalence passes for the named Road 0 checkpoints.
    - At least one checkpoint per draw dispatch kind `0..5` also passes, proving the renderer is not only correct for the opening road.
    - Final sign-off from committed oracle-backed fixtures and tests.

## Assumptions and Defaults
- Primary development happens on Windows 11 with this repo inside WSL Ubuntu 24.04. Any macOS-specific tooling in this repo reflects upstream history and should not be treated as the default local environment.
- Use DOSBox-X oracle captures as the source of truth. Host screenshots may remain as debugging aids, but automated acceptance should use normalized DOS capture data, not manual visual comparison.
- Use the native CLI capture manifests as an iteration tool between edits, not as a replacement for DOS fixtures.
- Road 0 is the initial calibration sequence, but the plan is not complete until all shipped live draw kinds `0..5` are covered by fixtures.
- Scope ends at gameplay/demo renderer equivalence. Audio, physics, and non-gameplay menus are unchanged unless they are required to produce deterministic capture entry into gameplay.
