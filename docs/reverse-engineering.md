# SkyRoads Reverse-Engineering Baseline

This project started from the original DOS distribution, not a source checkout. The repository is centered on the native port, tooling, and reverse-engineering notes, but the current workspace still carries the original DOS data files because the native build, extractors, and validation harnesses depend on them today.

The original distribution analyzed for these notes consisted of the shipping executable plus asset/data packs:

- `SKYROADS.EXE`: 16-bit DOS executable. Embedded strings indicate a 286 requirement, EGA/VGA checks, and the expected asset filenames.
- `*.LZS`: mixed resource containers. Most are image resources with `CMAP` and `PICT` chunks. `ANIM.LZS` is a small animation wrapper around the same chunk types. `ROADS.LZS`, `TREKDAT.LZS`, and `MUZAX.LZS` each use separate container layouts.
- `*.SND`: unsigned 8-bit PCM sample data or sample banks.
- `*.DAT`: small lookup tables used by HUD/runtime systems.
- `DEMO.REC`: recorded demo input/state data.

## Legal Note

These notes document technical reverse engineering and native-port work. Redistribution, packaging, and other rights should be evaluated independently before treating this as a finished public release.

## Verified Format Findings

- Image-like `.LZS` files begin with `CMAP`.
- `CMAP` layout in this build is:
  - 4 bytes: ASCII `CMAP`
  - 1 byte: palette entry count
  - `count * 3` bytes: VGA RGB palette values
  - `count * 2` bytes: auxiliary palette data
- `PICT` layout is:
  - 4 bytes: ASCII `PICT`
  - 2 bytes: unknown field
  - 2 bytes: image height
  - 2 bytes: image width
  - 3 bytes: SkyRoads compression parameters
  - variable: compressed pixel indices
- `ANIM.LZS` wraps the same image chunks, but still has small non-image values between some frames. The frame images are extractable; the wrapper semantics are not fully characterized yet.
- `ROADS.LZS` begins with a table of 31 `(offset, unpacked_size)` pairs.
- Each road entry begins with three 16-bit values, then a 72-color VGA palette, then the common SkyRoads compression stream.
- Decompressed road data is a sequence of 16-bit block descriptors arranged in rows of 7.
- `TREKDAT.LZS` is a sequence of 8 records, not a single raw compressed payload.
- Each `TREKDAT` record is:
  - 2 bytes: `load_buff_end`
  - 2 bytes: `bytes_to_read`
  - 3 bytes: SkyRoads compression widths
  - variable: compressed payload that expands to exactly `bytes_to_read` bytes
- All observed `TREKDAT` records use widths `(4, 10, 13)`.
- After record decompression, the DOS loader reconstructs an expanded buffer of exactly `load_buff_end` bytes.
- The first 624 bytes of each expanded `TREKDAT` record are a table of 312 little-endian pointers laid out as `13 rows x 24 columns`.
- Every observed `TREKDAT` pointer is unique and points past the 624-byte table area.
- `TREKDAT` shape records are not vector polylines. They are span lists:
  - 1 byte: color/index
  - 2 bytes: base pointer
  - repeated triplets derived from `(offset, width, padding)` until `0xFF`
  - the observed padding bytes are always zero in this build
- Local `TREKDAT` record sizes are:
  - record 0: file offset `0`, expanded size `24716`
  - record 1: file offset `11368`, expanded size `25775`
  - record 2: file offset `23558`, expanded size `26324`
  - record 3: file offset `35955`, expanded size `26702`
  - record 4: file offset `48331`, expanded size `27278`
  - record 5: file offset `60923`, expanded size `26780`
  - record 6: file offset `73425`, expanded size `26399`
  - record 7: file offset `85828`, expanded size `26153`
- `MUZAX.LZS` begins with a fixed song table, not a single outer header plus one shared payload.
- The first 16-bit value is also song 0's compressed start offset. In this build it is `120`, so the song table is `120 / 6 = 20` entries long.
- Each `MUZAX` song table entry is:
  - 2 bytes: compressed start offset in the file
  - 2 bytes: instrument count
  - 2 bytes: uncompressed song length
- 14 `MUZAX` song slots are populated; 6 trailing entries are zeroed.
- Each populated `MUZAX` song decompresses independently from its own file offset and all observed songs use widths `(6, 10, 12)`.
- The first `num_instruments * 16` bytes of each decompressed song are OPL instrument blocks.
- The remaining decompressed bytes are a stream of 2-byte music commands.
- For song 0, the decompressed layout is:
  - start offset `120`
  - `9` instruments
  - uncompressed length `12318`
  - `144` instrument bytes
  - `12174` command bytes
- `INTRO.SND` is raw unsigned 8-bit mono PCM at `8000 Hz`.
- `SFX.SND` is a banked sample file:
  - the first 16-bit value is the start offset of effect 0
  - every 16-bit value before that offset is another effect start
  - each effect runs until the next start offset or EOF
- Local `SFX.SND` currently yields 6 effect slots with lengths `3984`, `5154`, `8085`, `801`, `7771`, and `0` bytes.
- `DEMO.REC` is a byte stream of recorded controller states.
- For each demo byte `al`:
  - `(al & 3) - 1` is accelerate/decelerate
  - `((al >> 2) & 3) - 1` is left/right
  - `((al >> 4) & 1)` is jump
- The runtime indexes `DEMO.REC` by tile position using a factor of `0x10000 / 0x666`.
- The local `DEMO.REC` file is `6398` bytes long, which covers about `159.91` tiles of forward travel.
- `OXY_DISP.DAT`, `FUL_DISP.DAT`, and `SPEED.DAT` are dashboard fragment packs, not compressed images.
- The loader skips the first 16-bit word, checks the second 16-bit word, then uses either:
  - `0x0A` header words when the probe is not `0x2C`
  - `0x22` header words when the probe is `0x2C`
- After that header area, each DAT fragment is:
  - 2 bytes: screen position
  - 1 byte: width
  - 1 byte: height
  - `width * height` bytes: 3-color pixel data
- The dashboard DAT palette is:
  - `0`: transparent
  - `1`: dark purple `(97, 0, 93)`
  - `2`: light purple `(113, 0, 101)`
- Local DAT fragment counts are:
  - `OXY_DISP.DAT`: 10 fragments
  - `FUL_DISP.DAT`: 10 fragments
  - `SPEED.DAT`: 34 fragments
- `SKYROADS.EXE` has a 512-byte MZ header and a 29960-byte load module.
- The executable entry point is `0000:60D0`, which maps to file offset `0x62D0`.
- The executable contains only 2 relocation entries in this build.
- OpenRoads' `ExeReader` view starts at segment `0x66E`, which maps to image offset `0x66E0` and file offset `0x68E0`.
- `SKYROADS.EXE` also contains embedded direct-image data used by the HUD:
  - `NUMBERS`: 10 frames of `4 x 5` pixels at `ExeReader` stream offset `0x13C` and file offset `0x6A1C`
  - `JUMPMASTER`: 2 frames of `26 x 5` pixels at `ExeReader` stream offset `0x204` and file offset `0x6AE4`
- The upstream physics notes map cleanly into this build's EXE image at segment `1A2`, including:
  - `LOC 2308` -> file offset `0x3F28`
  - `LOC 2369` -> file offset `0x3F89`
  - `LOC 26C7` -> file offset `0x42E7`
  - `LOC 26CD` -> file offset `0x42ED`
  - `LOC 2A23` -> file offset `0x4643`
  - `LOC 2A4E` -> file offset `0x466E`
- Simple constant probes in `SKYROADS.EXE` also line up with known behavior:
  - demo-step divisor `0x0666` appears at file offsets `0x0C4A`, `0x0C73`, and `0x0CA0`
  - full fuel/oxygen value `0x7530` appears at file offsets `0x1D02`, `0x1D08`, `0x21A5`, `0x21AB`, `0x2C24`, `0x2C41`, `0x2C4F`, and `0x2C7E`
  - max forward speed `0x2AAA` appears repeatedly, including `0x1D9E`, `0x1DA9`, `0x26FC`, and `0x2707`
  - `TREKDAT` expansion constants `0x0410` and `0x0270` appear near file offsets `0x3C2C`, `0x3C2F`, and `0x3C87`
- The `DEMO.REC` decode path is now visible directly in the binary:
  - the code around `0x0C4A`, `0x0C73`, and `0x0CA0` loads `0x0666`
  - it reads current Z from `DS:9628`/`DS:962A`
  - it uses the returned index to read a byte from `DS:[BX + 0x962E]`
  - it stores decoded controls to `DS:933C` (accelerate/decelerate), `DS:9600` (left/right), and `DS:5488` (jump)
- The `TREKDAT` expander is also visible directly in the binary:
  - the routine near file offset `0x3C78` loads `0x0410`
  - it performs `rep movsw` with `0x0138`, matching the 624-byte pointer-table copy
  - it then performs the same `3-byte header + span pairs + inserted zero byte until 0xFF` expansion used by the current extractor
- The runtime data/stack segment used by the renderer starts at image offset `0x66E0`, the same relocated region used by OpenRoads' `ExeReader`.
- The startup loader populates an 8-entry `TREKDAT` segment table at `SS:0x0E82`:
  - it zeroes `DS:54B4`
  - it loops 8 times
  - each iteration allocates a segment, stores it to `SS:[0x0E82 + 2 * index]`, and expands one `TREKDAT` record into that segment
- The main road draw routine begins near image offset `0x2D03` and uses:
  - `current_row >> 3` as the coarse road row index
  - `current_row & 7` as the active `TREKDAT` ring-buffer slot
  - `SS:[0x0E82 + 2 * slot]` as the selected expanded `TREKDAT` segment
  - `0x1638 + row_group * 0x0E + 0x62` as the road-byte source
- Road cells are 16-bit descriptors, and the renderer dispatch kind comes from the low nibble of the descriptor's second byte (`(value >> 8) & 0x0F`), not from `value & 0x0F`.
- The road-tile draw-type dispatch table lives at `SS:0x0B7F` inside the runtime segment:
  - type `0` -> `0x2E50`
  - type `1` -> `0x303D`
  - type `2` -> `0x2E9F`
  - type `3` -> `0x2EE1`
  - type `4` -> `0x2F3C`
  - type `5` -> `0x2FB0`
  - types `6` through `15` -> `0x3AAD` (`ret`/no-op)
- Shipped road data only uses dispatch kinds `0..5`; the no-op slots `6..15` do not appear in the current `ROADS.LZS`.
- The current road descriptor catalog contains `170` distinct raw 16-bit values.
- Dispatch-kind cell counts in the shipped road set are:
  - kind `0`: `25781`
  - kind `1`: `987`
  - kind `2`: `2132`
  - kind `3`: `268`
  - kind `4`: `1079`
  - kind `5`: `189`
- Common base descriptors are strongly structured:
  - kind `0` centers on `0x0000..0x000F`
  - kind `1` centers on `0x0100..0x010F`
  - kind `2` centers on `0x0200..0x020F`
  - kind `3` centers on `0x0300..0x030F`
  - kind `4` centers on `0x0400..0x040F`
  - kind `5` centers on `0x0500..0x050F`
- The renderer also uses an initialized 8-byte classification table at `DS:0x0B77` with values `[1, 2, 3, 3, 4, 4, 1, 1]`.
- Two renderer backend helper sets are selected during initialization:
  - when `[0x36] == 1`: `setup=0x348B`, `emit_a=0x3137`, `emit_b=0x3174`, `advance=0x323F`
  - when `[0x36] != 1`: `setup=0x3462`, `emit_a=0x3083`, `emit_b=0x30D9`, `advance=0x31BF`, plus the extra normalize pass at `0x3A23`
- A scripted DOSBox-X startup trace is now working locally and confirms the untouched DOS startup asset order:
  - first-second window: `SKYROADS.EXE`, `SKYROADS.CFG`, `MUZAX.LZS`, `OXY_DISP.DAT`, `FUL_DISP.DAT`, `SPEED.DAT`, `DEMO.REC`, `TREKDAT.LZS`
  - by ten seconds: the same path continues into `INTRO.LZS`, `ANIM.LZS`, and `INTRO.SND`
- A 25-second untouched run still loads only the same intro assets, so the game remains inside the intro pipeline and does not naturally advance to menu/world assets within that observation window.

## What The Current Tooling Does

[`tools/skyroads_extract.py`](/Users/ammaar/Development/skyroads/tools/skyroads_extract.py) currently:

- identifies image resources from the original `.LZS` files
- decodes `CMAP`/`PICT` resources
- writes exact palette bytes and pixel index buffers
- emits `.ppm` previews for image resources
- unpacks `ROADS.LZS` into raw tile buffers and JSON row dumps
- parses all 8 `TREKDAT` records exactly
- writes each `TREKDAT` record's decompressed payload and expanded DOS buffer
- exports each `TREKDAT` pointer table as a `13 x 24` JSON grid
- decodes every `TREKDAT` shape into span metadata and emits false-color contact-sheet previews
- parses the 20-entry `MUZAX` song table exactly
- decompresses each populated `MUZAX` song independently
- splits each `MUZAX` song into raw payload, instrument bytes, command bytes, and JSON analysis
- exports `INTRO.SND` and `SFX.SND` as raw unsigned PCM plus `.wav` wrappers at `8000 Hz`
- decodes `DEMO.REC` into per-step controller states
- exports the HUD `*.DAT` files as raw fragment indices, per-fragment previews, and full composite overlays
- parses the MZ structure of `SKYROADS.EXE`
- exports the EXE load module, embedded `NUMBERS`/`JUMPMASTER` image sets, and mapped original physics locations
- extracts initialized runtime renderer tables from the EXE data segment
- emits focused executable reports for the demo-index path, TREKDAT expansion path, physics constant anchors, and the TREKDAT renderer path
- exports a machine-readable road descriptor catalog and summary for the renderer bitfield model

[`tools/skyroads_dosbox_trace.py`](/Users/ammaar/Development/skyroads/tools/skyroads_dosbox_trace.py) currently:

- runs the original DOS executable under DOSBox-X with `INT 21h` and file-I/O logging enabled
- captures the full PTY session to a raw startup log
- parses open/read/close events into `summary.json` and `summary.md`
- makes it practical to measure the original startup asset order without hand-grepping emulator output

[`tools/skyroads_dos_oracle.py`](/Users/ammaar/Development/skyroads/tools/skyroads_dos_oracle.py) currently:

- launches the original DOS executable under the DOSBox-X debugger with a PTY-backed raw log
- waits for debugger prompts deterministically and can script breakpoints, register captures, and binary memory dumps
- resolves EXE image-offset breakpoints against the live runtime code segment, so the Road-0 presets now target the road-renderer entrypoint at image offset `0x2D03` as runtime `0824:2D03` in the current build
- captures `CS:IP`, `DS`, `SS`, renderer-state bytes, the TREKDAT segment table, and the EXE runtime tables into a machine-readable checkpoint bundle
- supports both staged DOS-side BIOS keyboard injection and direct-preload capture modes for Road-0 startup/gameplay probing without host keystroke automation
- can optionally drive the DOS intro/menu path on macOS with timed host key events so the first gameplay render hit can be captured without hand-timing

The native Rust workspace currently adds:

- [`crates/skyroads-data`](/Users/ammaar/Development/skyroads/crates/skyroads-data): a no-dependency data crate that reproduces the shared SkyRoads decompressor plus exact `ROADS.LZS`, `DEMO.REC`, `TREKDAT.LZS`, `MUZAX.LZS`, `SKYROADS.EXE`, and level/collision semantics derived from the shipped road descriptors
- [`crates/skyroads-core`](/Users/ammaar/Development/skyroads/crates/skyroads-core): a deterministic core layer that now covers exact demo sampling by fixed-point Z, row-level renderer planning, explicit gameplay-frame planning, a native gameplay session, and a top-level attract-mode app state machine for intro, menu, help, settings, demo playback, and live gameplay entry from the main menu
- [`crates/skyroads-cli`](/Users/ammaar/Development/skyroads/crates/skyroads-cli): a verifier CLI that loads the original DOS assets, prints the native baseline summary, and can run a text-trace simulation of the shipped demo
- [`crates/skyroads-renderer-ref`](/Users/ammaar/Development/skyroads/crates/skyroads-renderer-ref): a CPU-first reference renderer that now composes original intro/menu/help/settings art, all shipped world backdrops, dashboard assets, car art, and game-over overlays into a native `320x200` framebuffer
- [`crates/skyroads-audio-ref`](/Users/ammaar/Development/skyroads/crates/skyroads-audio-ref): a reference audio path that plays `INTRO.SND`, parses `MUZAX`, schedules deterministic music events, and mixes native PCM plus `SFX.SND` playback at `48 kHz`
- [`crates/skyroads-sdl`](/Users/ammaar/Development/skyroads/crates/skyroads-sdl): a zero-dependency SDL host that now acts as a thin macOS platform shell around the attract-mode app, reference renderer, and reference audio mixer
- asset-backed Rust tests that assert the current shipped roads, demo, TREKDAT, MUZAX, EXE runtime tables, and gameplay/session expectations still match the verified DOS-derived numbers

Usage:

```bash
python3 tools/skyroads_extract.py summary --source .
python3 tools/skyroads_extract.py extract --source . --output extracted
python3 tools/skyroads_dosbox_trace.py --source . --output startup-trace --time-limit 10
python3 tools/skyroads_dos_oracle.py --source . --output road0-oracle --preset road0-initial-frame --capture-screen
cargo run -p skyroads-cli -- summary .
cargo run -p skyroads-cli -- demo-sim . 120
cargo run -p skyroads-sdl -- .
cargo test
```

Current native-playable status:

- `cargo run -p skyroads-sdl -- .` now launches a native build that starts in the intro flow, reaches the original menu path, supports live gameplay from `Start`, and still transitions into recorded demo playback on idle
- the default SDL path no longer uses the old top-down debug view
- the native gameplay path already uses original world/dashboard/car/game-over assets and native music/SFX playback, but the remaining large fidelity gap is the gameplay/demo road renderer: the current forward scene is asset-backed and uses real level/session state, not the final DOS-exact TREKDAT span renderer
- the core/render boundary no longer exports guessed ship pose fields; it now passes exact ship simulation/control inputs into the renderer so DOS-specific pose and placement can be derived inside the renderer or from DOS oracle captures
- static disassembly of the live gameplay caller at `0x0E0E` now shows that DOS passes `current_row` to the renderer in eighth-tile units, not whole-tile units; the native core has been corrected to use the same scaled row counter for `>> 3` group selection and `& 7` TREKDAT slot selection

## Port Plan For A True DOS-Faithful Reimplementation

1. Lock down every shipped file format.
2. Extract all art, road data, renderer tables, music data, sound data, and demo data into deterministic fixtures.
3. Capture runtime behavior from the DOS build:
   - frame timing
   - simulation step order
   - input polling cadence
   - RNG usage
   - menu flow
   - audio playback behavior
4. Rebuild the engine around those fixtures instead of improvising modern behavior.
5. Add equivalence checks:
   - same level geometry
   - same player state after demo playback
   - same menu transitions
   - same palette and animation sequencing
   - same sound/music event ordering
6. Only after the data path is stable, tackle the remaining executable-only behavior inside `SKYROADS.EXE`.

## Immediate Next Reverse-Engineering Targets

- Finish mapping the `13 x 24` `TREKDAT` pointer-grid axes to exact on-screen primitives now that the native attract-mode host can exercise the real gameplay state and reference renderer live.
- Determine which road tile values map to the six active draw routines and what the remaining `6..15` no-op dispatch slots represent in shipped levels.
- Replace the current interim forward demo/gameplay scene with a software-reference renderer path that matches DOS ordering, composition, and palette behavior exactly.
- Interpret `MUZAX` command semantics beyond the structural 2-byte command format and tie them back to exact OPL behavior.
- Characterize the non-image metadata between some `ANIM.LZS` frames so animation pacing is reproduced exactly.
- Use `DEMO.REC` as a deterministic equivalence fixture against the DOS executable and confirm that the DOS indexing/cadence matches the current interpretation exactly.
- Confirm the exact draw order and composition rules for the extracted HUD `*.DAT` fragments inside the live DOS dashboard code path.
- Run the DOS executable in a controlled emulator and record frame-by-frame behavior for one known demo path; startup tracing now works, but frame-accurate control/debugger scripting still needs a reliable harness in this environment.

## Useful External References

- ModdingWiki overview: [https://moddingwiki.shikadi.net/wiki/SkyRoads](https://moddingwiki.shikadi.net/wiki/SkyRoads)
- ModdingWiki image format: [https://moddingwiki.shikadi.net/wiki/SkyRoads_Image_Format](https://moddingwiki.shikadi.net/wiki/SkyRoads_Image_Format)
- ModdingWiki roads format: [https://moddingwiki.shikadi.net/wiki/SkyRoads_Roads_Format](https://moddingwiki.shikadi.net/wiki/SkyRoads_Roads_Format)
- ModdingWiki CFG/save format: [https://moddingwiki.shikadi.net/wiki/SkyRoads_CFG_Format](https://moddingwiki.shikadi.net/wiki/SkyRoads_CFG_Format)
- TASVideos mechanics notes and memory references: [https://tasvideos.org/1535M](https://tasvideos.org/1535M)
- OpenRoads upstream reference implementation: [https://github.com/anprogrammer/OpenRoads](https://github.com/anprogrammer/OpenRoads)
