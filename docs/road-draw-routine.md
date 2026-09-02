# Road Draw Routine (`0x2D03`) — Annotated Disassembly

Reverse-engineered from `SKYROADS.EXE` (16-bit MZ; image offset = file offset −
`0x200`). This is the main gameplay road renderer, entered each frame at image
offset `0x2D03` (runtime `0824:2D03`). It is the source of truth for porting the
native road rasterizer in `crates/skyroads-renderer-ref` to pixel-equivalence.

Reproduce with:

```bash
ndisasm -b 16 -e 0x2F03 -o 0x2D03 SKYROADS.EXE | less   # entry
ndisasm -b 16 -e 0x3050 -o 0x2E50 SKYROADS.EXE | less   # draw kind 0
```

All `[0xNNN]` operands are within the runtime data/stack segment (`SS`/`DS` ==
`0x0E92` during gameplay). Renderer state words live around `DS:0x0E34..0x0E92`.

## Entry + setup (`0x2D03`–`0x2D52`)

Eight word args are copied from the stack frame into renderer state
`[0xE34]..[0xE42]` (these are the placement/camera inputs), then `call [0xE44]`
runs a setup callback.

- `[0xE36]` = `current_row` (drives row-group and slot selection below).
- `[0xE42]` = a base segment; `ax = [0xE42] + 0x280` (or `+0x280>>3` when
  `[0x36]==0`), `es = ax` — the VGA write target page.

## Row-group + TREKDAT slot selection (`0x2D53`–`0x2D72`)

Matches the documented road-byte source and 8-slot ring:

```
bp = 0x1638 + (current_row >> 3) * 0x0E + 0x62      ; active road-byte window
ds = [ (current_row & 7) * 2 + 0x0E82 ]             ; expanded TREKDAT segment for slot
```

`(current_row>>3)` is the road row-group; `(current_row&7)` is the TREKDAT slot.
The native side reproduces both inputs exactly (verified:
`native_road_row0_matches_dos_active_road_window`,
`native_trekdat_pointer_grid_matches_dos_slot0`).

## Per-cell main loop (`0x2DB0`–`0x2DD3`)

```
0x2DB0  bl = [bp+1]                 ; second byte of the 16-bit road descriptor
0x2DB3  bx = (bl & 0x0F) << 1       ; dispatch kind = (descriptor >> 8) & 0x0F
0x2DB8  call [ss:bx + 0x0B7F]       ; draw-type dispatch table (kinds 0..5; 6..15 = noop)
0x2DBD  di += 0x0C                  ; advance draw column (12-px stride)
0x2DC0  bp += 2                     ; next road descriptor word
        ... loop while [ss:0xE52] (cells-per-row counter) != 0
```

This confirms the dispatch kind comes from the descriptor's **high byte** low
nibble, and the native draw-dispatch table matches DOS (verified:
`shipped_draw_dispatch_targets_match_dos_capture`).

## Row stepping + depth loop (`0x2DD5`–`0x2E1F`)

After each row the routine adjusts `di`/`bp`, increments the band counters
`[0xE54]`/`[0xE56]`, and re-runs from `0x2D83` for the next depth band via the
projection callback `call [ss:0xE4A]`. `[0xE50]` counts down the depth bands
(initialised to `0x0B`).

## Finish (`0x2E21`–`0x2E4F`)

Calls `0x3492` (`ax=1`), then copies `0x1DE` words from `DS:0x0E92` to `DS:0x124F`
(`rep movsw`) — the ship/HUD composition buffer — and returns. `0x0E92` is the
ship mask buffer base noted elsewhere in the RE notes.

## Draw kind 0 (`0x2E50`)

```
0x2E50  al = [bp+0]                 ; first byte of the descriptor (tile id)
0x2E53  al &= 0x0F
0x2E55  jz  skip                    ; tile 0 (empty) draws nothing
0x2E57  si = [di]                   ; destination pointer for this draw column
0x2E59  [si] = al                   ; write tile id
0x2E5B  call [ss:0xE4C]             ; span/projection helper
        ...
```

Kinds `1..5` live at `0x303D`, `0x2E9F`, `0x2EE1`, `0x2F3C`, `0x2FB0`
(see the dispatch table); `0x3AAD` is the shared no-op for kinds `6..15`.

## Porting status

The renderer inputs are verified identical to DOS: road descriptors, dispatch
table, tile-class table, TREKDAT pointer grid, and the four gameplay palette
banks. The native per-kind span path now has exact whole-frame fixtures for
dispatch kinds `0..5`, rather than only the early Road 0 calibration frame.

The ship-mask builder at `0x32A5` is also ported. It clears 956 bytes of the
957-byte mask, processes 33 rows, selects the left or right visible span around
the `SS:0x044A` half-width table, and applies the `[0xE40] - 8` lift adjustment
after the 24th row. DOS uses `0x7FFF` at `[0xE40]` during explosion rendering,
which deliberately leaves 260 mask bytes zero. Native tests lock the
unobstructed, both boundary-side, and explosion-sentinel branches.

The renderer closeout and its exact fixture matrix are tracked in
`plan/close_dos_gameplay_renderer_gap.md`.
