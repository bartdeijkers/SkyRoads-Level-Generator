"""Assemble an explicit, game-data-free payload and test the unpacked archive."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import zipfile


ROOT = Path(__file__).resolve().parents[1]
TARGETS = {
    "windows-x64": ("x86_64-pc-windows-msvc", "skyroads-sdl.exe", "README-WINDOWS.txt"),
    "linux-x64": ("x86_64-unknown-linux-gnu", "skyroads-sdl", "README-LINUX.txt"),
    "linux-arm64": ("aarch64-unknown-linux-gnu", "skyroads-sdl", "README-LINUX.txt"),
}
GAME_FILES = (
    "ANIM.LZS CARS.LZS DASHBRD.LZS DEMO.REC FUL_DISP.DAT GOMENU.LZS "
    "HELPMENU.LZS INTRO.LZS INTRO.SND MAINMENU.LZS MUZAX.LZS OXY_DISP.DAT "
    "ROADS.LZS SETMENU.LZS SFX.SND SPEED.DAT TREKDAT.LZS "
    "WORLD0.LZS WORLD1.LZS WORLD2.LZS WORLD3.LZS WORLD4.LZS WORLD5.LZS "
    "WORLD6.LZS WORLD7.LZS WORLD8.LZS WORLD9.LZS"
).split()


def cargo_metadata():
    output = subprocess.check_output(
        ["cargo", "metadata", "--locked", "--offline", "--format-version", "1"], cwd=ROOT
    )
    return json.loads(output)


def package_version(metadata):
    version = next(p["version"] for p in metadata["packages"] if p["name"] == "skyroads-sdl")
    ref = os.environ.get("GITHUB_REF", "")
    if ref.startswith("refs/tags/") and ref != f"refs/tags/v{version}":
        raise ValueError(f"Release tag must match Cargo version: v{version}")
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?", version):
        raise ValueError(f"Unsupported release version: {version}")
    return version


def dependency_notices(metadata):
    sections = ["Third-party Rust dependency notices\n"]
    # Include the whole locked graph, including build dependencies, so feature
    # changes cannot silently remove an attribution required by the binary.
    for package in sorted(metadata["packages"], key=lambda p: p["name"]):
        if package["source"] is None:
            continue
        directory = Path(package["manifest_path"]).parent
        notices = sorted(
            path for path in directory.iterdir()
            if path.is_file() and path.name.lower().startswith(("license", "copying", "notice"))
        )
        if not notices:
            raise ValueError(f"Missing license text for {package['name']}")
        sections.append(f"\n{'=' * 72}\n{package['name']} {package['version']}\n"
                        f"License: {package['license']}\nSource: {package['source']}\n")
        for notice in notices:
            sections.append(f"\n--- {notice.name} ---\n{notice.read_text(encoding='utf-8')}\n")
    return "".join(sections)


def check_binary(path, platform):
    data = path.read_bytes()
    if platform == "windows-x64":
        if data[:2] != b"MZ" or len(data) < 64:
            raise ValueError(f"Not a Windows executable: {path}")
        pe = int.from_bytes(data[60:64], "little")
        if data[pe:pe + 6] != b"PE\0\0\x64\x86":
            raise ValueError(f"Not a Windows AMD64 binary: {path}")
        return
    machine = 62 if platform == "linux-x64" else 183
    if data[:6] != b"\x7fELF\x02\x01" or int.from_bytes(data[18:20], "little") != machine:
        raise ValueError(f"Wrong ELF architecture for {platform}: {path}")


def verify_archive(archive, expected):
    """Check exact membership and bytes before any extraction or execution."""
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as handle:
            entries = handle.infolist()
            names = [entry.filename for entry in entries]
            actual = {name: handle.read(name) for name in names}
    else:
        with tarfile.open(archive, "r:gz") as handle:
            entries = handle.getmembers()
            if any(not entry.isfile() for entry in entries):
                raise ValueError("Archive contains non-regular files")
            names = [entry.name for entry in entries]
            actual = {entry.name: handle.extractfile(entry).read() for entry in entries}
            for entry in entries:
                if entry.name.endswith("/skyroads-sdl") and entry.mode & 0o111 != 0o111:
                    raise ValueError("Linux executable lost executable permissions")
    if len(names) != len(set(names)) or actual != expected:
        raise ValueError("Archive payload differs from the explicit package inventory")


def smoke_archive(archive, expected, executable, source_root):
    verify_archive(archive, expected)
    with tempfile.TemporaryDirectory(prefix="skyroads-smoke-") as temporary:
        directory = Path(temporary)
        # Write only the verified payload; never extract arbitrary archive paths.
        for name, contents in expected.items():
            destination = directory / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(contents)
        package_directory = next(iter(expected)).split("/", 1)[0]
        binary = directory / package_directory / executable
        binary.chmod(0o755)
        assets = directory / "user-game-data"
        assets.mkdir()
        for name in GAME_FILES:
            shutil.copyfile(source_root / name, assets / name)
        environment = {**os.environ, "SDL_VIDEODRIVER": "dummy", "SDL_AUDIODRIVER": "dummy"}
        for mode in ("gameplay", "procedural", "gamepad"):
            subprocess.run([str(binary), f"--smoke-{mode}", str(assets)],
                           cwd=binary.parent, env=environment, check=True, timeout=120)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=TARGETS, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--sdl-directory", type=Path)
    parser.add_argument("--output", type=Path, default=ROOT / "target" / "packages")
    parser.add_argument("--source-root", type=Path, default=ROOT)
    args = parser.parse_args()
    metadata = cargo_metadata()
    version = package_version(metadata)
    target, executable, readme = TARGETS[args.platform]
    check_binary(args.binary, args.platform)
    name = f"SkyRoads-Rust-{version}-{args.platform}"
    payload = {
        executable: args.binary.read_bytes(),
        "README.md": (ROOT / "README.md").read_bytes(),
        readme: (ROOT / "packaging" / readme).read_bytes(),
        "GAME-DATA.txt": (ROOT / "packaging" / "GAME-DATA.txt").read_bytes(),
        "THIRD-PARTY-NOTICES.txt": dependency_notices(metadata).encode(),
    }
    if args.platform == "windows-x64":
        if args.sdl_directory is None:
            parser.error("Windows packaging requires --sdl-directory (SDL2 VC development package)")
        dll = args.sdl_directory / "lib" / "x64" / "SDL2.dll"
        check_binary(dll, args.platform)
        payload["SDL2.dll"] = dll.read_bytes()
        payload["SDL2-LICENSE.txt"] = (args.sdl_directory / "LICENSE.txt").read_bytes()
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    info = {"version": version, "target": target, "commit": commit,
            "game_data_included": False,
            "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip()}
    payload["BUILD-INFO.json"] = (json.dumps(info, indent=2) + "\n").encode()
    expected = {f"{name}/{filename}": data for filename, data in payload.items()}
    args.output.mkdir(parents=True, exist_ok=True)
    suffix = ".zip" if args.platform == "windows-x64" else ".tar.gz"
    archive = args.output / (name + suffix)
    with tempfile.TemporaryDirectory(prefix="skyroads-package-") as temporary:
        stage = Path(temporary) / name
        stage.mkdir()
        for filename, data in payload.items():
            (stage / filename).write_bytes(data)
        (stage / executable).chmod(0o755)
        if suffix == ".zip":
            with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as handle:
                for filename in sorted(payload):
                    handle.write(stage / filename, f"{name}/{filename}")
        else:
            with tarfile.open(archive, "w:gz") as handle:
                for filename in sorted(payload):
                    handle.add(stage / filename, f"{name}/{filename}")
    smoke_archive(archive, expected, executable, args.source_root.resolve())
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_name(archive.name + ".sha256").write_text(
        f"{digest}  {archive.name}\n", encoding="utf-8"
    )
    print(f"Verified {archive}: {digest}")


if __name__ == "__main__":
    main()
