# Prompt Log

Chronological prompt log from the user that drove this reverse-engineering and native-port session.

Notes:

- Prompts are preserved as closely as possible to the original wording.
- Attached screenshots/images are noted inline, but not embedded here.
- Large implementation plans are kept in full because they were part of the actual steering for the work.

## Prompt 01

```text
I have this opensource DOS game that I want to reverse engineer, I want to unpack and basically recreate all of it - but it has to be completely true to what the game is in DOS so we need a true 1:1 port, explore this repo and lets make this reverse engineer and port happen
```

## Prompt 02

```text
check the readme again I think its fine to do - anyway im excited lets continue!!!
```

## Prompt 03

```text
ok well continue! and you can search the web to see what modders etc have done too if it helps at all
```

## Prompt 04

```text
OK do whatever it takes
```

## Prompt 05

```text
sick lets continue!!!
```

## Prompt 06

```text
ok continue!
```

## Prompt 07

```text
ok continue
```

## Prompt 08

```text
Ok go and do whatever you need to if you need to get a dos thing then do it - I want this shit to work fully
```

## Prompt 09

```text
Dont stop till its fully done and i want to be at a poont where once we have it fully reerse engineered we can port it to mac etc
```

## Prompt 10

```text
ok awesome lets go!
```

## Prompt 11

```text
OK lets do it! Keep going!
```

## Prompt 12

```text
OK keep going!
```

## Prompt 13

```text
Awesome keep going - we need to have a playable version ASAP so I want you to push ahead
```

## Prompt 14

```text
OK excellent so create a plan
- I want a fully playable demo that includes the graphics, sounds, everything of the origiinal, right now we only have a debug view
- Continue with our reverse engineer and port fully
```

## Prompt 15

```text
PLEASE IMPLEMENT THIS PLAN:
# SkyRoads DOS-Faithful Demo-First Port Plan

## Summary

- **Milestone A: fully playable attract-mode demo on macOS via `cargo run`**
  - Native build starts in the original intro flow, renders only original assets, plays original intro/music/SFX, reaches the original menu flow, runs the recorded demo with the real native simulation, and returns to the menu.
  - This milestone replaces the current debug rectangles with a DOS-style `320x200` indexed framebuffer and real audio output.
- **Milestone B: full gameplay port**
  - Add gameplay entry from the menu, full road/TREKDAT renderer, HUD, world backdrops, win/lose flow, and all 31 shipped roads.
- **Milestone C: ship-complete port**
  - Finish settings/help/go flows, exact CFG persistence, remaining fidelity gaps, and package the macOS app bundle.

## Key Implementation Changes

### 1. Extend the data/research layer so the demo can use only original assets

- Add new `skyroads-data` modules for:
  - image/menu archives: `INTRO.LZS`, `ANIM.LZS`, `MAINMENU.LZS`, `SETMENU.LZS`, `HELPMENU.LZS`, `GOMENU.LZS`, `WORLD*.LZS`, `DASHBRD.LZS`, `CARS.LZS`
  - HUD fragment packs: `OXY_DISP.DAT`, `FUL_DISP.DAT`, `SPEED.DAT`
  - audio assets: `INTRO.SND`, `SFX.SND`
  - config: `SKYROADS.CFG`
- Expose exact typed loaders:
  - `load_image_lzs_* -> ImageArchive`
  - `load_dashboard_dat_* -> HudFragmentPack`
  - `load_intro_snd_* -> Pcm8Sample`
  - `load_sfx_snd_* -> SfxBank`
  - `load_cfg_* -> SkyroadsCfg`
- Preserve unresolved fields as raw bytes in public types until DOS behavior is proven.
- Finish two reverse-engineering closures before renderer/audio implementation depends on them:
  - `ANIM.LZS` wrapper/timing semantics
  - enough `MUZAX` command semantics to schedule real music playback deterministically
- Add DOS capture tooling for:
  - intro/menu/demo state order
  - selected frame snapshots or CRCs
  - selected music/event traces
  - menu input/timing traces where DOSBox-X can expose them

### 2. Add a real app state machine in `skyroads-core`

- Expand `skyroads-core` from gameplay/session primitives into a top-level fixed-step app runtime with explicit states:
  - `Boot`
  - `Intro`
  - `MainMenu`
  - `HelpMenu`
  - `SettingsMenu`
  - `GoMenu`
  - `DemoPlayback`
  - `Gameplay` (phase B)
  - transition/fade helper state if the DOS flow requires it
- Add public core-facing types:
  - `AppMode`
  - `AppTickResult`
  - `RenderScene`
  - `AudioCommand`
  - `MenuId`
  - `MenuCursor`
  - `IntroSequenceState`
  - `DemoPlaybackState`
- Keep the core pure:
  - tick rate fixed at `70 Hz`
  - no SDL/window/audio calls in core
  - host supplies input and consumes render/audio commands
- Milestone A behavior:
  - boot/load defaults or CFG
  - play intro animation and intro sample
  - support skip keys
  - enter original menu flow
  - support menu navigation for the screens needed by attract mode
  - launch recorded demo from idle/menu path
  - return to menu when demo ends
- Milestone B behavior:
  - wire `Gameplay` into the same state machine instead of building a separate runtime path
  - reuse the existing native `GameplaySession` for ship physics and resource logic

### 3. Replace the debug view with a reference renderer crate

- Add `crates/skyroads-renderer-ref` as the CPU-first renderer.
- Public renderer interface:
  - input: `RenderScene`
  - output: `FrameBuffer320x200Indexed`
  - palette returned alongside pixel indices
- Renderer responsibilities:
  - PICT/image blitting with original palettes
  - intro/menu/help/settings/go/world screen composition
  - dashboard DAT fragment composition
  - HUD number/jumpmaster overlays from EXE-embedded assets
  - TREKDAT span rendering and road-cell dispatch for gameplay scenes
- Rendering rules:
  - output stays in original `320x200` indexed form
  - SDL host only scales and presents it
  - nearest-neighbor scaling, aspect-correct letterboxing, no filtering
- Do not extend the current debug drawing path.
  - Keep a debug overlay only as an optional developer toggle layered on top of the reference framebuffer.
- Milestone A renderer scope:
  - intro/menu/demo visuals must all come from original extracted assets
  - gameplay road renderer can remain incomplete internally, but demo playback visuals must no longer be the debug top-down view
- Milestone B renderer scope:
  - close the TREKDAT pointer-grid mapping
  - implement all six live road draw kinds from the EXE tables
  - match DOS road/HUD/world composition order

### 4. Add a reference audio crate and wire it through the host

- Add `crates/skyroads-audio-ref`.
- Public audio interface:
  - input: `AudioCommand` stream from core
  - output: mixed PCM blocks for the host callback
- Audio design:
  - SDL output target: `48 kHz` mono
  - `INTRO.SND` and `SFX.SND` use original unsigned `8-bit @ 8000 Hz` data, upsampled into the mix buffer
  - `MUZAX` is decoded into deterministic channel/register events
  - use a local pure-Rust OPL2/YM3812-compatible synth path in-repo so music playback is native and testable
- Milestone A audio scope:
  - intro sample playback
  - menu/demo music scheduling from `MUZAX`
  - SFX path ready even if attract mode uses only a subset
- Milestone B audio scope:
  - gameplay SFX triggers
  - exact per-state music selection and transitions
- The audio crate must expose an inspectable event timeline so tests can verify sequencing before waveform-perfect tuning is finished.

### 5. Turn `skyroads-sdl` into a thin host

- `skyroads-sdl` becomes only:
  - SDL window creation
  - texture upload/presentation of `320x200` frames
  - keyboard input capture
  - SDL audio callback
  - file-root selection
- Default run target:
  - `cargo run -p skyroads-sdl -- .`
- Milestone A host behavior:
  - boot directly into the real intro/menu/demo app state machine
  - use DOS-style menu/navigation keys as closely as possible
  - keep current debug controls only behind an explicit debug mode, not the default UX
- Milestone C adds:
  - macOS app bundle
  - asset-location rules for bundle vs workspace runs
  - config/save path placement

## Test Plan

- **Parser/data tests**
  - each shipped image archive parses and reports expected frame/palette counts
  - DAT/HUD packs parse to expected fragment counts
  - `INTRO.SND` and `SFX.SND` lengths/counts match current verified numbers
  - `MUZAX` song headers and command/event decoding stay deterministic
- **Core/state tests**
  - intro starts in the correct state and advances on the correct tick schedule
  - skip input exits intro correctly
  - idle/menu flow enters demo playback correctly
  - demo EOF returns to menu
  - gameplay state remains wired through the same app runtime
- **Renderer tests**
  - framebuffer CRC/golden tests for:
    - first intro frame
    - one mid-intro animation frame
    - main menu
    - help/settings/go screens
    - selected demo frames
  - later add gameplay frame CRCs once TREKDAT mapping is closed
- **Audio tests**
  - intro sample duration and buffer boundaries
  - MUZAX event decoding for first N commands of selected songs
  - deterministic mixed-buffer hashes for short intro/menu/demo windows
- **DOS equivalence checks**
  - startup file order stays matched to the existing DOSBox-X trace
  - selected intro/menu/demo frame CRCs or screenshots are compared against DOS captures
  - selected demo-state checkpoints compare ship position/resources/state against DOS-derived references
- **Milestone A acceptance**
  - `cargo run -p skyroads-sdl -- .` launches a native attract-mode build with original graphics and audio
  - no default debug top-down rendering remains
  - intro -> menu -> demo -> menu loop is complete and deterministic
- **Milestone B acceptance**
  - gameplay can be started from the native menus
  - all shipped road data renders through the reference renderer
  - HUD/world/audio flow works during live play

## Assumptions And Defaults

- First major milestone is **attract-mode fidelity**, not full gameplay completeness.
- DOS behavior is the spec; OpenRoads is allowed only as a research oracle, not as an unverified shipped behavior source.
- First deliverable is a **documented native run target on macOS**, not an app bundle.
- Menu/config persistence that is not required for intro/menu/demo correctness can land after Milestone A, but menu rendering and navigation for the attract-mode path must be present in Milestone A.
- The current `skyroads-sdl` debug host remains available only as an internal validation path until the reference renderer fully replaces it.
```

## Prompt 16

```text
March ahead I want a plan to get me to playable version with the assets etc asap
```

## Prompt 17

```text
do it
```

## Prompt 18

```text
Ok run it so I can play
```

## Prompt 19

```text
OK so I get to start but as you can see the map isnt rendering at all and i cant see my ship etc
```

Attachment: screenshot

## Prompt 20

```text
OK so better but still broken from where we need it to be (see the correct version from the original)
```

Attachment: screenshots

## Prompt 21

```text
Yep! And I thnk the thing youre seeing on the left here in my screenshot is actually the ship explosion sprite when it hits someting if im not wrong
```

## Prompt 22

```text
OK the ship rendering and map is still broken
If you look at our /private/tmp/skyroads_render_inspect/images/cars/frame_000.png file - you will see that the shop is there near the absolute bottom and each is basically a sprite frame of the ships different states all the way till it explodes. It also seems like the sheet needs rotation 90 degrees as the ship is showing up on its side.

And then again you can see in the screenshot the map is just not the same as the game and the ship was static and not working so we need a plan to fix this and I want a playable version that isnt this broken

Also can we for fun change something int he intro menu to show "Codex Port" in the text somewhere
```

Attachment: screenshot

## Prompt 23

```text
PLEASE IMPLEMENT THIS PLAN:
# SkyRoads Playable Renderer Recovery Plan

## Summary

Use a **balanced** path: get the native build to a visually convincing, actually playable state immediately, but structure each renderer change so it becomes the final DOS-faithful renderer instead of throwaway code.

The best proof of reverse engineering is **not** patching the shipped DOS files. It is making the native port intentionally compose new behavior from the reversed formats and renderer rules. So `Codex Port by Codex 5.4 and Ammaar` should be added by the native intro/menu renderer, while the original asset files stay untouched.

Success for this plan:
- gameplay window shows a real road, not a black field or placeholder blocks
- ship uses the correct extracted sprite frames, orientation, and state changes
- gameplay is readable and controllable from start to crash/win
- intro/menu visibly includes `Codex Port by Codex 5.4 and Ammaar`
- the new gameplay renderer is built as a stepping stone into the exact `TREKDAT` path, not a dead-end approximation

## Key Changes

### 1. Add explicit gameplay visual state to the core

Extend the gameplay render payload from `skyroads-core` so the renderer no longer guesses from raw physics alone.

Public/interface changes:
- add a `ShipVisualState` struct to the core render scene payload
- include:
  - `sprite_kind`: `alive | exploding | destroyed`
  - `bank`: `left | center | right`
  - `thrust_on`
  - `jumping`
  - `explosion_frame`
  - `ship_screen_bias_x`
- include the current row window together with absolute row indices, not just raw copied rows
- include camera-facing data needed by the renderer:
  - `current_row`
  - `fractional_z`
  - `world_index`
  - `did_win`
  - `craft_state`

Behavior rules:
- bank is driven by held left/right input
- thrust state is driven by accelerate/decelerate input and/or current z-velocity delta
- explosion frames advance deterministically after crash
- demo and live gameplay use the same visual-state path

### 2. Replace the broken ship draw with a real car-atlas pipeline

Stop treating `CARS.LZS` as a normal one-frame image. It is a vertical sprite atlas and must be parsed that way.

Implementation:
- add a `CarAtlas` helper in the renderer/data layer that:
  - reads the single `CARS.LZS` frame
  - segments it into individual sprites by vertical runs
  - trims transparent columns per segment
  - rotates the extracted sprites into the correct gameplay orientation
  - classifies segments into:
    - ship frames
    - explosion frames
    - non-gameplay effect sprites
- define an explicit frame map:
  - `alive_center`
  - `alive_left`
  - `alive_right`
  - `jump_center`
  - `jump_left`
  - `jump_right`
  - ordered explosion frames
- choose the sprite from `ShipVisualState`, not from ad hoc index guesses
- anchor the ship to the correct gameplay position above the dashboard, with deterministic pixel coordinates
- ship motion is screen-space only for now:
  - horizontal bias follows steering/bank
  - jump state moves the ship upward slightly
  - crash uses explosion frame progression

Acceptance:
- no atlas junk appears on screen
- no sideways ship appears on screen
- alive gameplay always shows a centered playable craft
- crashing shows explosion animation instead of the live ship

### 3. Replace the placeholder road pass with a proper transitional road renderer

The current row painter is still conceptually wrong. Replace it with a structured road renderer that uses the actual road descriptors and is deliberately shaped like the future `TREKDAT` renderer.

Implementation:
- introduce a dedicated `RoadRenderer` in `skyroads-renderer-ref`
- phase 1 road pass must render:
  - continuous road deck from contiguous `has_tile` spans
  - shoulders/edge lines
  - gaps
  - kill/boost/slide/refill tile coloration
  - cubes/towers as vertical roadside or on-road geometry
  - tunnel presence as overhead/side obstruction cues
- projection rules:
  - one projected slice per gameplay row in the window
  - far-to-near painter’s order
  - width, center shift, and horizon derived from row distance plus ship lateral offset
  - spans rendered as surfaces, not per-cell columns
- geometry rules:
  - a contiguous run of road cells becomes one deck polygon/slice span
  - cube-height cells render as raised obstacles, not flat tiles
  - empty cells remain void
- architecture rule:
  - the road pass should consume a per-row intermediate form like `ProjectedRoadSlice`
  - later, `ProjectedRoadSlice` gets replaced by exact `TREKDAT` span output instead of rewriting the whole renderer

Immediate output quality target:
- gameplay should resemble the original at a glance:
  - visible forward road
  - clear void on both sides
  - recognizable obstacles and pads
  - ship visually sitting on the track
- it does **not** need to be DOS-exact yet, but it must stop looking broken

### 4. Start the exact `TREKDAT` migration in parallel, not after

Do not wait until the transitional renderer is “done.” Build the exact renderer path behind the same surface API.

Implementation:
- map the `13 x 24` pointer grid into a named intermediate:
  - row depth index
  - column/primitive selector
  - dispatch kind
- derive a `TrekdatRenderer` module that emits DOS-style projected spans into the same framebuffer interface as the transitional `RoadRenderer`
- migrate road features in this order:
  1. flat road deck
  2. road edges/void boundaries
  3. cubes/raised geometry
  4. tunnel structures
  5. special tile appearances
  6. crash/occlusion correctness
- once the exact path matches the transitional one on enough scenarios, make it the default and keep the transitional path only as a debug fallback

### 5. Add the Codex branding as a native rendered menu element

Add `Codex Port by Codex 5.4 and Ammaar` to the intro/menu flow without modifying the original DOS files.

Implementation:
- render the text in the native renderer as part of intro/menu composition
- placement:
  - main menu: centered below the existing SkyRoads logo/menu selection art
  - intro: optional smaller line on the title hold frame only, not on every intro credit frame
- style:
  - small crisp pixel text
  - palette chosen from existing menu colors
  - must look intentional and in-world, not like window chrome
- implementation method:
  - use a native pixel-font rasterizer or extracted glyph source if a clean one is available
  - do not edit `INTRO.LZS`, `MAINMENU.LZS`, or the original DOS binary/data
- keep the text always on in the native port, not behind a debug flag

## Test Plan

### Parser / asset tests
- `CARS.LZS` atlas segmentation yields stable sprite count and stable dimensions for the chosen ship/explosion frames
- rotated ship frames have the expected aspect ratio and non-empty bounds
- branding text renderer produces deterministic output bounds and palette indices

### Core / state tests
- `Start` still enters gameplay from the main menu
- steering input changes `ShipVisualState.bank`
- crash transitions into explosion visual state and advances explosion frames deterministically
- demo playback and live gameplay both produce render scenes with populated visual state

### Renderer tests
- gameplay frame test:
  - ship sprite is present in the correct screen region
  - no giant atlas strip appears
- playfield test:
  - significant non-black pixel count in the gameplay area
  - significant non-background road pixel count in the center corridor
- golden frames:
  - main menu with `Codex Port by Codex 5.4 and Ammaar`
  - first live gameplay frame
  - first crash frame
  - one mid-demo frame
- later golden replacement:
  - compare selected gameplay frames against DOS captures once `TREKDAT` path is active

### Manual acceptance
- launch `cargo run -p skyroads-sdl -- .`
- skip intro, enter gameplay, and verify:
  - ship is upright and visible
  - steering visibly affects ship presentation
  - road is visible and readable
  - obstacles/pads are recognizable
  - crash shows explosion frames
  - menu shows `Codex Port by Codex 5.4 and Ammaar`

## Assumptions And Defaults

- Priority is **Balanced**: every step must improve playability and move toward the DOS renderer structure.
- `Codex Port by Codex 5.4 and Ammaar` will be rendered by native code, not by modifying shipped DOS asset/binary files.
- The next milestone is a **non-broken playable build**, not immediate pixel-perfect DOS equivalence.
- The transitional road renderer is allowed only if its interfaces are compatible with the final `TREKDAT` renderer and can be replaced module-for-module.
- The current SDL host, core state machine, and audio path remain the host/runtime foundation; the main work is now concentrated in render-scene data, ship atlas handling, and road rendering.
```

## Prompt 24

```text
OK much closer but here are the issues
- the ship still doesnt fully sit on the track but def much better
- once it falls it stays pointing downwards but can fly through the whole track and keeps making its collision sounds
- Some track parts are rendering better, some blocks appear, tunnels are not appearing at all
```

Attachments: screenshots

## Prompt 25

```text
Yeah I would remove ALL of the guesswork and disassemble everything to fix this and be true to source
```

## Prompt 26

```text
Run the build again since i closed it
```

## Prompt 27

```text
OK So you can see that the ship doesnt even start on the track and is pointing to the left instead of its stationary look.
Track rendering seems better depth wise but still feel like some parts of the track are definitiely missing.

I would create a plan where there's zero guess work and we are reverse engineering the exact pieces we need for this to be a 1:1 playable level with the right track, collisions, etc
```

Attachment: screenshot

## Prompt 28

```text
PLEASE IMPLEMENT THIS PLAN:
# SkyRoads Road-0 Exactness Plan

## Summary

Use `Road 0` as the single exact vertical slice and treat the original DOS EXE plus DOS captures as the only truth source. The goal of this plan is to remove all guessed renderer/state logic and replace it with ported DOS routines so that one shipped level is 1:1 for track rendering, ship pose, collisions, death, and SFX before expanding to the rest of the game.

The immediate failures in your screenshot map cleanly to missing DOS logic:
- ship start pose/screen placement is still inferred, not ported
- neutral ship sprite selection is still inferred
- road/tunnel composition is still missing parts of the DOS normalization and draw pipeline
- collision/death/SFX gating is still using native approximations instead of the original state machine

## Key Changes

### 1. Build a DOS oracle for one exact level
- Add a deterministic `Road 0` capture harness around the original DOS build.
- Capture, at fixed startup and gameplay checkpoints:
  - full frame images
  - ship state: `x/y/z`, `x/y/z velocity`, state, control input
  - renderer state: `current_row`, `current_row >> 3`, `current_row & 7`
  - collision/death transitions
  - SFX trigger sequence
- Capture checkpoints for:
  - first gameplay frame
  - first throttle frame
  - first steer-left and steer-right frame
  - first jump
  - first tunnel entry
  - first obstacle contact
  - first fall/death
- Treat these captures as golden fixtures for the port. No renderer or physics change is accepted unless it matches the captured checkpoints.

### 2. Replace the guessed renderer path with the actual DOS renderer path
- Stop using the raw `13 x 24` TREKDAT pointer table as a direct drawing input.
- Port the DOS renderer in the same stages the EXE uses:
  - raw TREKDAT expansion from `0x3A7A`
  - secondary TREKDAT normalization from `0x3A23`
  - per-frame renderer setup and normalization from `0x3492..0x38A3`
  - cell draw handlers from `0x2E50`, `0x303D`, `0x2E9F`, `0x2EE1`, `0x2F3C`, `0x2FB0`
  - primitive blitters from `0x3083`, `0x30D9`, `0x3137`, `0x3174`, `0x31BF`, `0x323F`
- Use the DOS layout exactly:
  - active slot from `current_row & 7`
  - coarse road row from `current_row >> 3`
  - 13 TREKDAT rows arranged as `4 cells x 6 primitive pointers`
  - left and right halves rendered through the same handler logic the EXE uses
- Implement the original painter order and split passes so the ship sits between the same road layers as DOS.
- Do not keep the current heuristic road renderer as the default path. It may remain only as a debug fallback until the exact path is complete.

### 3. Remove guessed ship visual logic and port the DOS ship/camera path
- Replace guessed `ShipVisualState` fields such as inferred bank, screen bias, vertical offset, and explosion progression with exact DOS-derived values.
- Reverse engineer and port:
  - gameplay start/spawn initialization
  - camera/view placement that determines where the road begins on frame 1
  - ship screen placement over the road
  - neutral/left/right/jump/explosion sprite selection from the car atlas
  - death/fall visual state progression
  - shadow/contact visibility rules
- Explicitly verify the neutral first-frame gameplay pose:
  - ship is on the track
  - ship uses the stationary center sprite
  - ship is not pre-banked left or right
- Defer any non-DOS overlay or cosmetic changes during this phase so validation frames remain comparable.

### 4. Port exact collision, tunnel, and SFX behavior for the vertical slice
- Trace and port the exact Road-0 collision and death path from the DOS gameplay logic.
- Fix the current native mismatches by replacing inferred behavior for:
  - surface contact detection
  - tunnel occupancy and tunnel collision tests
  - obstacle/void interaction
  - fall/death transition gating
  - repeated collision SFX after death
- Validate tunnel presence using DOS captures, not just shape presence in TREKDAT.
- Acceptance for this phase:
  - first crash matches DOS state transition and SFX count
  - falling does not continue through track geometry incorrectly
  - tunnel frames show the same structural pieces and occlusion order as DOS

### 5. Reshape the native interfaces around exactness
- Change the gameplay-to-renderer boundary so it carries exact renderer inputs, not guessed visual outputs.
- Public/interface changes:
  - `DemoPlaybackState` should expose exact simulation/render inputs only:
    - ship snapshot
    - `current_row`
    - coarse road row window centered on `current_row >> 3`
    - control/state fields needed by the ported DOS routines
  - remove guessed visual fields from the scene API:
    - inferred bank
    - inferred screen bias
    - inferred vertical offset
    - inferred surface flag
  - `skyroads-data` should expose typed TREKDAT structures for:
    - raw expanded record
    - normalized per-cell pointer bundles
    - sequential primitive decoding for a given pointer
- Keep DOS renderer-specific derivation inside the renderer module so visual quirks come from the ported draw path, not ad hoc state pushed from gameplay.

## Test Plan

- **DOS oracle tests**
  - Road-0 capture bundle exists for every checkpoint listed above.
  - Stored checkpoint metadata includes frame number, row indices, ship state, and SFX events.

- **TREKDAT/renderer tests**
  - raw expansion bytes match the known DOS-expanded buffers
  - normalized per-cell TREKDAT data matches DOS-derived reference dumps
  - each of the six draw handlers reproduces the expected primitive sequence for selected Road-0 cells
  - golden framebuffer tests for:
    - first gameplay frame
    - first throttle frame
    - first steer frame
    - first tunnel frame
    - first obstacle frame
    - first death frame

- **Gameplay/state tests**
  - Road-0 gameplay start puts the ship on track at the DOS-captured position
  - first frame uses the neutral center sprite
  - collision/death path matches DOS checkpoints
  - repeated wall-hit SFX do not continue after the DOS death transition point

- **Vertical-slice acceptance**
  - `cargo run -p skyroads-sdl -- .` can start Road 0 and visually match DOS at the captured checkpoints
  - the track is structurally complete for the slice, including tunnels and missing pieces now absent in the screenshot
  - ship placement, sprite state, collisions, death, and SFX all match the DOS oracle for that slice

## Assumptions And Defaults

- First exactness target is `Road 0`, not all levels in parallel.
- The original DOS EXE plus DOS captures are the only authority for behavior and visuals.
- OpenRoads, modding notes, and community references may help locate code paths, but they are not acceptance or truth sources.
- Any current custom branding or non-DOS overlays are deferred until the Road-0 exact slice passes.
- The heuristic renderer/state bridge is not extended further; all remaining work flows through the ported DOS routines.
```

## Prompt 29

```text
OK keep pushing we need to play this today!
```

## Prompt 30

```text
Wait I could restart this because I did give permissions - but you will need to pause work for a second
```

## Prompt 31

```text
OK we're back lets continue and I think you will have permissions now too for any of the testing you wanted to do that mac was blocking
```

## Prompt 32

```text
ok agreed to it!
```

## Prompt 33

```text
OK got you some screenshots! This is looking so much better, I think its still not detecting the drop detection and keeps making that falling sound once its dropped once but can still move through the whole track.

Then you can see the ship peers out of cubes and tunnels, but this is def getting much better!
```

Attachments: screenshots

## Prompt 39

```text
OK that was all really helpful lets go back to the job! and continue
```

## Prompt 40

```text
i can choose a folder if that helps
```

## Prompt 41

```text
done
```

## Prompt 42

```text
Lets also push this to Git with a nice readme of everything so far that we've done to make this work and a sharable nice readme https://github.com/ammaarreshi/skyroads-reverse-eng-codex.git
```

## Prompt 43

```text
Oh and for the git repo, add all prompts that I've sent so far in a prompt.md file, I want to open source that too so people see how I've been prompting to get to this result
```
