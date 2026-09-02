# Expand Controller Support

Status: software implementation complete through automated Linux and native
Windows release-runtime smoke checks. Direct 8BitDo SN30 Pro X-input/Bluetooth
on native Windows now has partial physical evidence; the required hardware
matrix remains incomplete.

External contracts last researched: 2026-09-01. Plan updated: 2026-09-02.

## Goal

Make the native SDL host usable with an Xbox-compatible controller, Steam Input,
and an 8BitDo SN30 Pro from the intro through gameplay. Keep the deterministic
game core and the DOS-compatible configuration format independent of controller
brands and host input APIs.

The first supported outcome is one active controller and one local player. A
player must be able to navigate menus, select a level, steer, accelerate, brake,
jump, retry, and return from gameplay without switching back to the keyboard.
Mouse and controller sensitivity must also be fine-tunable without changing the
deterministic simulation or the DOS-faithful default behavior.

## Scope boundary

This plan covers:

- native Windows 11 controller input;
- Linux SDL input, including WSL2 when a controller is actually exposed to the
  Linux guest;
- Steam Input gamepad emulation/legacy mode;
- direct Xbox controller input;
- direct 8BitDo SN30 Pro input in its Windows/X-input mode;
- 8BitDo SN30 Pro Switch mode routed through Steam Input;
- live mouse/controller sensitivity tuning, preview, reset, and native
  persistence;
- startup discovery, unplug/reconnect handling, deterministic input
  normalization, diagnostics, documentation, and packaging.

This plan does not cover:

- the native Steam Input API, action manifests, Steam-specific glyphs, or a
  Steam AppID integration;
- SDL3 migration;
- controller remapping UI, calibration profiles, or per-device persistence;
- rumble, gyro, touchpad, battery reporting, LEDs, or firmware updates;
- multiple local players;
- changes to SkyRoads physics, demo playback, or the DOS `SKYROADS.CFG` schema.

Native Steam Input API integration would add a Steamworks SDK and distribution
boundary. If that becomes a requirement, decide it separately in an ADR before
implementation.

## Implementation status

Implemented on 2026-09-02:

- device-neutral snapshots, tuned normalization, menu hysteresis, edge latching,
  and DOS-default equivalence tests;
- the safe SDL2 mapped-controller boundary, raw index-0 fallback, positional
  face-button policy, typed device events, stable instance IDs, and explicit
  sampling errors;
- mapped-first discovery, neutral-on-disconnect replacement, startup/change
  diagnostics, and the asset-independent `--controller-diagnostics` mode with
  GUID, mapping, axis, and button evidence for hardware triage;
- controller navigation in every app mode, `JOYSTICK`-only gamepad movement,
  and the separate injected `--smoke-gamepad` flow;
- the focused `INPUT` page, live semantic previews, strict native preference
  parsing, change-only saves, and reset/persistence behavior;
- root documentation and canonical Windows/Linux package notes for the exact
  logical controls, setup paths, and unverified platform boundaries.

Automated tests, strict Clippy, the Linux release build, and both Linux
dummy-driver smoke paths pass. A Windows x86-64 MSVC release was also
cross-linked with the packaged SDL 2.32.10 DLL, launched through native Windows
interop on Windows build 10.0.26200.9278, and passed controller diagnostics plus
both dummy-driver smoke paths. This evidence belongs in the implementation
handoff rather than being treated as physical-device evidence.

On 2026-09-02, with Steam fully stopped, Windows PnP identified a directly
connected `8Bitdo SN30 Pro` over Bluetooth. Packaged SDL 2.32.10 exposed it as
one mapped `Xbox One S Controller` with GUID
`030044f05e040000e002000000007200`. An isolated run persisted `JOYSTICK`
(`SKYROADS.CFG` bytes 2-3 were `01 00`). The user then confirmed D-pad and
left-stick menu navigation, confirm/back, gameplay steering, both triggers,
south-position jump, and one transition rather than repetition while holding a
menu direction. The same process later observed the device absent, reselected
it as mapped instance ID 1 when it reappeared, and exited cleanly.

Controller sensitivity was exercised at `50%`, `100%`, and `200%`. Full stick
directions and both triggers remained reachable at `50%`; light stick and
trigger travel activated earlier at `200%`. The exact `50%` and `200%` values
were checked in `SKYROADS-RS-INPUT.CFG`, restored after fresh process starts,
and the in-game reset was checked both visually and on disk as `100% / 100%`.
A deliberate gameplay power-off while steering and accelerating released both
inputs instead of leaving either latched; reconnecting restored controller input
without restarting the game. Firmware was not available in the queried Windows
device properties. In a later run, driving off the road and pressing the
south-position button after the frozen crash frame restarted the selected road;
East/Back then exited gameplay to level selection. Completing Road 1 and
pressing South after the final tunnel also returned to level selection. This is
still partial evidence: controller firmware and USB equivalence remain pending.
It does not justify a repository-owned mapping or a changelog claim.

No `/dev/input` directory is exposed in the current Ubuntu 24.04 WSL2 session,
so both WSL controller rows remain environment-blocked. Native Xbox, Steam
Input, SN30 Pro USB, and SN30 Pro Steam/Switch rows remain unverified; the SN30
Pro Bluetooth row is partial. WSLg mouse input also remains unverified.

## Baseline implementation evidence

Before this work, the host had a narrow raw-joystick path:

- [`sdl.rs`](../crates/skyroads-sdl/src/sdl.rs) initializes
  `SDL_INIT_JOYSTICK`, opens SDL joystick index `0` once at startup, and reads
  raw axes `0`/`1` plus raw button `0`.
- [`main.rs`](../crates/skyroads-sdl/src/main.rs) samples that joystick only
  during gameplay and only when the persisted mode is `ControlMode::Joystick`.
  Controller input cannot currently navigate the intro, menus, settings, or
  level selection.
- The device is not rescanned after startup. Plugging in later or removing the
  active device is not handled.
- Raw axis and button numbering is device-specific. There is no SDL controller
  mapping, device identity, hot-plug event, or Steam virtual-controller logic.
- [`controller_state_from_dos_joystick`](../crates/skyroads-core/src/gameplay.rs)
  already provides the desired deterministic `-1 / 0 / 1` gameplay contract.
- Controller activation is fixed at the DOS-derived half-center threshold. Mouse
  steering and acceleration/braking use fixed centered 320x200 thresholds. No
  sensitivity model or native input-preference file exists yet.
- [`ControlMode::Joystick`](../crates/skyroads-data/src/cfg.rs) is persisted as
  DOS value `1`; that byte-level compatibility should remain unchanged.

At the time this plan was written, the active Ubuntu 24.04 WSL2 environment had
no `/dev/input` directory. That means no WSL hardware behavior has been verified
yet. It is an environment prerequisite, not evidence of an SDL or game defect.

## Planned design

### Use SDL2's standardized game-controller API

Prefer `SDL_GameController` over raw axis/button numbers. SDL's controller layer
maps known devices to stable logical axes and buttons, while the project remains
on its existing SDL2 runtime and hand-written FFI boundary.

Do not add direct XInput, Windows.Gaming.Input, or Steamworks polling alongside
SDL. Parallel input APIs create duplicate-device and lifetime problems and are
unnecessary for the requested devices.

Keep the existing raw `Joystick` wrapper as a reported fallback while the new
path is proven. The selection order is:

1. first mapped SDL game controller;
2. first raw joystick only when no mapped controller exists;
3. no active controller, producing neutral gameplay input.

Do not remove the raw fallback until supported-device evidence shows it is no
longer needed.

### Keep controller concerns in the SDL host

The proposed future input flow is:

```text
physical controller
        |
        +-- direct OS input -----------+
        |                              |
        +-- Steam Input emulation -----+--> SDL_GameController
                                               |
                                               v
                                      active-controller manager
                                               |
                                               v
                                      logical gamepad snapshot
                                               |
                              +----------------+----------------+
                              |                                 |
                              v                                 v
                    menu edge/held input             deterministic gameplay
                         AppInput                       ControllerState
```

Add a focused `gamepad.rs` module in `skyroads-sdl` for safe, pure normalization
and latching. Keep foreign declarations, C layouts, resource ownership, and SDL
error translation in `sdl.rs`. Let `main.rs` orchestrate device events and merge
gamepad input with the existing keyboard/mouse paths.

Do not add controller types to `skyroads-data`, and do not make the core know
about Xbox, Steam, 8BitDo, SDL axes, GUIDs, or button labels.

### Preserve the DOS settings model

Keep the settings-art label and persisted mode as `JOYSTICK` /
`ControlMode::Joystick`. It represents the gameplay input family, not a specific
hardware protocol.

Gamepad menu navigation must work regardless of the selected gameplay control
mode. This lets a player open Settings and select `JOYSTICK` using the controller
itself. Gamepad gameplay movement is applied only while
`ControlMode::Joystick` is selected.

Do not persist a GUID, device index, or controller name in `SKYROADS.CFG`.
Device indexes are transient, and adding fields would break the DOS-compatible
format.

Sensitivity is also a native host preference, not a DOS setting. Persist it in
`SKYROADS-RS-INPUT.CFG` and leave `SKYROADS.CFG` byte-for-byte compatible.

### Define one logical control map

Use position/intent in documentation rather than assuming that every controller
prints Xbox letters on its face. For example, SDL's logical `A` action normally
means the south face button after mapping, even when an 8BitDo shell prints a
different letter there.

| Context | Physical intent | SDL logical inputs | Host action |
| --- | --- | --- | --- |
| Intro and menus | navigate | D-pad or left stick | directional `AppInput` edges |
| Intro and menus | confirm/select | south face button or Start | `AppInput.enter` edge |
| Menus/gameplay | back | east face button or Back/View | `AppInput.escape` edge |
| Controls menu | previous/next display value | left/right shoulder | semantic setting edge for selected `DISPLAY` or `VIDEO MODE` |
| Gameplay | steer | D-pad left/right or left-stick X | `turn_input` `-1 / 0 / 1` |
| Gameplay | accelerate | D-pad/stick up or right trigger | `accel_input = 1` |
| Gameplay | brake | D-pad/stick down or left trigger | `accel_input = -1` |
| Gameplay | jump/retry | south face button | jump held during play; select edge after death/win |

The DOS-exact renderer adds no separate retry or win prompt. Hardware acceptance
should wait for the explosion, fall, or final tunnel scene, fully release the
south button, and press it again. Retry input is valid as soon as the core enters
a non-alive state; after a win, the fresh press returns to level selection.

Normalization rules:

- At the default `100%` controller sensitivity, reuse the existing DOS-faithful
  half-center gameplay threshold for left-stick axes: engage beyond signed
  magnitude `0x4000`.
- Give menu stick navigation hysteresis. At the default sensitivity, engage at
  `0x4000` and release at three quarters of the derived engage threshold. A held
  stick must not scroll every 70 Hz tick.
- At the default sensitivity, treat a trigger as pressed above half of SDL's
  `0..32767` trigger range.
- D-pad input takes priority over the stick on the same axis.
- When exactly one trigger is active, triggers take priority over left-stick Y.
  Both triggers, both directions, or otherwise contradictory input resolve to
  neutral rather than relying on branch order.
- Combine keyboard and controller menu booleans with logical OR. Never emit two
  app actions because Steam mapped one physical button to both keyboard and
  gamepad input.
- Keep edge state separate from held state. Holding the confirm button must not
  skip the intro and then immediately select multiple menu entries.
- On disconnect, publish a neutral snapshot before the next simulation tick so
  steering, throttle, brake, or jump cannot remain stuck.

### Make mouse and controller sensitivity fine-tunable

Model tuning with two required, validated values rather than device-specific
optional fields:

```text
InputTuning
  mouse_sensitivity: SensitivityPercent
  controller_sensitivity: SensitivityPercent
```

`SensitivityPercent` is an integer from `50` through `200`, adjusted in `5%`
steps. `100%` is the default and must reproduce the current thresholds exactly.
Values outside that range are invalid rather than silently clamped during
parsing.

Apply tuning only at the host normalization boundary:

- Mouse sensitivity changes the activation distance from the 320x200 center.
  Derive horizontal and vertical distances separately because the DOS defaults
  are different: X engages outside `0x0096..=0x00AA`, while Y engages outside
  `0x000F..=0x00B9`. Higher sensitivity reduces those distances; lower
  sensitivity increases them.
- Controller sensitivity changes the derived left-stick and trigger activation
  thresholds. Higher sensitivity lowers the threshold; lower sensitivity raises
  it. D-pad and button input remain digital and are unaffected.
- Raw-joystick fallback and mapped controllers use the same controller tuning
  after their values have been normalized into the common logical snapshot.
- Controller menu hysteresis is derived from the tuned engage threshold, so
  sensitivity changes do not reintroduce menu chatter.
- Use checked integer arithmetic. Every derived engage threshold must remain
  strictly below the device's maximum magnitude, and every mouse activation
  distance must remain inside the reachable framebuffer range. Even `50%` must
  still allow every action at full deflection.
- The conversion remains `-1 / 0 / 1`; tuning changes when an action engages,
  not its strength, the 70 Hz simulation, demo input, or physics.

Add a native `INPUT` settings entry that opens a focused tuning page instead of
overloading the recovered DOS control-mode row. The page must provide:

- `MOUSE SENSITIVITY` and `CONTROLLER SENSITIVITY` values;
- left/right adjustment in `5%` steps and an explicit `RESET TO 100%` action;
- immediate application without restarting or leaving the page;
- live mouse-axis and controller stick/trigger activation indicators, so a
  player can tune the connected device before starting a level;
- keyboard and gamepad navigation using the same edge semantics as other menus.

The settings scene may carry the device-neutral `InputTuning` value needed for
rendering, but raw SDL values and controller identities remain in the SDL host.
Load and save the two values through a focused `input_preferences.rs` module in
`skyroads-sdl`, following the strict error reporting and round-trip testing used
by `display_preferences.rs`. A missing file selects `100% / 100%`; malformed or
out-of-range content emits one actionable warning and falls back to those
defaults without preventing startup.

### Manage one active device explicitly

Introduce a small active-device state such as:

```text
NoController | MappedGameController | RawJoystickFallback
```

Each active handle owns its SDL resource and stable joystick instance ID. The
manager must follow SDL's two identifier meanings correctly:

- `SDL_CONTROLLERDEVICEADDED.which` is a temporary device index used to open a
  controller;
- removed and remapped events carry the stable instance ID of an opened device.
- The raw fallback follows the equivalent `SDL_JOYDEVICEADDED` device-index and
  `SDL_JOYDEVICEREMOVED` instance-ID distinction.

Lifecycle policy:

1. Enumerate all devices after SDL initialization and select the first mapped
   game controller.
2. Use raw joystick index `0` only if enumeration found no mapped controller.
3. Ignore additional devices while the active device remains connected.
4. If the active device is removed, close it once, emit neutral input, and
   rescan for a replacement.
5. If either a controller or raw-joystick add event arrives while none is
   active, rescan using the normal preference order.
6. On a mapping-change event, retain the matching active handle and refresh its
   diagnostic metadata unless SDL reports it detached.

This deliberately avoids activity-based switching and saved device preferences
in the first implementation. Those policies are harder to reason about and are
not needed for one-player support.

### Treat mappings as data, not brand-specific code

Start with the mappings built into the packaged SDL runtime. Xbox-compatible
devices and the SN30 Pro in X-input mode should enter through that standardized
path.

If an exact SN30 Pro connection mode is visible as a joystick but
`SDL_IsGameController` rejects it:

1. record platform, SDL version, connection type, controller firmware, name,
   GUID, axes, buttons, and current mapping result;
2. check the same device against the newer packaged SDL runtime before adding a
   workaround;
3. add the smallest verified mapping to a repository-owned mapping file;
4. load that file before controller enumeration and include it in Windows and
   Linux packages;
5. document the mapping source and license.

Do not branch on substrings such as `"8BitDo"` in gameplay code. Do not force
`SDL_JOYSTICK_HIDAPI_8BITDO` away from SDL's default unless testing demonstrates
a specific platform problem and the override fixes it without regressing Xbox
or Steam virtual controllers.

### Support Steam Input through gamepad emulation first

The first Steam Input target is gamepad emulation/legacy mode. Steam can expose
a configured physical device to a conventional game as an Xbox-style gamepad;
SDL then supplies the same logical controller contract as a direct Xbox device.

Do not add the native Steam Input API merely to claim Steam compatibility.
Native actions become valuable only when the project needs an official Steam
configuration, action sets, or device-specific glyphs.

Test Steam Input with SDL's normal defaults. Do not add speculative SDL hints.
Valve specifically recommends SDL 2.0.8 or newer so Steam can prevent duplicate
physical and emulated input; this project already requires SDL 2.0.18 or newer.

### Separate Windows and WSL acceptance

Steam Input acceptance belongs on the native Windows build launched through the
Windows Steam client. A Steam-created Windows virtual controller is not evidence
that the WSL Linux guest receives a controller.

WSL acceptance requires a controller visible to Linux. Microsoft documents that
USB devices are not forwarded to WSL natively and that `usbipd-win` is required.
While a USB device is attached to WSL, Windows cannot use that same device. Use
a wired controller for this path and verify Linux visibility and permissions
before launching the game.

Do not make WSL controller forwarding an application startup responsibility.
The application should report `no controller detected` clearly and continue to
support keyboard input.

## Implementation slices

### Slice 1: Freeze the logical input contract

- Add `gamepad.rs` with a plain `GamepadSnapshot` containing only named logical
  axes/buttons required by the game.
- Add pure conversion functions for menu input and gameplay `ControllerState`.
- Add a `GamepadLatch` that separates edges from held values.
- Add validated `SensitivityPercent` and `InputTuning` values plus pure
  mouse/controller threshold derivation.
- Cover default equivalence, minimum/default/maximum sensitivity, reachable
  extremes, threshold boundaries, hysteresis, D-pad priority, trigger priority,
  contradictory input, and held-confirm behavior with table-driven unit tests.
- Prove that `ControlMode`, `SKYROADS.CFG`, demo input, and simulation types do
  not change.

Exit criterion: all controller semantics can be tested without SDL or hardware.

### Slice 2: Add a safe SDL game-controller boundary

- Initialize `SDL_INIT_GAMECONTROLLER` in addition to the existing subsystems.
- Add only the SDL2 declarations needed for enumeration, mapped-controller
  checks, open/close, name, logical axes/buttons, attached state, underlying
  joystick instance ID, controller device events, and the joystick add/remove
  events required by the retained raw fallback.
- Add `#[repr(C)]` event layouts with compile-time size/alignment checks.
- Wrap `SDL_GameController*` in a single-owner RAII type borrowing `Sdl`, just as
  the current window/audio resources do.
- Keep the underlying joystick pointer borrowed from the game-controller handle;
  never close it separately.
- Translate nulls, negative IDs, and SDL errors at the boundary.
- Add focused safe-API tests for enum conversions, event decoding, and invalid
  indices. Review every new unsafe call's lifetime, ownership, layout, and
  integer-conversion obligations.

Exit criterion: mapped devices can be enumerated and sampled without exposing a
raw SDL pointer outside `sdl.rs`.

### Slice 3: Add discovery, hot-plug, and diagnostics

- Replace the startup-only `Joystick::open_first` value in `main.rs` with the
  explicit active-controller manager.
- Extend event polling with typed mapped-controller and raw-joystick
  add/remove/remap notifications; do not expose the raw `SDL_Event` union.
- Implement the selection and neutral-on-disconnect policy above.
- Print one concise startup/change line containing SDL device name, mapped/raw
  status, instance ID, and selected state.
- Add a controller diagnostics launch mode that lists detected devices and
  reports normalized logical inputs. It must be usable before loading game
  assets so WSL and mapping failures can be separated from asset failures.
- If no controller exists, keep running and warn only when `JOYSTICK` gameplay
  mode is selected.

Exit criterion: start-without-device, plug, unplug, and reconnect are observable
and do not crash or leave non-neutral input.

### Slice 4: Integrate controller navigation and gameplay

- Sample the active controller once per host frame after pumping SDL events.
- Merge gamepad menu edges into the keyboard `AppInput` path in every app mode.
- Apply gamepad movement only to `gameplay_controls_override` while
  `ControlMode::Joystick` is active.
- Preserve the existing keyboard and DOS-style mouse behavior.
- Add regressions proving a held gamepad button is consumed once per app edge,
  while held gameplay movement remains present across simulation catch-up ticks.
- Extend the automated smoke harness with an injected logical gamepad sequence
  so intro -> menu -> level selection -> gameplay can be tested without physical
  hardware. Keep the existing SDL dummy-driver smoke test as a separate host
  check.

Exit criterion: an injected mapped-controller sequence reaches gameplay and
produces the same deterministic `ControllerState` as equivalent keyboard input.

### Slice 5: Add live input tuning and persistence

- Extend the native settings model and renderer with the focused `INPUT` tuning
  page, two percentage values, activation indicators, and reset action.
- Apply mouse tuning in the host-side mouse-to-`ControllerState` conversion,
  using `controller_state_from_dos_mouse` as the exact `100%` reference. Apply
  controller tuning while producing the logical gamepad snapshot. Keep the core
  output digital and deterministic.
- Load `SKYROADS-RS-INPUT.CFG` at startup, apply changes immediately, and save
  only after a valid value changes.
- Add strict parse/encode/round-trip tests, missing-file defaults, malformed-file
  fallback, range validation, and restart persistence.
- Prove `100% / 100%` produces the exact current mouse and joystick threshold
  outputs at every boundary.
- Prove `50%` and `200%` remain reachable, monotonic, and free of integer
  overflow for SDL's full signed-stick and unsigned-trigger ranges.

Exit criterion: both sensitivities can be changed, previewed, reset, persisted,
and restored while the default remains behaviorally identical to the current
host.

### Slice 6: Validate exact devices and add mappings only when needed

Run the hardware matrix below. Capture diagnostics before changing mappings or
SDL hints. For each failure, classify it as:

- device not visible to the OS/WSL guest;
- visible raw joystick but no SDL controller mapping;
- mapping present but logical control incorrect;
- Steam physical/virtual duplication;
- application edge/held-state defect.

Only the last three classifications authorize application or mapping changes.
Repeat the complete matrix after any mapping or SDL hint change.

Exit criterion: every required matrix row has recorded pass evidence, or a row
is explicitly marked environment-blocked without weakening other rows.

### Slice 7: Document and package the supported paths

- Update `README.md` with the logical control map, hot-plug behavior, sensitivity
  range/default, and native input-preference file.
- Update the Windows package readme with direct Xbox, Steam Input emulation, and
  SN30 Pro X-input setup, plus sensitivity controls and persistence.
- Update the Linux package readme with WSL USB-forwarding and permission
  prerequisites, sensitivity controls, and the input-preference file, clearly
  separate from WSLg display/audio setup.
- Include a mapping file in both packages only if Slice 6 proves it is needed.
- Record tested OS, SDL runtime, connection mode, and controller firmware rather
  than making an unbounded `all controllers supported` claim.
- Add a changelog entry only after implementation and hardware acceptance are
  complete.

Exit criterion: packaged instructions reproduce every supported matrix row and
do not present proposed behavior as already implemented.

## Controller hardware acceptance matrix

| Priority | Host | Controller route | Connection/mode | Expected result | Status |
| --- | --- | --- | --- | --- | --- |
| Required | Windows 11 native build | Xbox controller direct | USB | mapped once; full menu/gameplay path; unplug/reconnect safe | Pending |
| Required | Windows 11 native build | Xbox controller direct | Bluetooth | same logical controls as USB | Pending |
| Required | Windows 11 build launched through Steam | Xbox controller via Steam Input | gamepad emulation enabled | one Xbox-style logical device; no double input | Pending |
| Required | Windows 11 native build | 8BitDo SN30 Pro direct | `X + Start` X-input mode, USB | mapped controller; correct stick, D-pad, triggers, face buttons | Pending |
| Required | Windows 11 native build | 8BitDo SN30 Pro direct | `X + Start` X-input mode, Bluetooth | same logical controls as USB | Partial - mapping, `JOYSTICK`, full menu/gameplay path including death retry, win return, and gameplay exit, held-menu edge, active-input neutralization/reconnect, and `50%`/`100%`/`200%` sensitivity persistence/reset observed; firmware and USB checks pending |
| Required | Windows 11 build launched through Steam | 8BitDo SN30 Pro | Switch mode through Steam Input | one emulated logical device; positional face-button behavior documented | Pending |
| Required for WSL claim | Ubuntu 24.04 WSL2 | Xbox controller direct to Linux | wired USB attached with `usbipd-win` | device visible to Linux and SDL; full menu/gameplay path | Environment-blocked in the current WSL session: no `/dev/input` |
| Required for WSL claim | Ubuntu 24.04 WSL2 | 8BitDo SN30 Pro direct to Linux | wired X-input mode attached with `usbipd-win` | mapped or explicitly supplied mapping; full path | Environment-blocked in the current WSL session: no `/dev/input` |
| Best effort | Windows/Linux | 8BitDo SN30 Pro direct | D-input or Switch mode without Steam | works when SDL has a verified mapping; no raw-number special case | Not run |

For every required row, verify:

- device name, SDL version, connection type, firmware, mapped/raw status, and
  active instance ID are recorded;
- intro skip, all menu directions, select, back, level selection, steering,
  acceleration, braking, jump, death retry, win return, and gameplay exit work;
- holding a menu control produces one edge, not repeated transitions;
- `50%`, `100%`, and `200%` controller sensitivity produce observable,
  monotonic changes while every stick direction and trigger remains reachable;
- `100%` matches the existing DOS-derived joystick threshold, and a saved
  non-default value survives restart and can be reset;
- reconnecting does not require restarting the game;
- unplugging during steering/throttle produces neutral input on the next tick;
- Steam Input enabled/disabled selects the intended logical device and never
  doubles an action;
- keyboard and mouse modes still behave as before.

## Mouse sensitivity acceptance matrix

| Priority | Host | Input route | Expected result |
| --- | --- | --- | --- |
| Required | Windows 11 native build | native SDL mouse | both axes tune live; defaults and persistence work |
| Required for WSL claim | Ubuntu 24.04 WSL2 | WSLg pointer input | same logical thresholds and tuning behavior as Windows |

For each mouse row, verify:

- horizontal steering and vertical acceleration/braking remain reachable at
  `50%`, change monotonically at `100%` and `200%`, and do not chatter at rest;
- `100%` exactly matches the existing X thresholds `0x0096 / 0x00AA` and Y
  thresholds `0x000F / 0x00B9`;
- the tuning page previews both axes, applies changes immediately, persists a
  non-default value across restart, and resets to `100%`;
- a missing, malformed, or out-of-range input-preference file selects the full
  `100% / 100%` default without preventing startup.

## Automated verification gates

Run after each focused slice where applicable:

```bash
cargo fmt --check
cargo test -p skyroads-sdl
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release -p skyroads-sdl
SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy \
  target/release/skyroads-sdl --smoke-gameplay .
```

Also build and run the native Windows release with its packaged SDL DLL. The
headless smoke test proves host/game flow but cannot replace the physical-device
matrix.

## Risks and guards

| Risk | Guard |
| --- | --- |
| SDL device index is confused with instance ID | Model add and remove/remap events as different typed variants and test decoding. |
| Controller removal leaves a dangling FFI handle | Own one handle with RAII, close exactly once, and neutralize before the next tick. |
| Steam sends physical and virtual input | Use only one active SDL controller, stay on SDL 2.0.18+, and test Steam enabled/disabled before adding hints. |
| SN30 Pro labels differ by mode | Describe controls by position/intent and consume SDL logical mappings rather than device names. |
| SDL 2.30 and packaged SDL 2.32 mappings differ | Record both runtimes and ship a verified mapping only when built-ins are insufficient. |
| WSL cannot see a Windows-paired controller | Require Linux device visibility first and document `usbipd-win`; test Steam Input on native Windows. |
| Expanded FFI introduces layout or lifetime bugs | Keep unsafe code in `sdl.rs`, add C-layout assertions, expose safe owned/borrowed wrappers, and review each call's proof. |
| Analog drift causes menu chatter | Use explicit engage/release hysteresis and table-driven boundary tests. |
| Low sensitivity makes an action unreachable | Clamp derived runtime thresholds inside reachable bounds and test full deflection at `50%`. |
| High sensitivity amplifies drift | Keep hysteresis, show live activation, and validate `200%` on every required controller route. |
| Malformed native sensitivity settings prevent startup | Parse strictly, warn once, and fall back atomically to `100% / 100%`. |
| New settings break DOS compatibility | Keep `ControlMode::Joystick == 1`; store input tuning only in `SKYROADS-RS-INPUT.CFG`. |

## Completion criteria

The work is complete only when:

- all required native Windows controller and mouse rows pass;
- both WSL controller rows and the WSL mouse row pass before WSL controller and
  input-tuning support are advertised;
- controller navigation works from intro through gameplay;
- hot-plug, removal, and reconnect are safe and observable;
- Xbox, Steam-emulated, and SN30 Pro inputs produce the same logical actions;
- mouse and controller sensitivity can be adjusted and previewed live across the
  full supported range;
- defaults preserve the existing thresholds, extremes remain usable, reset
  works, and native input preferences survive restart;
- keyboard, mouse, demo playback, deterministic simulation, and CFG round trips
  have no regressions;
- automated gates and Windows/Linux release builds pass;
- package documentation states exact tested boundaries.

## External contracts consulted

- SDL2 standardized controller opening and instance-ID rules:
  <https://wiki.libsdl.org/SDL2/SDL_GameControllerOpen>
- SDL2 controller device-event semantics:
  <https://wiki.libsdl.org/SDL2/SDL_ControllerDeviceEvent>
- SDL2 logical axis ranges, including trigger behavior:
  <https://wiki.libsdl.org/SDL2/SDL_GameControllerGetAxis>
- SDL2 mapped-controller detection and optional mapping data:
  <https://wiki.libsdl.org/SDL2/SDL_IsGameController> and
  <https://wiki.libsdl.org/SDL2/SDL_GameControllerAddMapping>
- SDL2 8BitDo HIDAPI hint behavior:
  <https://wiki.libsdl.org/SDL2/SDL_HINT_JOYSTICK_HIDAPI_8BITDO>
- Steam Input legacy/native distinction:
  <https://partner.steamgames.com/doc/features/steam_controller/concepts>
- Steam Input gamepad-emulation behavior and SDL duplicate-input guidance:
  <https://partner.steamgames.com/doc/features/steam_controller/steam_input_gamepad_emulation_bestpractices>
  and
  <https://partner.steamgames.com/doc/features/steam_controller/getting_started_for_devs>
- 8BitDo SN30 Pro modes and Windows/Steam guidance:
  <https://download.8bitdo.com/Manual/Controller/SN30pro%2BSF30pro/SN30-Pro-8.pdf>
  and <https://support.8bitdo.com/faq/sn30-pro.html>
- Microsoft WSL USB forwarding requirements:
  <https://learn.microsoft.com/en-us/windows/wsl/connect-usb>
