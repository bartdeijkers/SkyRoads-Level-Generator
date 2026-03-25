#!/usr/bin/env python3

from __future__ import annotations

import argparse
import fcntl
import json
import os
import platform
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
class StageSpec:
    name: str
    resume_command: str
    repeat_count: int = 1
    bios_keys: tuple[str, ...] = ()
    capture_screen: bool = False
    timeout_seconds: float | None = None


@dataclass(frozen=True)
class OraclePreset:
    name: str
    description: str
    breakpoints: tuple[BreakpointSpec, ...]
    dumps: tuple[DumpSpec, ...]
    bios_keys: tuple[str, ...] = ()
    auto_keys: tuple[KeyEvent, ...] = ()
    warmup_vrt_count: int = 0
    stages: tuple[StageSpec, ...] = ()


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
    warmup_vrt_count=1,
    stages=(
        StageSpec("after_first_space", "vrt", repeat_count=4, capture_screen=True),
        StageSpec("queue_second_space", "vrt", bios_keys=("space",), repeat_count=4, capture_screen=True),
        StageSpec("wait_for_menu", "vrt", repeat_count=40, capture_screen=False),
        StageSpec("queue_start_press", "vrt", bios_keys=("return",), repeat_count=6, capture_screen=True),
        StageSpec("queue_confirm_press", "f5", bios_keys=("return",), capture_screen=False, timeout_seconds=60.0),
    ),
    auto_keys=(
        KeyEvent(6.0, "space"),
        KeyEvent(1.0, "space"),
        KeyEvent(1.0, "space"),
        KeyEvent(1.0, "return"),
        KeyEvent(0.75, "return"),
    ),
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
}


class DosboxDebuggerSession:
    def __init__(
        self,
        source_root: Path,
        output_root: Path,
        dosbox: Path,
        time_limit: int,
        cycles: str,
    ) -> None:
        self.source_root = source_root
        self.output_root = output_root
        self.dosbox = dosbox
        self.time_limit = time_limit
        self.cycles = cycles
        self.raw_log_path = output_root / "oracle.log"
        self.process: subprocess.Popen[bytes] | None = None
        self.master_fd: int | None = None
        self.log_bytes = bytearray()
        self.log_handle = None

    def start(self) -> None:
        command = [
            str(self.dosbox),
            "-defaultconf",
            "-fastlaunch",
            "-debug",
            "-break-start",
            "-time-limit",
            str(self.time_limit),
            "-c",
            f"cycles {self.cycles}",
            "-c",
            f"mount c {self.source_root} -nocachedir",
            "-c",
            "c:",
            "-c",
            "skyroads.exe",
        ]
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
    return result


def build_markdown(summary: dict[str, Any]) -> str:
    lines = [
        "# SkyRoads DOS Oracle Capture",
        "",
        f"- Preset: `{summary['preset']}`",
        f"- Source root: `{summary['source_root']}`",
        f"- DOSBox-X: `{summary['dosbox']}`",
        f"- Key backend: `{summary['key_backend']}`",
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
    lines.extend(["", "## Checkpoints", ""])
    for checkpoint in summary["checkpoints"]:
        registers = checkpoint["registers"]
        lines.append(
            f"- `{checkpoint['checkpoint_name']}`: hit `{checkpoint['hit_index']}`, "
            f"`CS:IP={registers['cs']:04X}:{registers['ip']:04X}`, "
            f"`DS={registers['ds']:04X}`, `SS={registers['ss']:04X}`"
        )
        for dump in checkpoint["dumps"]:
            if dump["name"] == "renderer_state" and "current_row" in dump:
                lines.append(
                    f"  - renderer state: current_row `{dump['current_row']}`, "
                    f"group `{dump['road_row_group']}`, slot `{dump['trekdat_slot']}`"
                )
            elif dump["name"] == "trekdat_segment_table":
                segments = ", ".join(f"{value:04X}" for value in dump["segments"])
                lines.append(f"  - TREKDAT segments: `{segments}`")
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
        help="Disable the preset's built-in host key sequence.",
    )
    parser.add_argument(
        "--no-stages",
        action="store_true",
        help="Disable any preset-defined staged debugger/input sequence.",
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
    key_backend = detect_key_backend()
    screenshot_backend = detect_screenshot_backend()

    session = DosboxDebuggerSession(source_root, output_root, dosbox, args.time_limit, args.cycles)
    notes: list[str] = []
    checkpoints: list[dict[str, Any]] = []
    stage_screenshots: list[dict[str, Any]] = []
    auto_keys = tuple() if args.no_auto_keys or key_backend is None else preset.auto_keys
    bios_keys = tuple() if args.no_bios_keys else preset.bios_keys
    warmup_vrt_count = preset.warmup_vrt_count if args.warmup_vrt_count is None else args.warmup_vrt_count
    stages = tuple() if args.no_stages else (
        preset.stages if not args.no_bios_keys else tuple(stage for stage in preset.stages if not stage.bios_keys)
    )
    key_thread: HostKeySequence | None = None
    prompt_count = 0
    run_error: Exception | None = None
    failure_screenshot_path: str | None = None
    startup_registers: dict[str, int] | None = None
    breakpoints: list[BreakpointSpec] = []
    breakpoints_resolved = False

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
        else:
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
        for stage in stages:
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

        if args.resume_command == "f5":
            session.resume()
        else:
            session.send_line("VRT")
        _, prompt_count = session.wait_for_prompt(prompt_count, args.checkpoint_timeout)

        if key_thread is not None:
            key_thread.stop()
            key_thread.join(timeout=1)

        registers = capture_register_snapshot(session)
        checkpoint_name = infer_breakpoint_name(registers, breakpoints)
        checkpoint_dir = output_root / checkpoint_name
        checkpoint_dir.mkdir(parents=True, exist_ok=True)
        dump_results = [capture_dump_file(session, dump, checkpoint_dir) for dump in dumps]
        screenshot_path = None
        if args.capture_screen:
            try:
                screenshot_path = capture_screenshot(checkpoint_dir, checkpoint_name)
            except Exception as exc:  # pragma: no cover - host integration
                notes.append(f"Host screenshot capture failed: {exc}")

        checkpoints.append(
            {
                "hit_index": 1,
                "checkpoint_name": checkpoint_name,
                "registers": registers,
                "dumps": dump_results,
                "screenshot": screenshot_path,
            }
        )
    except Exception as exc:
        run_error = exc
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

    summary = {
        "preset": preset.name,
        "source_root": str(source_root),
        "output_root": str(output_root),
        "dosbox": str(dosbox),
        "key_backend": key_backend or "none",
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
        "resume_command": args.resume_command,
        "bios_keys": list(bios_keys),
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
        "dumps": [
            {
                "name": item.name,
                "address": item.debugger_address(),
                "length": item.length,
            }
            for item in dumps
        ],
        "checkpoints": checkpoints,
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
