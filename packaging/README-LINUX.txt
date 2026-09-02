SkyRoads Rust 0.1.0 - Linux x64 (Ubuntu 24.04 / WSL2)
=====================================================

This build targets 64-bit Ubuntu 24.04 and requires SDL 2.0.18 or newer.
Install the SDL2 runtime once:

    sudo apt install libsdl2-2.0-0

Run the game from the unpacked archive directory:

    ./skyroads-sdl .

WSLg must be available for the visible game window and audio. WSLg
display/audio integration is separate from controller forwarding; a controller
paired with Windows is not automatically visible inside Linux.

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

WSL wired-controller setup
--------------------------
Microsoft documents that WSL requires usbipd-win for USB forwarding. Use a
wired controller; Windows cannot use that device while it is attached to WSL.

1. Keep the WSL distribution running.
2. In an elevated Windows terminal, list devices and bind the controller once:

       usbipd list
       usbipd bind --busid <BUSID>

3. Attach it to WSL:

       usbipd attach --wsl --busid <BUSID>

4. In WSL, verify USB and input-device visibility before launching the game:

       lsusb
       ls -l /dev/input

The current Linux user must be able to read the controller event/joystick
device. USB forwarding and Linux permissions are host setup responsibilities;
the game continues with keyboard input when no controller is visible.

For an 8BitDo SN30 Pro, use Windows/X-input mode (X + Start) before attaching
the wired device. A direct raw, D-input, or Switch-mode device is best effort
unless SDL provides a verified mapping. Steam Input acceptance belongs to the
native Windows build and is not evidence that WSL received a controller. No
repository-owned controller mapping is currently shipped.

Diagnostics
-----------
Run this before reporting a device problem:

    ./skyroads-sdl --controller-diagnostics

The command loads no game assets. It lists the SDL version and each device's
mapped/raw status, GUID, mapping, axis/button counts, and probe instance ID. It
then reports the selected device and prints normalized input changes. Stop it
with Ctrl+C. Record the Ubuntu/WSL version, SDL version, USB route, controller
firmware, mapped/raw result, and observed logical controls.

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

WSLg display and audio
----------------------
The game starts in borderless fullscreen at the current desktop resolution.
Use Settings to select WINDOWED, BORDERLESS, or EXCLUSIVE and to choose an
available resolution and refresh rate. WSLg commonly exposes fewer modes and a
lower refresh rate than native Windows. Display preferences are stored in
SKYROADS-RS-DISPLAY.CFG.

Music uses the original MUZAX data through a YM3812/OPL2 emulator. Road music
randomly selects all twelve road tracks and does not immediately repeat a track.

Headless verification
---------------------
These commands verify the host/game flow without a visible window or audio
device. The gamepad command injects logical input and does not prove that Linux
can see physical hardware:

    SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy \
        ./skyroads-sdl --smoke-gameplay .
    SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy \
        ./skyroads-sdl --smoke-gamepad .

Compatibility and validation boundary
-------------------------------------
This is a dynamically linked Ubuntu 24.04 x86-64 build. It requires glibc and
libSDL2-2.0.so.0 from the host distribution; compatibility with older Linux
distributions is not claimed.

As of 2026-09-02, the current WSL2 environment exposes no /dev/input directory.
Neither wired Xbox nor wired SN30 Pro controller acceptance has been recorded,
and WSLg mouse tuning remains physically unverified. The Linux code has
automated normalization, lifecycle, persistence, release-build, and injected
gamepad-flow coverage; those checks do not replace the WSL hardware matrix.
