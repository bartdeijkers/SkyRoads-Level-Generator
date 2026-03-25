# Close the Remaining DOS Gameplay Renderer Gap

## Summary
- Target the strict end-state: gameplay presentation must use the DOS road/span path and DOS view-placement path, with no live fallback renderer or fallback ship/shadow anchor in the normal gameplay render flow.
- Define success as oracle-backed equivalence, not “looks close”: representative gameplay checkpoints must match DOS frame output and placement behavior.
- Keep scope limited to gameplay/demo presentation. Do not widen this plan into physics, audio, or menu work except where they are needed to drive deterministic renderer captures.

## Implementation Changes
- Expand the DOS oracle flow in `tools/skyroads_dos_oracle.py` from a single `renderer_entry` hit into a named gameplay-render capture sequence:
  - Add a preset that reaches gameplay and captures checkpoints for first frame, steady neutral, steady left, steady right, sustained throttle, first airborne/jump, fresh death, and delayed game-over.
  - For each checkpoint, dump renderer state, the active road-byte window used by the `0x1638 + row_group * 0x0E + 0x62` path, the active TREKDAT slot/segment data, the runtime words that feed ship/view placement, and the post-render VGA frame data.
  - Normalize each capture into a committed fixture bundle under a single repo fixture location, with machine-readable metadata plus a canonical frame hash; keep raw oracle artifacts local-only.
- Finish the DOS road/span renderer in `crates/skyroads-renderer-ref/src/lib.rs`:
  - Treat `draw_dos_trekdat_pass()` as the only gameplay road path.
  - Remove `draw_demo_rows_fallback()` and `project_road_slices()` from the normal gameplay render path; keep them only behind debug-only code if they remain useful for development.
  - Validate and, where needed, correct `row_sequence`, left/right cell-column mapping, draw-type `0..5` helpers, backend-normalization behavior, tunnel/cube ordering, and color override behavior against oracle captures instead of visual judgment.
  - Add one curated checkpoint per shipped dispatch kind `0..5` from real levels, so completion is not based on Road 0 alone.
- Replace fallback view placement with DOS-derived placement:
  - Remove the fallback implementation in `ship_screen_placement_from_slices()`.
  - Recover and port the exact inputs and formulas/tables for camera centering, road centering, ship sprite X/Y, shadow X/Y, grounded vs airborne offsets, and before-ship/after-ship draw ordering.
  - If placement depends on additional DOS runtime words not currently dumped, add those addresses to the oracle capture first, then port the formula from those values; do not hardcode another “stable fallback.”
- Introduce one internal render-context model for gameplay equivalence:
  - Add a dedicated internal context/fixture type that contains only DOS-relevant render inputs for one checkpoint: row state, road bytes, active TREKDAT slot, ship simulation state, and captured placement-driving words.
  - Use that context both for fixture-driven tests and for the live renderer path so the tested path and shipped path are identical.

## Tests and Acceptance
- Replace the current fallback-based placement tests with oracle-based equivalence tests:
  - Remove tests that assert fallback anchor behavior.
  - Add fixture tests that render native frames for the named checkpoints and compare them against committed DOS oracle hashes.
  - Add focused structural tests for row-group/slot selection, pointer-row dispatch order, and draw-type `0..5` behavior so failures localize before full-frame diffs.
- Acceptance criteria:
  - The normal gameplay renderer never calls the fallback road projection path.
  - The normal gameplay renderer never returns a fallback ship/shadow placement.
  - Whole-frame equivalence passes for the named Road 0 checkpoints.
  - At least one checkpoint per draw dispatch kind `0..5` also passes, proving the renderer is not only correct for the opening road.
  - Existing smoke coverage stays green so the exact renderer still works in the SDL host.

## Assumptions and Defaults
- Use DOSBox-X oracle captures as the source of truth. Host screenshots may remain as debugging aids, but automated acceptance should use normalized DOS capture data, not manual visual comparison.
- Road 0 is the initial calibration sequence, but the plan is not complete until all shipped live draw kinds `0..5` are covered by fixtures.
- Scope ends at gameplay/demo renderer equivalence. Audio, physics, and non-gameplay menus are unchanged unless they are required to produce deterministic capture entry into gameplay.
