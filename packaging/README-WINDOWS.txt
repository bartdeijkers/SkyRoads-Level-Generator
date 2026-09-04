SkyRoads Rust - Windows x64
==========================

Original game data is not included. Follow GAME-DATA.txt to obtain and install
your own copy before launching. Keep SDL2.dll beside skyroads-sdl.exe.
From the unpacked directory, run:

    skyroads-sdl.exe "C:\Games\SkyRoads-data"

If you put the data beside the executable, you can double-click skyroads-sdl.exe.
BUILD-INFO.json records this package's version, target, and source commit.

Keyboard controls
-----------------
Arrow keys: menu navigation, steering, throttle, and brake
Enter: select or restart
Space: skip intro, jump, or restart
Escape: return to the menu
Q: quit; alternatively select QUIT from the main menu
Shift+Enter: toggle windowed mode and the most recently used fullscreen mode

Controller controls
-------------------
Gamepad navigation works in every control mode. Select JOYSTICK in Controls
before using gamepad movement during gameplay.

D-pad or left stick: navigate menus
South face button or Start: confirm/select
East face button or Back/View: go back
LB / RB in Controls: previous / next DISPLAY or VIDEO MODE value
Select QUIT on the main menu, then confirm: quit
D-pad left/right or left-stick X: steer
D-pad/stick up or right trigger: accelerate
D-pad/stick down or left trigger: brake
South face button: jump, retry after a crash, or select after a win

Controls are described by position because Xbox, Nintendo-style, and 8BitDo
button labels differ. The south button is the lower face button; the east
button is the right face button.

The DOS-exact terminal paths do not draw separate retry or win prompts. After an
explosion or fall, release the south button and press it again to retry. After
the final tunnel scene settles, release the button and press it again to return
to level selection.

The game selects one SDL input device. It prefers the first mapped controller
and retains raw joystick index 0 only as a limited fallback. Additional devices
are ignored until the selected device disconnects. Unplugging neutralizes input
before the next simulation tick and triggers a rescan; reconnecting does not
require restarting the game.

Windows controller setup
------------------------
Intended direct Xbox route: connect an Xbox-compatible controller by USB or
Bluetooth.

Intended direct 8BitDo SN30 Pro route: start it in Windows/X-input mode with
X + Start, then connect by USB or Bluetooth.

Intended Steam Input route: launch this native Windows build through the
Windows Steam client with gamepad emulation/legacy mode enabled. The expected
result is one Xbox-style logical device, without duplicate actions. For an
SN30 Pro in Switch mode, Steam Input emulation is the intended route; direct
Switch or D-input mode is best effort and depends on SDL having a verified
mapping.

The native Steam Input API, action manifests, Steam-specific glyphs, and
device-name special cases are not included. No repository-owned controller
mapping is currently shipped.

Diagnostics
-----------
Run this from a terminal before reporting a device problem:

    skyroads-sdl.exe --controller-diagnostics

The command loads no game assets. It lists the SDL version and each device's
mapped/raw status, GUID, mapping, axis/button counts, and probe instance ID. It
then reports the selected device and prints normalized input changes. Stop it
with Ctrl+C. Record the Windows version, packaged SDL version, USB/Bluetooth or
Steam route, controller firmware, mapped/raw result, and observed logical
controls.

Input sensitivity
-----------------
Open Controls -> INPUT to adjust MOUSE SENSITIVITY and CONTROLLER SENSITIVITY.
Both values range from 50% through 200% in 5% steps. Higher values engage with
less travel; lower values require more travel. The 100% default preserves the
DOS-derived thresholds, and RESET TO 100% restores both defaults. The page
applies changes immediately and previews mouse axes, controller stick axes, and
triggers.

Changed values are stored beside the game data in SKYROADS-RS-INPUT.CFG. A
missing file uses 100% / 100%. Invalid content emits one warning and falls back
to both defaults. This file does not change the DOS-compatible SKYROADS.CFG.

Display and audio
-----------------
The game starts in borderless fullscreen at the current desktop resolution.
Use Settings to select WINDOWED, BORDERLESS, or EXCLUSIVE and to choose an
exact resolution and refresh rate. Display preferences are stored in
SKYROADS-RS-DISPLAY.CFG.

Music uses the original MUZAX data through a YM3812/OPL2 emulator. Road music
randomly selects all twelve road tracks and does not immediately repeat a track.

Controller validation boundary
------------------------------
As of 2026-09-02, a Windows x86-64 release with SDL 2.32.10 passes native
startup diagnostics and both injected dummy-driver smoke paths on Windows build
10.0.26200.9278. Automated normalization, lifecycle, and persistence coverage
also passes. With Steam stopped, an 8BitDo SN30 Pro connected over Bluetooth was
mapped once as an Xbox-style SDL controller; an isolated run persisted JOYSTICK,
menu navigation, confirm/back, steering, both triggers, jump, and held-menu edge
behavior worked, and mapped reselection was observed after the device temporarily
disappeared. Controller sensitivity at 50%, 100%, and 200%, restart restoration,
and reset were also checked live and against the isolated preference file.
Powering off while steering and accelerating released both inputs, and
reconnecting restored control without restarting the game. A fresh South press
after a crash restarted the road, and East/Back exited gameplay to level
selection. Completing Road 1 and pressing South after the final tunnel also
returned to level selection. Firmware and USB comparison remain unverified, so
this is partial evidence and no native Windows acceptance row is complete. The
routes above remain setup instructions and implementation targets, not a claim
that every controller or connection mode has been hardware-certified.
