#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
from collections import OrderedDict
from pathlib import Path


DEFAULT_DOSBOX = Path("/Applications/dosbox-x.app/Contents/MacOS/dosbox-x")

OPEN_RE = re.compile(r"LOG:\s+(\d+)\s+FILES:file open command (\d+) file (.+)")
READ_RE = re.compile(r"LOG:\s+(\d+)\s+DEBUG FILES:Reading (\d+) bytes from (.+)")
CLOSE_RE = re.compile(r"LOG:\s+(\d+)\s+FILES:Closing file (.+)")


def normalize_name(name: str) -> str:
    return name.strip().upper()


def build_markdown(summary: dict[str, object]) -> str:
    startup_order = summary["startup_order"]
    file_stats = summary["files"]

    lines = [
        "# SkyRoads DOS Startup Trace",
        "",
        f"- Source root: `{summary['source_root']}`",
        f"- DOSBox-X: `{summary['dosbox']}`",
        f"- Time limit: `{summary['time_limit_seconds']}` seconds",
        "",
        "## Startup File Order",
        "",
    ]
    for index, name in enumerate(startup_order, start=1):
        stats = file_stats[name]
        lines.append(
            f"{index}. `{name}`: opened at log line {stats['open_line']}, "
            f"{stats['read_count']} reads, {stats['total_bytes_read']} bytes total"
        )
    lines.extend(
        [
            "",
            "## Per-File Read Sizes",
            "",
        ]
    )
    for name in startup_order:
        stats = file_stats[name]
        read_sizes = ", ".join(str(size) for size in stats["read_sizes"]) or "none"
        lines.append(
            f"- `{name}`: reads [{read_sizes}]"
        )
    return "\n".join(lines) + "\n"


def parse_trace(log_path: Path) -> dict[str, object]:
    lines = log_path.read_text(encoding="utf-8", errors="replace").splitlines()
    files: OrderedDict[str, dict[str, object]] = OrderedDict()

    for line_number, line in enumerate(lines, start=1):
        if match := OPEN_RE.search(line):
            _, open_command, raw_name = match.groups()
            if int(open_command) != 0:
                continue
            key = normalize_name(raw_name)
            entry = files.setdefault(
                key,
                {
                    "name": raw_name.strip(),
                    "open_command": int(open_command),
                    "open_line": line_number,
                    "close_line": None,
                    "read_count": 0,
                    "total_bytes_read": 0,
                    "read_sizes": [],
                },
            )
            if entry["open_line"] is None:
                entry["open_line"] = line_number
        elif match := READ_RE.search(line):
            _, raw_size, raw_name = match.groups()
            key = normalize_name(raw_name)
            entry = files.setdefault(
                key,
                {
                    "name": raw_name.strip(),
                    "open_command": None,
                    "open_line": None,
                    "close_line": None,
                    "read_count": 0,
                    "total_bytes_read": 0,
                    "read_sizes": [],
                },
            )
            size = int(raw_size)
            entry["read_count"] = int(entry["read_count"]) + 1
            entry["total_bytes_read"] = int(entry["total_bytes_read"]) + size
            cast_sizes = entry["read_sizes"]
            assert isinstance(cast_sizes, list)
            cast_sizes.append(size)
        elif match := CLOSE_RE.search(line):
            _, raw_name = match.groups()
            key = normalize_name(raw_name)
            entry = files.get(key)
            if entry is not None and entry["close_line"] is None:
                entry["close_line"] = line_number

    all_open_order = [name for name, entry in files.items() if entry["open_line"] is not None]
    startup_order = [name for name in all_open_order if not name.startswith("Z:\\")]
    return {
        "log_line_count": len(lines),
        "all_open_order": all_open_order,
        "startup_order": startup_order,
        "files": files,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Run SkyRoads under DOSBox-X and capture startup I/O traces.")
    parser.add_argument("--source", type=Path, default=Path("."), help="Path containing SKYROADS.EXE and data files.")
    parser.add_argument("--output", type=Path, required=True, help="Directory to write the raw log and summaries.")
    parser.add_argument("--dosbox", type=Path, default=DEFAULT_DOSBOX, help="Path to the DOSBox-X binary.")
    parser.add_argument("--time-limit", type=int, default=10, help="DOSBox-X run time limit in seconds.")
    args = parser.parse_args()

    source_root = args.source.resolve()
    output_root = args.output.resolve()
    dosbox = args.dosbox.resolve()

    if not (source_root / "SKYROADS.EXE").exists():
        raise SystemExit(f"missing SKYROADS.EXE under {source_root}")
    if shutil.which("script") is None:
        raise SystemExit("missing required host tool: script")
    if not dosbox.exists():
        raise SystemExit(f"missing DOSBox-X binary: {dosbox}")

    output_root.mkdir(parents=True, exist_ok=True)
    raw_log_path = output_root / "startup.log"

    command = [
        "script",
        "-q",
        str(raw_log_path),
        str(dosbox),
        "-defaultconf",
        "-fastlaunch",
        "-silent",
        "-debug",
        "-log-fileio",
        "-log-int21",
        "-time-limit",
        str(args.time_limit),
        "-c",
        f"mount c {source_root} -nocachedir",
        "-c",
        "c:",
        "-c",
        "skyroads.exe",
    ]
    subprocess.run(command, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT)

    summary = parse_trace(raw_log_path)
    summary.update(
        {
            "source_root": str(source_root),
            "output_root": str(output_root),
            "dosbox": str(dosbox),
            "time_limit_seconds": args.time_limit,
            "command": command[2:],
        }
    )

    serializable = dict(summary)
    serializable["files"] = {name: data for name, data in summary["files"].items()}
    (output_root / "summary.json").write_text(json.dumps(serializable, indent=2, sort_keys=True) + "\n", encoding="ascii")
    (output_root / "summary.md").write_text(build_markdown(serializable), encoding="ascii")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
