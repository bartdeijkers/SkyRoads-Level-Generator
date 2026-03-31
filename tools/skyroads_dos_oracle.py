#!/usr/bin/env python3

from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

MACOS_DEFAULT_DOSBOX = Path("/Applications/dosbox-x.app/Contents/MacOS/dosbox-x")
WSL_POWERSHELL = Path("/mnt/c/WINDOWS/System32/WindowsPowerShell/v1.0/powershell.exe")
PROMPT_MARKER = "I-> _"
F5_ESCAPE = "\x1b[15~"
REGISTER_COMMAND = "EV CS IP DS ES SS AX BX CX DX SI DI BP SP"
REGISTER_NAMES = ["cs", "ip", "ds", "es", "ss", "ax", "bx", "cx", "dx", "si", "di", "bp", "sp"]
MEMDUMP_BIN_NAMES = ("memdump.bin", "MEMDUMP.BIN")
MACOS_SCREENSHOT_TOOL = "screencapture"
MACOS_KEY_CODES = {
    "space": 49,
    "return": 36,
    "enter": 36,
    "escape": 53,
    "up": 126,
    "down": 125,
    "left": 123,
    "right": 124,
}
POWERSHELL_SEND_KEYS = {
    "space": "{SPACE}",
    "return": "~",
    "enter": "~",
    "escape": "{ESC}",
    "up": "{UP}",
    "down": "{DOWN}",
    "left": "{LEFT}",
    "right": "{RIGHT}",
}
ADDKEY_KEYWORDS = {
    "space": "space",
    "return": "enter",
    "enter": "enter",
    "escape": "esc",
    "up": "up",
    "down": "down",
    "left": "left",
    "right": "right",
}
BIOS_KEYWORDS = {
    "space": (0x20, 0x39),
    "return": (0x0D, 0x1C),
    "enter": (0x0D, 0x1C),
    "escape": (0x1B, 0x01),
    "up": (0x00, 0x48),
    "down": (0x00, 0x50),
    "left": (0x00, 0x4B),
    "right": (0x00, 0x4D),
}
BIOS_KEYBOARD_SEGMENT = "0040"
BIOS_KEYBOARD_HEAD_OFFSET = 0x001A
BIOS_KEYBOARD_TAIL_OFFSET = 0x001C
BIOS_KEYBOARD_BUFFER_OFFSET = 0x001E
BIOS_KEYBOARD_BUFFER_CAPACITY = 16
ROAD_WINDOW_BASE_OFFSET = 0x1638
ROAD_WINDOW_ROW_STRIDE = 0x0E
ROAD_WINDOW_ACTIVE_OFFSET = 0x62
TREKDAT_POINTER_GRID_BYTES = 0x0270
VGA_FRAME_DUMP_NAME = "vga_frame"
VGA_FRAME_SEGMENT = "A000"
VGA_FRAME_OFFSET = "0000"
VGA_FRAME_BYTES = 320 * 200
FIXTURE_BUNDLE_VERSION = 1
DEFAULT_FIXTURE_ROOT = Path("fixtures/dos-gameplay-renderer")
FILE_OPEN_RE = re.compile(r"LOG:\s+(\d+)\s+FILES:file open command (\d+) file (.+)")
FILE_READ_RE = re.compile(r"LOG:\s+(\d+)\s+(?:DEBUG )?FILES:Reading (\d+) bytes from (.+)")
FILE_CLOSE_RE = re.compile(r"LOG:\s+(\d+)\s+FILES:Closing file (.+)")
TRACE_DEVICE_NAMES = frozenset({"CON", "PRN", "NUL", "AUX"})


def running_in_wsl() -> bool:
    release = platform.release().lower()
    return (
        "WSL_DISTRO_NAME" in os.environ
        or "WSL_INTEROP" in os.environ
        or "microsoft" in release
    )


def default_dosbox_path() -> Path:
    path_from_env = os.environ.get("DOSBOX_X")
    if path_from_env:
        return Path(path_from_env)
    path_from_path = shutil.which("dosbox-x")
    if path_from_path:
        return Path(path_from_path)
    return MACOS_DEFAULT_DOSBOX


def detect_key_backend() -> str | None:
    if shutil.which("osascript") is not None:
        return "macos"
    if running_in_wsl() and WSL_POWERSHELL.exists():
        return "powershell"
    return None


def detect_screenshot_backend() -> str | None:
    if shutil.which(MACOS_SCREENSHOT_TOOL) is not None:
        return "macos"
    return None


@dataclass(frozen=True)
class BreakpointSpec:
    name: str
    address: str | None = None
    image_offset: int | None = None

    def resolve(self, code_segment: int) -> "BreakpointSpec":
        if self.address is not None:
            return self
        if self.image_offset is None:
            raise ValueError(f"breakpoint {self.name!r} has neither an address nor an image offset")
        return BreakpointSpec(
            name=self.name,
            address=f"{code_segment:04X}:{self.image_offset:04X}",
            image_offset=self.image_offset,
        )


@dataclass(frozen=True)
class DumpSpec:
    name: str
    segment: str
    offset: str
    length: int

    def debugger_address(self) -> str:
        return f"{self.segment}:{self.offset}"


@dataclass(frozen=True)
class KeyEvent:
    delay_seconds: float
    key_name: str


@dataclass(frozen=True)
class AddKeySequence:
    events: tuple[KeyEvent, ...]

    def shell_commands(self) -> tuple[str, ...]:
        commands: list[str] = []
        elapsed_milliseconds = 0
        for event in self.events:
            button_name = addkey_button_name(event.key_name)
            elapsed_milliseconds += max(0, round(event.delay_seconds * 1000))
            commands.append(f"ADDKEY p{elapsed_milliseconds} {button_name}")
        return tuple(commands)


@dataclass(frozen=True)
class StageSpec:
    name: str
    resume_command: str
    repeat_count: int = 1
    bios_keys: tuple[str, ...] = ()
    capture_screen: bool = False
    timeout_seconds: float | None = None


@dataclass(frozen=True)
class CheckpointSpec:
    name: str | None = None
    resume_command: str = "f5"
    repeat_count: int = 1
    bios_keys: tuple[str, ...] = ()
    frame_bios_keys: tuple[tuple[str, ...], ...] = ()
    capture_screen: bool = False
    timeout_seconds: float | None = None


@dataclass(frozen=True)
class OraclePreset:
    name: str
    description: str
    breakpoints: tuple[BreakpointSpec, ...]
    dumps: tuple[DumpSpec, ...]
    bios_keys: tuple[str, ...] = ()
    guest_launch_sequence: AddKeySequence | None = None
    guest_launch_uses_bios_keys: bool = False
    guest_launch_uses_stages: bool = False
    auto_keys: tuple[KeyEvent, ...] = ()
    warmup_vrt_count: int = 0
    stages: tuple[StageSpec, ...] = ()
    checkpoints: tuple[CheckpointSpec, ...] = ()


ROAD0_MENU_LAUNCH_AUTO_KEYS = (
    KeyEvent(6.0, "space"),
    KeyEvent(1.0, "space"),
    KeyEvent(1.0, "space"),
    KeyEvent(1.0, "return"),
    KeyEvent(0.75, "return"),
)
ROAD0_LAUNCH_AUTO_KEYS = ROAD0_MENU_LAUNCH_AUTO_KEYS + (
    KeyEvent(6.0, "return"),
)
ROAD0_MENU_LAUNCH_ADDKEY = AddKeySequence(ROAD0_MENU_LAUNCH_AUTO_KEYS)
ROAD0_LAUNCH_ADDKEY = AddKeySequence(ROAD0_LAUNCH_AUTO_KEYS)


ROAD0_INITIAL_FRAME = OraclePreset(
    name="road0-initial-frame",
    description=(
        "Skip the intro, start Road 0, and capture the first gameplay-side hit of the "
        "main DOS road renderer at image offset 0x2D03."
    ),
    breakpoints=(BreakpointSpec("renderer_entry", image_offset=0x2D03),),
    dumps=(
        DumpSpec("renderer_state", "DS", "0E36", 0x20),
        DumpSpec("tile_class_by_low3", "DS", "0B77", 0x08),
        DumpSpec("draw_dispatch_by_type", "SS", "0B7F", 0x20),
        DumpSpec("trekdat_segment_table", "SS", "0E82", 0x10),
    ),
    bios_keys=(
        "space",
    ),
    guest_launch_sequence=ROAD0_LAUNCH_ADDKEY,
    guest_launch_uses_bios_keys=False,
    guest_launch_uses_stages=False,
    warmup_vrt_count=1,
    stages=(
        StageSpec("after_first_space", "vrt", repeat_count=4, capture_screen=True),
        StageSpec("queue_second_space", "vrt", bios_keys=("space",), repeat_count=4, capture_screen=True),
        StageSpec("wait_for_menu", "vrt", repeat_count=40, capture_screen=False),
        StageSpec("queue_start_press", "vrt", bios_keys=("return",), repeat_count=6, capture_screen=True),
        StageSpec("queue_confirm_press", "f5", bios_keys=("return",), capture_screen=False, timeout_seconds=60.0),
    ),
    auto_keys=ROAD0_LAUNCH_AUTO_KEYS,
)


def repeat_frame_bios_keys(
    keys: tuple[str, ...],
    count: int,
) -> tuple[tuple[str, ...], ...]:
    return tuple(keys for _ in range(count))


def concat_frame_bios_keys(
    *chunks: tuple[tuple[str, ...], ...],
) -> tuple[tuple[str, ...], ...]:
    combined: list[tuple[str, ...]] = []
    for chunk in chunks:
        combined.extend(chunk)
    return tuple(combined)


def make_named_vrt_checkpoints(
    prefix: str,
    count: int,
    *,
    first_bios_keys: tuple[str, ...] = (),
) -> tuple[CheckpointSpec, ...]:
    checkpoints: list[CheckpointSpec] = []
    for index in range(count):
        checkpoints.append(
            CheckpointSpec(
                name=f"{prefix}-{index + 1:02d}",
                resume_command="vrt",
                bios_keys=first_bios_keys if index == 0 else (),
            )
        )
    return tuple(checkpoints)


def road0_gameplay_scenario_preset(
    name: str,
    description: str,
    checkpoint_name: str,
    frame_bios_keys: tuple[tuple[str, ...], ...],
) -> OraclePreset:
    return OraclePreset(
        name=name,
        description=description,
        breakpoints=ROAD0_INITIAL_FRAME.breakpoints,
        dumps=ROAD0_INITIAL_FRAME.dumps,
        bios_keys=ROAD0_INITIAL_FRAME.bios_keys,
        guest_launch_sequence=ROAD0_INITIAL_FRAME.guest_launch_sequence,
        auto_keys=ROAD0_INITIAL_FRAME.auto_keys,
        warmup_vrt_count=ROAD0_INITIAL_FRAME.warmup_vrt_count,
        stages=ROAD0_INITIAL_FRAME.stages,
        checkpoints=(
            CheckpointSpec(
                name=checkpoint_name,
                resume_command="f5",
                frame_bios_keys=frame_bios_keys,
            ),
        ),
    )


def gomenu_vrt_scan_preset(
    name: str,
    description: str,
    checkpoint_name: str,
    *,
    checkpoint_bios_keys: tuple[str, ...] = (),
) -> OraclePreset:
    return OraclePreset(
        name=name,
        description=description,
        breakpoints=(),
        dumps=(),
        bios_keys=ROAD0_INITIAL_FRAME.bios_keys,
        guest_launch_sequence=ROAD0_MENU_LAUNCH_ADDKEY,
        guest_launch_uses_bios_keys=True,
        guest_launch_uses_stages=True,
        auto_keys=ROAD0_MENU_LAUNCH_AUTO_KEYS,
        warmup_vrt_count=ROAD0_INITIAL_FRAME.warmup_vrt_count,
        stages=ROAD0_INITIAL_FRAME.stages[:-1],
        checkpoints=(
            CheckpointSpec(
                name=checkpoint_name,
                resume_command="vrt",
                bios_keys=checkpoint_bios_keys,
            ),
        ),
    )


ROAD0_STEADY_NEUTRAL = road0_gameplay_scenario_preset(
    name="road0-steady-neutral",
    description=(
        "Start Road 0, advance eight gameplay renderer hits with no gameplay input, "
        "and capture the steady neutral checkpoint."
    ),
    checkpoint_name="steady-neutral",
    frame_bios_keys=repeat_frame_bios_keys((), 8),
)


ROAD0_SUSTAINED_THROTTLE = road0_gameplay_scenario_preset(
    name="road0-sustained-throttle",
    description=(
        "Start Road 0, inject throttle for twenty-four gameplay frames, and capture "
        "the sustained-throttle checkpoint."
    ),
    checkpoint_name="sustained-throttle",
    frame_bios_keys=repeat_frame_bios_keys(("up",), 24),
)


ROAD0_STEADY_LEFT = road0_gameplay_scenario_preset(
    name="road0-steady-left",
    description=(
        "Start Road 0, inject throttle plus left steering for twenty-four gameplay "
        "frames, and capture the steady-left checkpoint."
    ),
    checkpoint_name="steady-left",
    frame_bios_keys=repeat_frame_bios_keys(("up", "left"), 24),
)


ROAD0_STEADY_RIGHT = road0_gameplay_scenario_preset(
    name="road0-steady-right",
    description=(
        "Start Road 0, inject throttle plus right steering for twenty-four gameplay "
        "frames, and capture the steady-right checkpoint."
    ),
    checkpoint_name="steady-right",
    frame_bios_keys=repeat_frame_bios_keys(("up", "right"), 24),
)


ROAD0_FIRST_AIRBORNE = road0_gameplay_scenario_preset(
    name="road0-first-airborne",
    description=(
        "Start Road 0, inject eight throttle frames and then the first throttle-plus-jump "
        "frame, and capture the first airborne checkpoint."
    ),
    checkpoint_name="first-airborne",
    frame_bios_keys=concat_frame_bios_keys(
        repeat_frame_bios_keys(("up",), 8),
        repeat_frame_bios_keys(("up", "space"), 1),
    ),
)

GOMENU_DEFAULT_SELECTION = gomenu_vrt_scan_preset(
    name="gomenu-default-selection",
    description=(
        "Skip the intro, stop in GoMenu after the Start press, and capture the default "
        "level-selection state before confirming a road."
    ),
    checkpoint_name="default-selection",
)

GOMENU_RIGHT_SELECTION = gomenu_vrt_scan_preset(
    name="gomenu-right-selection",
    description=(
        "Skip the intro, stop in GoMenu, inject a Right-arrow BIOS key, and capture the "
        "level-selection state one VRT later."
    ),
    checkpoint_name="right-selection",
    checkpoint_bios_keys=("right",),
)

GOMENU_DOWN_SELECTION = gomenu_vrt_scan_preset(
    name="gomenu-down-selection",
    description=(
        "Skip the intro, stop in GoMenu, inject a Down-arrow BIOS key, and capture the "
        "level-selection state one VRT later."
    ),
    checkpoint_name="down-selection",
    checkpoint_bios_keys=("down",),
)

PRESETS = {
    ROAD0_INITIAL_FRAME.name: ROAD0_INITIAL_FRAME,
    "road0-direct-preload": OraclePreset(
        name="road0-direct-preload",
        description=(
            "Queue the full intro/menu input sequence up front, then run directly into the "
            "Road 0 gameplay renderer."
        ),
        breakpoints=(BreakpointSpec("renderer_entry", image_offset=0x2D03),),
        dumps=ROAD0_INITIAL_FRAME.dumps,
        bios_keys=("space", "space", "return", "return"),
        warmup_vrt_count=1,
    ),
    "road0-first-five-renderer-frames": OraclePreset(
        name="road0-first-five-renderer-frames",
        description=(
            "Skip the intro, start Road 0, and capture the first five gameplay-side hits of "
            "the main DOS road renderer at image offset 0x2D03."
        ),
        breakpoints=ROAD0_INITIAL_FRAME.breakpoints,
        dumps=ROAD0_INITIAL_FRAME.dumps,
        bios_keys=ROAD0_INITIAL_FRAME.bios_keys,
        auto_keys=ROAD0_INITIAL_FRAME.auto_keys,
        warmup_vrt_count=ROAD0_INITIAL_FRAME.warmup_vrt_count,
        stages=ROAD0_INITIAL_FRAME.stages,
        checkpoints=tuple(
            CheckpointSpec(name=f"frame_{index:02d}", resume_command="f5")
            for index in range(5)
        ),
    ),
    "road0-post-confirm-vrt-scan": OraclePreset(
        name="road0-post-confirm-vrt-scan",
        description=(
            "Skip the intro, queue the final Road 0 confirm press, and capture a VRT-by-VRT "
            "register scan immediately afterwards to diagnose the launch state before gameplay."
        ),
        breakpoints=(),
        dumps=(),
        bios_keys=ROAD0_INITIAL_FRAME.bios_keys,
        guest_launch_sequence=ROAD0_MENU_LAUNCH_ADDKEY,
        guest_launch_uses_bios_keys=True,
        guest_launch_uses_stages=True,
        auto_keys=ROAD0_MENU_LAUNCH_AUTO_KEYS,
        warmup_vrt_count=ROAD0_INITIAL_FRAME.warmup_vrt_count,
        stages=ROAD0_INITIAL_FRAME.stages[:-1],
        checkpoints=make_named_vrt_checkpoints(
            "post-confirm-vrt",
            16,
            first_bios_keys=("return",),
        ),
    ),
    GOMENU_DEFAULT_SELECTION.name: GOMENU_DEFAULT_SELECTION,
    GOMENU_RIGHT_SELECTION.name: GOMENU_RIGHT_SELECTION,
    GOMENU_DOWN_SELECTION.name: GOMENU_DOWN_SELECTION,
    ROAD0_STEADY_NEUTRAL.name: ROAD0_STEADY_NEUTRAL,
    ROAD0_SUSTAINED_THROTTLE.name: ROAD0_SUSTAINED_THROTTLE,
    ROAD0_STEADY_LEFT.name: ROAD0_STEADY_LEFT,
    ROAD0_STEADY_RIGHT.name: ROAD0_STEADY_RIGHT,
    ROAD0_FIRST_AIRBORNE.name: ROAD0_FIRST_AIRBORNE,
}


class DosboxDebuggerSession:
    def __init__(
        self,
        source_root: Path,
        output_root: Path,
        dosbox: Path,
        time_limit: int,
        cycles: str,
        pre_launch_commands: tuple[str, ...] = (),
    ) -> None:
        self.source_root = source_root
        self.output_root = output_root
        self.dosbox = dosbox
        self.time_limit = time_limit
        self.cycles = cycles
        self.pre_launch_commands = pre_launch_commands
        self.raw_log_path = output_root / "oracle.log"
        self.process: subprocess.Popen[bytes] | None = None
        self.master_fd: int | None = None
        self.log_bytes = bytearray()
        self.log_handle = None

    def start(self) -> None:
        command = build_dosbox_command(
            self.dosbox,
            self.source_root,
            self.time_limit,
            self.cycles,
            self.pre_launch_commands,
        )
        master_fd, slave_fd = os.openpty()
        flags = fcntl.fcntl(master_fd, fcntl.F_GETFL)
        fcntl.fcntl(master_fd, fcntl.F_SETFL, flags | os.O_NONBLOCK)
        self.log_handle = self.raw_log_path.open("wb")
        self.process = subprocess.Popen(
            command,
            cwd=self.output_root,
            stdin=slave_fd,
            stdout=slave_fd,
            stderr=slave_fd,
            close_fds=True,
        )
        os.close(slave_fd)
        self.master_fd = master_fd

    def stop(self) -> None:
        self._pump_output()
        if self.process is None or self.process.poll() is not None:
            self._close_handles()
            return
        self.process.terminate()
        try:
            self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=3)
        self._pump_output()
        self._close_handles()

    def read_log(self) -> str:
        self._pump_output()
        return self.log_bytes.decode("utf-8", errors="replace")

    def prompt_count(self) -> int:
        return self.read_log().count(PROMPT_MARKER)

    def require_running(self, action: str) -> None:
        self._pump_output()
        if self.process is None:
            raise RuntimeError(f"DOSBox-X debugger session is not running while trying to {action}")
        exit_code = self.process.poll()
        if exit_code is not None:
            raise RuntimeError(
                f"DOSBox-X exited with status {exit_code} before it could {action}"
            )

    def send_line(self, line: str) -> None:
        self.require_running(f"send debugger command `{line}`")
        if self.master_fd is None:
            raise RuntimeError("DOSBox-X debugger session is not running")
        os.write(self.master_fd, (line + "\n").encode("ascii"))

    def resume(self) -> None:
        self.require_running("resume execution")
        if self.master_fd is None:
            raise RuntimeError("DOSBox-X debugger session is not running")
        os.write(self.master_fd, F5_ESCAPE.encode("ascii"))

    def wait_for_prompt(self, previous_count: int, timeout_seconds: float) -> tuple[str, int]:
        deadline = time.monotonic() + timeout_seconds
        while time.monotonic() < deadline:
            text = self.read_log()
            count = text.count(PROMPT_MARKER)
            if count > previous_count:
                return text, count
            if self.process is not None and self.process.poll() is not None:
                raise RuntimeError(
                    f"DOSBox-X exited with status {self.process.returncode} before the debugger prompt returned"
                )
            time.sleep(0.1)
        raise TimeoutError(
            f"timed out after {timeout_seconds:.1f}s waiting for the DOSBox-X debugger prompt"
        )

    def wait_for_substring(self, substring: str, previous_length: int, timeout_seconds: float) -> str:
        deadline = time.monotonic() + timeout_seconds
        while time.monotonic() < deadline:
            text = self.read_log()
            if substring in text[previous_length:]:
                return text
            if self.process is not None and self.process.poll() is not None:
                raise RuntimeError(
                    f"DOSBox-X exited with status {self.process.returncode} while waiting for log text {substring!r}"
                )
            time.sleep(0.05)
        raise TimeoutError(f"timed out waiting for log text {substring!r}")

    def _pump_output(self) -> None:
        if self.master_fd is None:
            return
        while True:
            try:
                chunk = os.read(self.master_fd, 65536)
            except BlockingIOError:
                break
            except OSError:
                break
            if not chunk:
                break
            self.log_bytes.extend(chunk)
            if self.log_handle is not None:
                self.log_handle.write(chunk)
                self.log_handle.flush()

    def _close_handles(self) -> None:
        if self.master_fd is not None:
            try:
                os.close(self.master_fd)
            except OSError:
                pass
            self.master_fd = None
        if self.log_handle is not None:
            self.log_handle.close()
            self.log_handle = None


class HostKeySequence(threading.Thread):
    def __init__(
        self,
        backend: str,
        dosbox_pid: int | None,
        events: tuple[KeyEvent, ...],
    ) -> None:
        super().__init__(daemon=True)
        self.backend = backend
        self.dosbox_pid = dosbox_pid
        self.events = events
        self.stop_event = threading.Event()
        self.errors: list[str] = []

    def stop(self) -> None:
        self.stop_event.set()

    def run(self) -> None:
        for event in self.events:
            if self.stop_event.wait(event.delay_seconds):
                return
            try:
                send_host_key(self.backend, self.dosbox_pid, event.key_name)
            except Exception as exc:  # pragma: no cover - manual host integration
                self.errors.append(str(exc))
                return


def parse_breakpoint(value: str) -> BreakpointSpec:
    name, address = value.split("=", 1)
    address = address.strip()
    if address.startswith("@"):
        return BreakpointSpec(name=name.strip(), image_offset=int(address[1:], 0))
    segment, offset = address.split(":", 1)
    return BreakpointSpec(name=name.strip(), address=f"{segment.strip().upper()}:{offset.strip().upper()}")


def parse_dump(value: str) -> DumpSpec:
    name, address, raw_length = value.split("=", 1)[0], value.split("=", 1)[1], None
    segment, offset, raw_length = address.split(":", 2)
    return DumpSpec(
        name=name.strip(),
        segment=segment.strip().upper(),
        offset=offset.strip().upper(),
        length=int(raw_length, 0),
    )


def addkey_button_name(key_name: str) -> str:
    try:
        return ADDKEY_KEYWORDS[key_name]
    except KeyError as exc:
        raise ValueError(f"unsupported ADDKEY key name {key_name!r}") from exc


def build_dosbox_command(
    dosbox: Path,
    source_root: Path,
    time_limit: int,
    cycles: str,
    pre_launch_commands: tuple[str, ...] = (),
) -> list[str]:
    command = [
        str(dosbox),
        "-defaultconf",
        "-fastlaunch",
        "-debug",
        "-break-start",
        "-time-limit",
        str(time_limit),
    ]
    shell_commands = [
        f"cycles {cycles}",
        f"mount c {source_root} -nocachedir",
        "c:",
        *pre_launch_commands,
        "skyroads.exe",
    ]
    for shell_command in shell_commands:
        command.extend(["-c", shell_command])
    return command


@dataclass(frozen=True)
class LaunchPlan:
    guest_launch_sequence: AddKeySequence | None
    launch_input_backend: str
    bios_keys: tuple[str, ...]
    auto_keys: tuple[KeyEvent, ...]
    stages: tuple[StageSpec, ...]
    pre_launch_commands: tuple[str, ...]


def select_stage_flow(
    stages: tuple[StageSpec, ...],
    *,
    no_stages: bool,
    no_bios_keys: bool,
) -> tuple[StageSpec, ...]:
    if no_stages:
        return tuple()
    if no_bios_keys:
        return tuple(stage for stage in stages if not stage.bios_keys)
    return stages


def build_launch_plan(
    preset: OraclePreset,
    *,
    key_backend: str | None,
    no_auto_keys: bool,
    no_bios_keys: bool,
    no_stages: bool,
) -> LaunchPlan:
    guest_launch_sequence = None if no_auto_keys else preset.guest_launch_sequence
    if guest_launch_sequence is not None:
        guest_bios_keys = preset.bios_keys if preset.guest_launch_uses_bios_keys else tuple()
        guest_stages = preset.stages if preset.guest_launch_uses_stages else tuple()
        return LaunchPlan(
            guest_launch_sequence=guest_launch_sequence,
            launch_input_backend="guest-addkey",
            bios_keys=tuple() if no_bios_keys else guest_bios_keys,
            auto_keys=tuple(),
            stages=select_stage_flow(
                guest_stages,
                no_stages=no_stages,
                no_bios_keys=no_bios_keys,
            ),
            pre_launch_commands=guest_launch_sequence.shell_commands(),
        )

    auto_keys = tuple() if no_auto_keys or key_backend is None else preset.auto_keys
    if auto_keys:
        launch_input_backend = f"host-{key_backend}"
    elif no_auto_keys:
        launch_input_backend = "disabled"
    else:
        launch_input_backend = "manual"

    return LaunchPlan(
        guest_launch_sequence=None,
        launch_input_backend=launch_input_backend,
        bios_keys=tuple() if no_bios_keys else preset.bios_keys,
        auto_keys=auto_keys,
        stages=select_stage_flow(
            preset.stages,
            no_stages=no_stages,
            no_bios_keys=no_bios_keys,
        ),
        pre_launch_commands=tuple(),
    )


def parse_registers(log_text: str) -> dict[str, int]:
    marker = f"LOG: EV of '{REGISTER_COMMAND.removeprefix('EV ')}' is:"
    marker_index = log_text.rfind(marker)
    if marker_index < 0:
        raise ValueError("missing EV register output in DOSBox-X log")
    register_line = log_text[marker_index:].splitlines()[1].removeprefix("LOG:").strip()
    values = register_line.split()
    if len(values) != len(REGISTER_NAMES):
        raise ValueError(f"unexpected register count in EV output: {register_line!r}")
    return {
        name: int(value, 16)
        for name, value in zip(REGISTER_NAMES, values, strict=True)
    }


def infer_breakpoint_name(registers: dict[str, int], breakpoints: list[BreakpointSpec]) -> str:
    current = f"{registers['cs']:04X}:{registers['ip']:04X}"
    for breakpoint in breakpoints:
        if current == breakpoint.address.upper():
            return breakpoint.name
    return f"cs{registers['cs']:04x}_ip{registers['ip']:04x}"


def sanitize_path_component(value: str) -> str:
    sanitized = re.sub(r"[^A-Za-z0-9._-]+", "-", value.strip())
    return sanitized.strip(".-") or "checkpoint"


def normalize_trace_name(name: str) -> str:
    return name.strip().upper()


def is_relevant_trace_file(normalized_name: str) -> bool:
    return not normalized_name.startswith("Z:\\") and normalized_name not in TRACE_DEVICE_NAMES


def parse_file_trace(log_path: Path) -> dict[str, Any] | None:
    if not log_path.exists():
        return None

    lines = log_path.read_text(encoding="utf-8", errors="replace").splitlines()
    files: dict[str, dict[str, Any]] = {}
    open_events: list[dict[str, Any]] = []

    for line_number, line in enumerate(lines, start=1):
        if match := FILE_OPEN_RE.search(line):
            raw_tick, raw_command, raw_name = match.groups()
            open_command = int(raw_command)
            if open_command != 0:
                continue

            name = raw_name.strip()
            normalized_name = normalize_trace_name(name)
            entry = files.setdefault(
                normalized_name,
                {
                    "name": name,
                    "normalized_name": normalized_name,
                    "open_command": open_command,
                    "open_count": 0,
                    "open_line": None,
                    "open_tick": None,
                    "close_line": None,
                    "close_tick": None,
                    "read_count": 0,
                    "total_bytes_read": 0,
                    "read_sizes": [],
                },
            )
            entry["open_count"] = int(entry["open_count"]) + 1
            if entry["open_line"] is None:
                entry["open_line"] = line_number
                entry["open_tick"] = int(raw_tick)
            open_events.append(
                {
                    "name": name,
                    "normalized_name": normalized_name,
                    "open_line": line_number,
                    "open_tick": int(raw_tick),
                }
            )
            continue

        if match := FILE_READ_RE.search(line):
            _, raw_size, raw_name = match.groups()
            name = raw_name.strip()
            normalized_name = normalize_trace_name(name)
            entry = files.setdefault(
                normalized_name,
                {
                    "name": name,
                    "normalized_name": normalized_name,
                    "open_command": None,
                    "open_count": 0,
                    "open_line": None,
                    "open_tick": None,
                    "close_line": None,
                    "close_tick": None,
                    "read_count": 0,
                    "total_bytes_read": 0,
                    "read_sizes": [],
                },
            )
            size = int(raw_size)
            entry["read_count"] = int(entry["read_count"]) + 1
            entry["total_bytes_read"] = int(entry["total_bytes_read"]) + size
            read_sizes = entry["read_sizes"]
            assert isinstance(read_sizes, list)
            read_sizes.append(size)
            continue

        if match := FILE_CLOSE_RE.search(line):
            raw_tick, raw_name = match.groups()
            normalized_name = normalize_trace_name(raw_name)
            entry = files.get(normalized_name)
            if entry is not None and entry["close_line"] is None:
                entry["close_line"] = line_number
                entry["close_tick"] = int(raw_tick)

    startup_open_events = [
        event for event in open_events if is_relevant_trace_file(event["normalized_name"])
    ]

    startup_sequence: list[dict[str, Any]] = []
    seen_names: set[str] = set()
    for event in startup_open_events:
        normalized_name = event["normalized_name"]
        if normalized_name in seen_names:
            continue
        seen_names.add(normalized_name)
        entry = files[normalized_name]
        startup_sequence.append(
            {
                "name": entry["name"],
                "normalized_name": normalized_name,
                "open_line": entry["open_line"],
                "open_tick": entry["open_tick"],
                "open_count": entry["open_count"],
                "read_count": entry["read_count"],
                "total_bytes_read": entry["total_bytes_read"],
                "read_sizes": entry["read_sizes"],
                "close_line": entry["close_line"],
                "close_tick": entry["close_tick"],
            }
        )

    return {
        "log_line_count": len(lines),
        "open_event_count": len(open_events),
        "startup_open_events": startup_open_events,
        "startup_sequence": startup_sequence,
        "files": files,
    }


def current_log_snapshot(session: DosboxDebuggerSession) -> dict[str, int]:
    text = session.read_log()
    return {
        "log_byte_count": len(text.encode("utf-8", errors="replace")),
        "log_line_count": len(text.splitlines()),
    }


def make_phase_marker(
    session: DosboxDebuggerSession,
    name: str,
    kind: str,
    status: str,
) -> dict[str, Any]:
    snapshot = current_log_snapshot(session)
    return {
        "name": name,
        "kind": kind,
        "status": status,
        **snapshot,
    }


def build_phase_file_trace(
    file_trace: dict[str, Any] | None,
    phase_markers: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    if not file_trace or not phase_markers:
        return []

    open_events = file_trace.get("startup_open_events")
    if not isinstance(open_events, list):
        return []

    phase_summaries: list[dict[str, Any]] = []
    event_index = 0
    previous_line = 0
    for marker in sorted(phase_markers, key=lambda item: (item["log_line_count"], item["name"])):
        phase_events: list[dict[str, Any]] = []
        while event_index < len(open_events) and open_events[event_index]["open_line"] <= marker["log_line_count"]:
            event = open_events[event_index]
            if event["open_line"] > previous_line:
                phase_events.append(event)
            event_index += 1

        phase_summaries.append(
            {
                "name": marker["name"],
                "kind": marker["kind"],
                "status": marker["status"],
                "log_line_count": marker["log_line_count"],
                "log_byte_count": marker["log_byte_count"],
                "opened_files": [event["name"] for event in phase_events],
                "opened_file_lines": [event["open_line"] for event in phase_events],
            }
        )
        previous_line = marker["log_line_count"]

    return phase_summaries


def derived_road_window_dump(renderer_state_dump: dict[str, Any]) -> DumpSpec | None:
    road_row_group = renderer_state_dump.get("road_row_group")
    if not isinstance(road_row_group, int):
        return None
    offset = ROAD_WINDOW_BASE_OFFSET + road_row_group * ROAD_WINDOW_ROW_STRIDE + ROAD_WINDOW_ACTIVE_OFFSET
    return DumpSpec("active_road_window", "DS", f"{offset:04X}", ROAD_WINDOW_ROW_STRIDE)


def derived_trekdat_pointer_grid_dump(
    renderer_state_dump: dict[str, Any],
    trekdat_segment_table_dump: dict[str, Any],
) -> DumpSpec | None:
    trekdat_slot = renderer_state_dump.get("trekdat_slot")
    segments = trekdat_segment_table_dump.get("segments")
    if not isinstance(trekdat_slot, int) or not isinstance(segments, list):
        return None
    if trekdat_slot < 0 or trekdat_slot >= len(segments):
        return None
    segment = segments[trekdat_slot]
    if not isinstance(segment, int) or segment == 0:
        return None
    return DumpSpec(
        "active_trekdat_pointer_grid",
        f"{segment:04X}",
        "0000",
        TREKDAT_POINTER_GRID_BYTES,
    )


def build_derived_dump_specs(dump_results: list[dict[str, Any]]) -> list[DumpSpec]:
    dumps_by_name = {dump["name"]: dump for dump in dump_results}
    renderer_state_dump = dumps_by_name.get("renderer_state")
    if not isinstance(renderer_state_dump, dict):
        return []

    derived_specs: list[DumpSpec] = []
    road_window_dump = derived_road_window_dump(renderer_state_dump)
    if road_window_dump is not None:
        derived_specs.append(road_window_dump)

    trekdat_segment_table_dump = dumps_by_name.get("trekdat_segment_table")
    if isinstance(trekdat_segment_table_dump, dict):
        trekdat_dump = derived_trekdat_pointer_grid_dump(
            renderer_state_dump,
            trekdat_segment_table_dump,
        )
        if trekdat_dump is not None:
            derived_specs.append(trekdat_dump)

    return derived_specs


def interpret_dump(dump: DumpSpec, data: bytes) -> dict[str, Any]:
    result: dict[str, Any] = {
        "name": dump.name,
        "address": dump.debugger_address(),
        "length": len(data),
    }
    if dump.name == "renderer_state" and len(data) >= 2:
        current_row = int.from_bytes(data[0:2], "little")
        result["current_row"] = current_row
        result["road_row_group"] = current_row >> 3
        result["trekdat_slot"] = current_row & 7
        result["raw_words"] = [
            int.from_bytes(data[index : index + 2], "little")
            for index in range(0, min(len(data), 16), 2)
        ]
    elif dump.name == "tile_class_by_low3":
        result["values"] = list(data)
    elif dump.name == "draw_dispatch_by_type":
        result["targets"] = [
            int.from_bytes(data[index : index + 2], "little")
            for index in range(0, len(data), 2)
        ]
    elif dump.name == "trekdat_segment_table":
        result["segments"] = [
            int.from_bytes(data[index : index + 2], "little")
            for index in range(0, len(data), 2)
        ]
    elif dump.name == "active_road_window":
        result["byte_values"] = list(data)
        result["word_values"] = [
            int.from_bytes(data[index : index + 2], "little")
            for index in range(0, len(data), 2)
        ]
    elif dump.name == "active_trekdat_pointer_grid":
        words = [
            int.from_bytes(data[index : index + 2], "little")
            for index in range(0, len(data), 2)
        ]
        result["pointer_word_count"] = len(words)
        result["nonzero_pointer_count"] = sum(1 for value in words if value != 0)
        result["first_pointer_words"] = words[:16]
    elif dump.name == VGA_FRAME_DUMP_NAME:
        result["width"] = 320
        result["height"] = 200
        result["row_stride"] = 320
    return result


def build_markdown(summary: dict[str, Any]) -> str:
    file_trace = summary.get("file_trace")
    lines = [
        "# SkyRoads DOS Oracle Capture",
        "",
        f"- Preset: `{summary['preset']}`",
        f"- Source root: `{summary['source_root']}`",
        f"- DOSBox-X: `{summary['dosbox']}`",
        f"- Key backend: `{summary['key_backend']}`",
        f"- Launch input backend: `{summary['launch_input_backend']}`",
        f"- Screenshot backend: `{summary['screenshot_backend']}`",
        f"- Cycles: `{summary['cycles']}`",
        f"- Time limit: `{summary['time_limit_seconds']}` seconds",
        f"- Captured checkpoints: `{len(summary['checkpoints'])}`",
        "",
        "## Breakpoints",
        "",
    ]
    if summary.get("startup_registers"):
        startup = summary["startup_registers"]
        lines.extend(
            [
                f"- Startup CS:IP `{startup['cs']:04X}:{startup['ip']:04X}`, "
                f"`DS={startup['ds']:04X}`, `SS={startup['ss']:04X}`",
                "",
            ]
        )
    for breakpoint in summary["breakpoints"]:
        if "image_offset" in breakpoint:
            lines.append(
                f"- `{breakpoint['name']}` at `{breakpoint['address']}` "
                f"(image offset `0x{breakpoint['image_offset']:04X}`)"
            )
        else:
            lines.append(f"- `{breakpoint['name']}` at `{breakpoint['address']}`")
    if summary.get("stage_screenshots"):
        lines.extend(["", "## Stage Screenshots", ""])
        for item in summary["stage_screenshots"]:
            lines.append(
                f"- step `{item['step_index']}` stage `{item['stage']}` "
                f"(repeat `{item['repeat_index']}`): `{item['path']}`"
            )
    if file_trace and file_trace.get("startup_sequence"):
        lines.extend(["", "## Startup File Order", ""])
        for index, item in enumerate(file_trace["startup_sequence"], start=1):
            lines.append(
                f"{index}. `{item['name']}`: opened at log line `{item['open_line']}`, "
                f"{item['open_count']} open(s), {item['read_count']} read(s), "
                f"{item['total_bytes_read']} byte(s)"
            )
    if file_trace and file_trace.get("phase_summaries"):
        lines.extend(["", "## File Opens By Phase", ""])
        for phase in file_trace["phase_summaries"]:
            label = f"{phase['kind']} `{phase['name']}`"
            if phase["status"] != "completed":
                label += f" ({phase['status']})"
            if phase["opened_files"]:
                opened = ", ".join(f"`{name}`" for name in phase["opened_files"])
                lines.append(f"- {label}: {opened}")
            else:
                lines.append(f"- {label}: none")
    lines.extend(["", "## Checkpoints", ""])
    for checkpoint in summary["checkpoints"]:
        registers = checkpoint["registers"]
        lines.append(
            f"- `{checkpoint['checkpoint_name']}`: hit `{checkpoint['hit_index']}`, "
            f"`CS:IP={registers['cs']:04X}:{registers['ip']:04X}`, "
            f"`DS={registers['ds']:04X}`, `SS={registers['ss']:04X}`"
        )
        if checkpoint.get("breakpoint_name") and checkpoint["breakpoint_name"] != checkpoint["checkpoint_name"]:
            lines.append(f"  - breakpoint: `{checkpoint['breakpoint_name']}`")
        for dump in checkpoint["dumps"]:
            if dump["name"] == "renderer_state" and "current_row" in dump:
                lines.append(
                    f"  - renderer state: current_row `{dump['current_row']}`, "
                    f"group `{dump['road_row_group']}`, slot `{dump['trekdat_slot']}`"
                )
            elif dump["name"] == "trekdat_segment_table":
                segments = ", ".join(f"{value:04X}" for value in dump["segments"])
                lines.append(f"  - TREKDAT segments: `{segments}`")
            elif dump["name"] == "active_road_window":
                values = " ".join(f"{value:02X}" for value in dump["byte_values"])
                lines.append(f"  - active road bytes: `{values}`")
            elif dump["name"] == "active_trekdat_pointer_grid":
                lines.append(
                    f"  - active TREKDAT pointer words: `{dump['pointer_word_count']}` "
                    f"({dump['nonzero_pointer_count']} non-zero)"
                )
            elif dump["name"] == VGA_FRAME_DUMP_NAME and "sha256" in dump:
                lines.append(f"  - VGA frame SHA-256: `{dump['sha256']}`")
    if summary.get("fixture_bundles"):
        lines.extend(["", "## Fixture Bundles", ""])
        for fixture in summary["fixture_bundles"]:
            lines.append(
                f"- `{fixture['checkpoint_name']}` -> `{fixture['fixture_path']}`"
            )
            if fixture.get("frame_sha256"):
                lines.append(f"  - frame SHA-256: `{fixture['frame_sha256']}`")
    if summary.get("notes"):
        lines.extend(["", "## Notes", ""])
        for note in summary["notes"]:
            lines.append(f"- {note}")
    return "\n".join(lines) + "\n"


def capture_stage_screenshot(output_root: Path, stage_name: str, step_index: int) -> str:
    backend = detect_screenshot_backend()
    if backend != "macos":
        raise RuntimeError("no supported screenshot backend is available on this host")
    screenshot_path = output_root / f"stage_{step_index:02d}_{stage_name}.png"
    subprocess.run(
        [MACOS_SCREENSHOT_TOOL, "-x", str(screenshot_path)],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return str(screenshot_path)


def find_child_pid(parent_pid: int) -> int | None:
    try:
        output = subprocess.check_output(["pgrep", "-P", str(parent_pid)], text=True)
    except (OSError, subprocess.CalledProcessError):
        return None
    for line in output.splitlines():
        line = line.strip()
        if line:
            return int(line)
    return None


def send_host_key(backend: str, dosbox_pid: int | None, key_name: str) -> None:
    if backend == "macos":
        send_macos_key(dosbox_pid, key_name)
        return
    if backend == "powershell":
        send_powershell_key(key_name)
        return
    raise RuntimeError(f"unsupported host key backend: {backend}")


def send_macos_key(dosbox_pid: int | None, key_name: str) -> None:
    if key_name not in MACOS_KEY_CODES:
        raise ValueError(f"unsupported key name {key_name!r}")
    if shutil.which("osascript") is None:
        raise RuntimeError("missing required host tool: osascript")
    key_code = MACOS_KEY_CODES[key_name]
    if dosbox_pid is None:
        activation_script = 'tell application "System Events" to tell process "dosbox-x" to set frontmost to true'
    else:
        activation_script = (
            'tell application "System Events" to set frontmost of '
            f'(first application process whose unix id is {dosbox_pid}) to true'
        )
    subprocess.run(
        [
            "osascript",
            "-e",
            activation_script,
            "-e",
            f'tell application "System Events" to key code {key_code}',
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def send_powershell_key(key_name: str) -> None:
    if key_name not in POWERSHELL_SEND_KEYS:
        raise ValueError(f"unsupported key name {key_name!r}")
    if not WSL_POWERSHELL.exists():
        raise RuntimeError(f"missing required host tool: {WSL_POWERSHELL}")

    key_sequence = POWERSHELL_SEND_KEYS[key_name]
    script = f"""
$process = Get-Process | Where-Object {{
    $_.MainWindowTitle -and (
        $_.MainWindowTitle -like '*DOSBox-X*' -or
        $_.MainWindowTitle -like '*SkyRoads*'
    )
}} | Select-Object -First 1
if ($null -eq $process) {{
    throw 'could not find a DOSBox-X window on the Windows host'
}}
$wshell = New-Object -ComObject WScript.Shell
if (-not $wshell.AppActivate($process.MainWindowTitle)) {{
    throw "could not activate DOSBox-X window '$($process.MainWindowTitle)' on the Windows host"
}}
Start-Sleep -Milliseconds 100
$wshell.SendKeys('{key_sequence}')
"""
    subprocess.run(
        [str(WSL_POWERSHELL), "-NoProfile", "-Command", script],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def capture_register_snapshot(session: DosboxDebuggerSession) -> dict[str, int]:
    previous_length = len(session.read_log())
    session.send_line(REGISTER_COMMAND)
    log_text = session.wait_for_substring("EV of", previous_length, 5.0)
    return parse_registers(log_text)


def capture_dump_file(
    session: DosboxDebuggerSession,
    dump: DumpSpec,
    checkpoint_dir: Path,
) -> dict[str, Any]:
    clear_stale_memdump_files(session.output_root)
    session.send_line(f"MEMDUMPBIN {dump.debugger_address()} {dump.length:X}")
    memdump_path = wait_for_memdump_file(session, 5.0)
    target_path = checkpoint_dir / f"{dump.name}.bin"
    memdump_path.replace(target_path)
    raw = target_path.read_bytes()
    result = interpret_dump(dump, raw)
    result["sha256"] = hashlib.sha256(raw).hexdigest()
    result["path"] = str(target_path)
    return result


def capture_screenshot(checkpoint_dir: Path, checkpoint_name: str) -> str:
    backend = detect_screenshot_backend()
    if backend != "macos":
        raise RuntimeError("no supported screenshot backend is available on this host")
    screenshot_path = checkpoint_dir / f"{checkpoint_name}.png"
    subprocess.run(
        [MACOS_SCREENSHOT_TOOL, "-x", str(screenshot_path)],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return str(screenshot_path)


def capture_named_screenshot(output_root: Path, name: str) -> str:
    backend = detect_screenshot_backend()
    if backend != "macos":
        raise RuntimeError("no supported screenshot backend is available on this host")
    screenshot_path = output_root / f"{name}.png"
    subprocess.run(
        [MACOS_SCREENSHOT_TOOL, "-x", str(screenshot_path)],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return str(screenshot_path)


def resolve_breakpoints_for_registers(
    requested: list[BreakpointSpec],
    registers: dict[str, int],
) -> list[BreakpointSpec]:
    code_segment = registers["cs"]
    return [breakpoint.resolve(code_segment) for breakpoint in requested]


def wait_for_memdump_file(session: DosboxDebuggerSession, timeout_seconds: float) -> Path:
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        session.read_log()
        for name in MEMDUMP_BIN_NAMES:
            candidate = session.output_root / name
            if candidate.exists():
                return candidate
        if session.process is not None and session.process.poll() is not None:
            raise RuntimeError(
                f"DOSBox-X exited with status {session.process.returncode} before writing a memdump file"
            )
        time.sleep(0.05)
    expected_names = ", ".join(MEMDUMP_BIN_NAMES)
    raise TimeoutError(
        f"timed out after {timeout_seconds:.1f}s waiting for DOSBox-X to write one of: {expected_names}"
    )


def clear_stale_memdump_files(output_root: Path) -> None:
    for name in MEMDUMP_BIN_NAMES:
        candidate = output_root / name
        if candidate.exists():
            candidate.unlink()


def preload_bios_keyboard_buffer(session: DosboxDebuggerSession, key_names: tuple[str, ...]) -> None:
    if not key_names:
        return
    if len(key_names) > BIOS_KEYBOARD_BUFFER_CAPACITY:
        raise ValueError(
            f"cannot preload {len(key_names)} BIOS keys; capacity is {BIOS_KEYBOARD_BUFFER_CAPACITY}"
        )

    buffer_bytes: list[int] = []
    for key_name in key_names:
        try:
            ascii_code, scan_code = BIOS_KEYWORDS[key_name]
        except KeyError as exc:
            raise ValueError(f"unsupported BIOS key name {key_name!r}") from exc
        buffer_bytes.extend([ascii_code, scan_code])

    head_bytes = [BIOS_KEYBOARD_BUFFER_OFFSET & 0xFF, BIOS_KEYBOARD_BUFFER_OFFSET >> 8]
    tail_offset = BIOS_KEYBOARD_BUFFER_OFFSET + len(buffer_bytes)
    tail_bytes = [tail_offset & 0xFF, tail_offset >> 8]

    session.send_line(
        "SM "
        f"{BIOS_KEYBOARD_SEGMENT}:{BIOS_KEYBOARD_HEAD_OFFSET:04X} "
        + " ".join(f"{value:02X}" for value in [*head_bytes, *tail_bytes])
    )
    session.send_line(
        "SM "
        f"{BIOS_KEYBOARD_SEGMENT}:{BIOS_KEYBOARD_BUFFER_OFFSET:04X} "
        + " ".join(f"{value:02X}" for value in buffer_bytes)
    )


def capture_checkpoint(
    session: DosboxDebuggerSession,
    output_root: Path,
    checkpoint_name: str | None,
    hit_index: int,
    breakpoints: list[BreakpointSpec],
    dumps: list[DumpSpec],
    capture_screen_enabled: bool,
    notes: list[str],
) -> dict[str, Any]:
    registers = capture_register_snapshot(session)
    breakpoint_name = infer_breakpoint_name(registers, breakpoints)
    resolved_name = sanitize_path_component(checkpoint_name or breakpoint_name)
    checkpoint_dir = output_root / resolved_name
    checkpoint_dir.mkdir(parents=True, exist_ok=True)

    dump_results = [capture_dump_file(session, dump, checkpoint_dir) for dump in dumps]
    derived_specs = build_derived_dump_specs(dump_results)
    existing_names = {dump["name"] for dump in dump_results}
    for derived_dump in derived_specs:
        if derived_dump.name in existing_names:
            continue
        dump_results.append(capture_dump_file(session, derived_dump, checkpoint_dir))
        existing_names.add(derived_dump.name)

    screenshot_path = None
    if capture_screen_enabled:
        try:
            screenshot_path = capture_screenshot(checkpoint_dir, resolved_name)
        except Exception as exc:  # pragma: no cover - host integration
            notes.append(f"Host screenshot capture failed for `{resolved_name}`: {exc}")

    return {
        "hit_index": hit_index,
        "checkpoint_name": resolved_name,
        "breakpoint_name": breakpoint_name,
        "registers": registers,
        "dumps": dump_results,
        "screenshot": screenshot_path,
    }


def default_checkpoint_specs(resume_command: str) -> tuple[CheckpointSpec, ...]:
    return (CheckpointSpec(name=None, resume_command=resume_command),)


def fixture_bundle_from_checkpoint(
    fixture_root: Path,
    preset_name: str,
    checkpoint: dict[str, Any],
) -> dict[str, Any]:
    checkpoint_name = sanitize_path_component(str(checkpoint["checkpoint_name"]))
    fixture_dir = fixture_root / sanitize_path_component(preset_name) / checkpoint_name
    fixture_dir.mkdir(parents=True, exist_ok=True)

    frame_sha256 = None
    dumps = []
    for dump in checkpoint["dumps"]:
        fixture_dump = {
            "name": dump["name"],
            "address": dump["address"],
            "length": dump["length"],
            "sha256": dump["sha256"],
        }
        if dump["name"] == VGA_FRAME_DUMP_NAME:
            frame_sha256 = dump["sha256"]
        for key in (
            "current_row",
            "road_row_group",
            "trekdat_slot",
            "raw_words",
            "values",
            "targets",
            "segments",
            "byte_values",
            "word_values",
            "pointer_word_count",
            "nonzero_pointer_count",
            "first_pointer_words",
            "width",
            "height",
            "row_stride",
        ):
            if key in dump:
                fixture_dump[key] = dump[key]
        dumps.append(fixture_dump)

    fixture = {
        "bundle_version": FIXTURE_BUNDLE_VERSION,
        "preset": preset_name,
        "checkpoint_name": checkpoint_name,
        "breakpoint_name": checkpoint["breakpoint_name"],
        "hit_index": checkpoint["hit_index"],
        "registers": checkpoint["registers"],
        "frame_sha256": frame_sha256,
        "dumps": dumps,
    }
    fixture_path = fixture_dir / "fixture.json"
    fixture_path.write_text(json.dumps(fixture, indent=2, sort_keys=True) + "\n", encoding="ascii")
    return {
        "checkpoint_name": checkpoint_name,
        "fixture_path": str(fixture_path),
        "frame_sha256": frame_sha256,
    }


def write_fixture_bundles(
    fixture_root: Path,
    preset_name: str,
    checkpoints: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    fixture_root.mkdir(parents=True, exist_ok=True)
    return [
        fixture_bundle_from_checkpoint(fixture_root, preset_name, checkpoint)
        for checkpoint in checkpoints
    ]


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Capture DOSBox-X debugger checkpoints from the original SkyRoads executable."
    )
    parser.add_argument("--source", type=Path, default=Path("."), help="Path containing SKYROADS.EXE and data files.")
    parser.add_argument("--output", type=Path, required=True, help="Directory to write logs, dumps, and summaries.")
    parser.add_argument(
        "--dosbox",
        type=Path,
        default=default_dosbox_path(),
        help="Path to the DOSBox-X binary.",
    )
    parser.add_argument("--time-limit", type=int, default=30, help="DOSBox-X run time limit in seconds.")
    parser.add_argument(
        "--cycles",
        default="max",
        help="DOSBox cycles setting passed as `cycles VALUE` before launching SKYROADS.EXE.",
    )
    parser.add_argument(
        "--checkpoint-timeout",
        type=float,
        default=15.0,
        help="Seconds to wait for each breakpoint hit after resuming execution.",
    )
    parser.add_argument(
        "--startup-timeout",
        type=float,
        default=15.0,
        help="Seconds to wait for the initial DOSBox-X debugger prompt.",
    )
    parser.add_argument(
        "--preset",
        choices=sorted(PRESETS),
        default=ROAD0_INITIAL_FRAME.name,
        help="Named capture preset.",
    )
    parser.add_argument(
        "--breakpoint",
        action="append",
        default=[],
        help="Additional breakpoint in NAME=SEG:OFF format. Can be repeated.",
    )
    parser.add_argument(
        "--debugger-command",
        action="append",
        default=[],
        help="Additional raw DOSBox-X debugger command to send before resuming execution.",
    )
    parser.add_argument(
        "--resume-command",
        choices=("f5", "vrt"),
        default="f5",
        help="How to leave the debugger after setup. 'f5' runs normally; 'vrt' stops again on the next vertical retrace.",
    )
    parser.add_argument(
        "--dump",
        action="append",
        default=[],
        help="Additional dump in NAME=SEG:OFF:LENGTH format. LENGTH may be decimal or 0x-prefixed.",
    )
    parser.add_argument(
        "--capture-screen",
        action="store_true",
        help="Capture a host screenshot after each checkpoint prompt.",
    )
    parser.add_argument(
        "--capture-vga-frame",
        action="store_true",
        help=(
            "Dump A000:0000 (320x200 mode-13h VGA memory) at each checkpoint prompt. "
            "Use this only when the chosen prompt is known to be post-render."
        ),
    )
    parser.add_argument(
        "--no-bios-keys",
        action="store_true",
        help="Disable the preset's BIOS keyboard-buffer preload.",
    )
    parser.add_argument(
        "--warmup-vrt-count",
        type=int,
        default=None,
        help="Override the preset's number of initial VRT cycles before BIOS key injection.",
    )
    parser.add_argument(
        "--no-auto-keys",
        action="store_true",
        help="Disable the preset's built-in automatic launch key sequence.",
    )
    parser.add_argument(
        "--no-stages",
        action="store_true",
        help="Disable any preset-defined staged debugger/input sequence.",
    )
    parser.add_argument(
        "--write-fixtures",
        action="store_true",
        help=(
            "Normalize captured checkpoints into fixture bundles under the repo fixture root "
            "or the path provided by --fixture-root."
        ),
    )
    parser.add_argument(
        "--fixture-root",
        type=Path,
        default=None,
        help=(
            "Override the normalized fixture root. Defaults to "
            f"{DEFAULT_FIXTURE_ROOT} under --source."
        ),
    )
    args = parser.parse_args()

    source_root = args.source.resolve()
    output_root = args.output.resolve()
    dosbox = args.dosbox.resolve()
    preset = PRESETS[args.preset]

    if not (source_root / "SKYROADS.EXE").exists():
        raise SystemExit(f"missing SKYROADS.EXE under {source_root}")
    if not dosbox.exists():
        raise SystemExit(f"missing DOSBox-X binary: {dosbox}")

    output_root.mkdir(parents=True, exist_ok=True)

    extra_breakpoints = [parse_breakpoint(value) for value in args.breakpoint]
    extra_dumps = [parse_dump(value) for value in args.dump]
    unresolved_breakpoints = list(preset.breakpoints) + extra_breakpoints
    dumps = list(preset.dumps) + extra_dumps
    if args.capture_vga_frame:
        dumps.append(DumpSpec(VGA_FRAME_DUMP_NAME, VGA_FRAME_SEGMENT, VGA_FRAME_OFFSET, VGA_FRAME_BYTES))
    key_backend = detect_key_backend()
    screenshot_backend = detect_screenshot_backend()

    notes: list[str] = []
    checkpoints: list[dict[str, Any]] = []
    stage_screenshots: list[dict[str, Any]] = []
    warmup_vrt_count = preset.warmup_vrt_count if args.warmup_vrt_count is None else args.warmup_vrt_count
    launch_plan = build_launch_plan(
        preset,
        key_backend=key_backend,
        no_auto_keys=args.no_auto_keys,
        no_bios_keys=args.no_bios_keys,
        no_stages=args.no_stages,
    )
    guest_launch_sequence = launch_plan.guest_launch_sequence
    auto_keys = launch_plan.auto_keys
    launch_input_backend = launch_plan.launch_input_backend
    bios_keys = launch_plan.bios_keys
    stages = launch_plan.stages
    pre_launch_commands = launch_plan.pre_launch_commands
    session = DosboxDebuggerSession(
        source_root,
        output_root,
        dosbox,
        args.time_limit,
        args.cycles,
        pre_launch_commands=pre_launch_commands,
    )
    key_thread: HostKeySequence | None = None
    prompt_count = 0
    run_error: Exception | None = None
    failure_screenshot_path: str | None = None
    startup_registers: dict[str, int] | None = None
    breakpoints: list[BreakpointSpec] = []
    breakpoints_resolved = False
    fixture_bundles: list[dict[str, Any]] = []
    phase_markers: list[dict[str, Any]] = []
    active_phase_name: str | None = None
    active_phase_kind: str | None = None

    try:
        session.start()
        _, prompt_count = session.wait_for_prompt(0, args.startup_timeout)
        startup_registers = capture_register_snapshot(session)
        if all(breakpoint.address is not None for breakpoint in unresolved_breakpoints):
            breakpoints = resolve_breakpoints_for_registers(unresolved_breakpoints, startup_registers)
            for breakpoint in breakpoints:
                session.send_line(f"BP {breakpoint.address}")
            breakpoints_resolved = True
            notes.append(
                f"Resolved breakpoints against startup CS `{startup_registers['cs']:04X}`."
            )
        else:
            notes.append(
                "Deferring EXE-relative breakpoint resolution until execution reaches the "
                "loaded SkyRoads code segment."
            )
        for command in args.debugger_command:
            session.send_line(command)
        for vrt_index in range(warmup_vrt_count):
            session.send_line("VRT")
            _, prompt_count = session.wait_for_prompt(prompt_count, args.checkpoint_timeout)
            if not breakpoints_resolved:
                current_registers = capture_register_snapshot(session)
                if current_registers["cs"] != 0xF000:
                    breakpoints = resolve_breakpoints_for_registers(unresolved_breakpoints, current_registers)
                    for breakpoint in breakpoints:
                        session.send_line(f"BP {breakpoint.address}")
                    breakpoints_resolved = True
                    notes.append(
                        f"Resolved EXE-relative breakpoints against runtime CS `{current_registers['cs']:04X}` "
                        f"during warm-up VRT {vrt_index + 1}/{warmup_vrt_count}."
                    )
            notes.append(f"Completed warm-up VRT {vrt_index + 1}/{warmup_vrt_count}.")
        if guest_launch_sequence is not None:
            notes.append(
                f"Scheduled the preset guest ADDKEY launch sequence with {len(pre_launch_commands)} timed key event(s)."
            )
        if bios_keys:
            preload_bios_keyboard_buffer(session, bios_keys)
            notes.append(
                "Preloaded the BIOS keyboard buffer with: "
                + ", ".join(bios_keys)
            )

        dosbox_pid = find_child_pid(session.process.pid) if session.process is not None else None
        if auto_keys:
            key_thread = HostKeySequence(key_backend, dosbox_pid, auto_keys)
            key_thread.start()
            notes.append(
                f"Started the preset {key_backend} key sequence to skip the intro and launch Road 0."
            )
        elif guest_launch_sequence is None:
            if args.no_auto_keys:
                notes.append(
                    "No automatic key sequence was used because --no-auto-keys was set."
                )
            elif key_backend is None:
                notes.append(
                    "No automatic key sequence was used because no supported host key backend is available."
                )
            else:
                notes.append(
                    "No automatic key sequence was used; navigate the DOS window manually before the breakpoint fires."
                )

        stage_step_index = 0
        if warmup_vrt_count:
            phase_markers.append(make_phase_marker(session, "warmup", "warmup", "completed"))
        for stage in stages:
            active_phase_name = stage.name
            active_phase_kind = "stage"
            if stage.bios_keys:
                preload_bios_keyboard_buffer(session, stage.bios_keys)
                notes.append(
                    f"Preloaded the BIOS keyboard buffer for stage `{stage.name}` with: "
                    + ", ".join(stage.bios_keys)
                )
            timeout_seconds = stage.timeout_seconds or args.checkpoint_timeout
            for repeat_index in range(stage.repeat_count):
                if stage.resume_command == "f5":
                    session.resume()
                else:
                    session.send_line("VRT")
                _, prompt_count = session.wait_for_prompt(prompt_count, timeout_seconds)
                if not breakpoints_resolved:
                    current_registers = capture_register_snapshot(session)
                    if current_registers["cs"] != 0xF000:
                        breakpoints = resolve_breakpoints_for_registers(unresolved_breakpoints, current_registers)
                        for breakpoint in breakpoints:
                            session.send_line(f"BP {breakpoint.address}")
                        breakpoints_resolved = True
                        notes.append(
                            f"Resolved EXE-relative breakpoints against runtime CS `{current_registers['cs']:04X}` "
                            f"during stage `{stage.name}`."
                        )
                stage_step_index += 1
                if stage.capture_screen and screenshot_backend is not None:
                    try:
                        screenshot_path = capture_stage_screenshot(output_root, stage.name, stage_step_index)
                        stage_screenshots.append(
                            {
                                "stage": stage.name,
                                "step_index": stage_step_index,
                                "repeat_index": repeat_index + 1,
                                "path": screenshot_path,
                            }
                        )
                    except Exception as exc:  # pragma: no cover - host integration
                        notes.append(f"Stage screenshot capture failed for `{stage.name}`: {exc}")
                elif stage.capture_screen and repeat_index == 0:
                    notes.append(
                        f"Skipped stage screenshot for `{stage.name}` because no supported screenshot backend is available."
                    )
            notes.append(
                f"Completed stage `{stage.name}` via `{stage.resume_command}` x{stage.repeat_count}."
            )
            phase_markers.append(make_phase_marker(session, stage.name, "stage", "completed"))
            active_phase_name = None
            active_phase_kind = None

        checkpoint_specs = preset.checkpoints or default_checkpoint_specs(args.resume_command)
        for hit_index, checkpoint_spec in enumerate(checkpoint_specs, start=1):
            active_phase_name = checkpoint_spec.name or str(hit_index)
            active_phase_kind = "checkpoint"
            timeout_seconds = checkpoint_spec.timeout_seconds or args.checkpoint_timeout
            if checkpoint_spec.frame_bios_keys:
                for frame_index, frame_bios_keys in enumerate(checkpoint_spec.frame_bios_keys, start=1):
                    if frame_bios_keys:
                        preload_bios_keyboard_buffer(session, frame_bios_keys)
                        notes.append(
                            f"Preloaded the BIOS keyboard buffer for checkpoint "
                            f"`{checkpoint_spec.name or hit_index}` frame step `{frame_index}` with: "
                            + ", ".join(frame_bios_keys)
                        )
                    if checkpoint_spec.resume_command == "f5":
                        session.resume()
                    else:
                        session.send_line("VRT")
                    _, prompt_count = session.wait_for_prompt(prompt_count, timeout_seconds)
                    if not breakpoints_resolved:
                        current_registers = capture_register_snapshot(session)
                        if current_registers["cs"] != 0xF000:
                            breakpoints = resolve_breakpoints_for_registers(
                                unresolved_breakpoints,
                                current_registers,
                            )
                            for breakpoint in breakpoints:
                                session.send_line(f"BP {breakpoint.address}")
                            breakpoints_resolved = True
                            notes.append(
                                f"Resolved EXE-relative breakpoints against runtime CS "
                                f"`{current_registers['cs']:04X}` during checkpoint "
                                f"`{checkpoint_spec.name or hit_index}` frame step `{frame_index}`."
                            )
            else:
                if checkpoint_spec.bios_keys:
                    preload_bios_keyboard_buffer(session, checkpoint_spec.bios_keys)
                    notes.append(
                        f"Preloaded the BIOS keyboard buffer for checkpoint "
                        f"`{checkpoint_spec.name or hit_index}` with: "
                        + ", ".join(checkpoint_spec.bios_keys)
                    )
                for _ in range(checkpoint_spec.repeat_count):
                    if checkpoint_spec.resume_command == "f5":
                        session.resume()
                    else:
                        session.send_line("VRT")
                    _, prompt_count = session.wait_for_prompt(prompt_count, timeout_seconds)
                    if not breakpoints_resolved:
                        current_registers = capture_register_snapshot(session)
                        if current_registers["cs"] != 0xF000:
                            breakpoints = resolve_breakpoints_for_registers(unresolved_breakpoints, current_registers)
                            for breakpoint in breakpoints:
                                session.send_line(f"BP {breakpoint.address}")
                            breakpoints_resolved = True
                            notes.append(
                                f"Resolved EXE-relative breakpoints against runtime CS `{current_registers['cs']:04X}` "
                                f"during checkpoint `{checkpoint_spec.name or hit_index}`."
                            )

            checkpoints.append(
                capture_checkpoint(
                    session,
                    output_root,
                    checkpoint_spec.name,
                    hit_index,
                    breakpoints,
                    dumps,
                    args.capture_screen or checkpoint_spec.capture_screen,
                    notes,
                )
            )
            phase_markers.append(
                make_phase_marker(
                    session,
                    checkpoint_spec.name or str(hit_index),
                    "checkpoint",
                    "captured",
                )
            )
            active_phase_name = None
            active_phase_kind = None

        if key_thread is not None:
            key_thread.stop()
            key_thread.join(timeout=1)

        if args.write_fixtures or args.fixture_root is not None:
            fixture_root = (
                args.fixture_root.resolve()
                if args.fixture_root is not None
                else (source_root / DEFAULT_FIXTURE_ROOT).resolve()
            )
            fixture_bundles = write_fixture_bundles(fixture_root, preset.name, checkpoints)
            notes.append(
                f"Wrote {len(fixture_bundles)} normalized fixture bundle(s) under {fixture_root}."
            )
            if any(bundle.get("frame_sha256") is None for bundle in fixture_bundles):
                notes.append(
                    "One or more fixture bundles do not include a canonical frame hash because "
                    "no `vga_frame` dump was captured at that checkpoint."
                )
    except Exception as exc:
        run_error = exc
        if active_phase_name is not None and active_phase_kind is not None:
            phase_markers.append(
                make_phase_marker(session, active_phase_name, active_phase_kind, "failed")
            )
        notes.append(f"Capture run failed: {exc}")
        if key_thread is not None and key_thread.errors:
            notes.extend(f"Automatic key input failed: {error}" for error in key_thread.errors)
        if args.capture_screen:
            try:
                failure_screenshot_path = capture_named_screenshot(output_root, "failure-state")
                notes.append(f"Captured failure screenshot at {failure_screenshot_path}")
            except Exception as screenshot_exc:  # pragma: no cover - host integration
                notes.append(f"Failure screenshot capture failed: {screenshot_exc}")
    finally:
        if key_thread is not None:
            key_thread.stop()
            key_thread.join(timeout=1)
        session.stop()

    file_trace = parse_file_trace(session.raw_log_path)
    if file_trace is not None:
        file_trace["phase_summaries"] = build_phase_file_trace(file_trace, phase_markers)
        startup_sequence = file_trace.get("startup_sequence")
        if isinstance(startup_sequence, list) and startup_sequence:
            last_startup_file = startup_sequence[-1]
            notes.append(
                f"Observed {len(startup_sequence)} unique game-side file open(s); "
                f"last first-open was `{last_startup_file['name']}` at log line "
                f"`{last_startup_file['open_line']}`."
            )
        phase_summaries = file_trace.get("phase_summaries")
        if isinstance(phase_summaries, list):
            for phase in phase_summaries:
                if phase["status"] != "failed" or not phase["opened_files"]:
                    continue
                opened = ", ".join(phase["opened_files"])
                notes.append(
                    f"During failed {phase['kind']} `{phase['name']}`, new game-side file opens were: {opened}."
                )
                break

    summary = {
        "preset": preset.name,
        "source_root": str(source_root),
        "output_root": str(output_root),
        "dosbox": str(dosbox),
        "key_backend": key_backend or "none",
        "launch_input_backend": launch_input_backend,
        "screenshot_backend": screenshot_backend or "none",
        "cycles": args.cycles,
        "time_limit_seconds": args.time_limit,
        "checkpoint_timeout_seconds": args.checkpoint_timeout,
        "startup_timeout_seconds": args.startup_timeout,
        "startup_registers": startup_registers,
        "requested_breakpoints": [
            {
                "name": item.name,
                **({"address": item.address} if item.address is not None else {}),
                **({"image_offset": item.image_offset} if item.image_offset is not None else {}),
            }
            for item in unresolved_breakpoints
        ],
        "breakpoints": [
            {
                "name": item.name,
                "address": item.address,
                **({"image_offset": item.image_offset} if item.image_offset is not None else {}),
            }
            for item in breakpoints
        ],
        "debugger_commands": list(args.debugger_command),
        "pre_launch_commands": list(pre_launch_commands),
        "resume_command": args.resume_command,
        "guest_launch_sequence": (
            {
                "events": [
                    {
                        "delay_seconds": event.delay_seconds,
                        "key_name": event.key_name,
                    }
                    for event in guest_launch_sequence.events
                ],
                "shell_commands": list(pre_launch_commands),
            }
            if guest_launch_sequence is not None
            else None
        ),
        "bios_keys": list(bios_keys),
        "auto_keys": [
            {
                "delay_seconds": event.delay_seconds,
                "key_name": event.key_name,
            }
            for event in auto_keys
        ],
        "warmup_vrt_count": warmup_vrt_count,
        "stages": [
            {
                "name": stage.name,
                "resume_command": stage.resume_command,
                "repeat_count": stage.repeat_count,
                "bios_keys": list(stage.bios_keys),
                "capture_screen": stage.capture_screen,
                "timeout_seconds": stage.timeout_seconds,
            }
            for stage in stages
        ],
        "stage_screenshots": stage_screenshots,
        "phase_markers": phase_markers,
        "dumps": [
            {
                "name": item.name,
                "address": item.debugger_address(),
                "length": item.length,
            }
            for item in dumps
        ],
        "file_trace": file_trace,
        "checkpoints": checkpoints,
        "fixture_bundles": fixture_bundles,
        "notes": notes,
        "failure_screenshot": failure_screenshot_path,
        "raw_log_path": str(session.raw_log_path),
    }

    (output_root / "summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n",
        encoding="ascii",
    )
    (output_root / "summary.md").write_text(build_markdown(summary), encoding="ascii")
    return 1 if run_error is not None else 0


if __name__ == "__main__":
    raise SystemExit(main())
