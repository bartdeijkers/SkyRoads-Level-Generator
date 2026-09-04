"""Regressions for archive leakage, tampering, and incorrect target selection."""

import io
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import release


class ReleaseTests(unittest.TestCase):
    def test_tags_must_match_the_built_version(self):
        metadata = {"packages": [{"name": "skyroads-sdl", "version": "0.1.0"}]}
        with patch.dict(os.environ, {"GITHUB_REF": "refs/tags/v0.2.0"}):
            with self.assertRaisesRegex(ValueError, "must match"):
                release.package_version(metadata)
        with patch.dict(os.environ, {"GITHUB_REF": "refs/tags/v0.1.0"}):
            self.assertEqual(release.package_version(metadata), "0.1.0")

    def test_zip_rejects_original_assets_and_tampered_binaries(self):
        expected = {"package/skyroads-sdl.exe": b"native binary"}
        cases = [
            {**expected, "package/ROADS.LZS": b"original data"},
            {"package/skyroads-sdl.exe": b"replaced binary"},
            {**expected, "../escape": b"unexpected"},
        ]
        with tempfile.TemporaryDirectory() as temporary:
            archive = Path(temporary) / "release.zip"
            for payload in cases:
                with self.subTest(payload=payload):
                    with zipfile.ZipFile(archive, "w") as handle:
                        for name, data in payload.items():
                            handle.writestr(name, data)
                    with self.assertRaisesRegex(ValueError, "inventory"):
                        release.verify_archive(archive, expected)

    def test_linux_requires_executable_permissions_and_regular_files(self):
        name = "package/skyroads-sdl"
        expected = {name: b"native binary"}
        with tempfile.TemporaryDirectory() as temporary:
            archive = Path(temporary) / "release.tar.gz"
            for mode, kind in [(0o644, tarfile.REGTYPE), (0o755, tarfile.SYMTYPE)]:
                with tarfile.open(archive, "w:gz") as handle:
                    entry = tarfile.TarInfo(name)
                    entry.mode = mode
                    entry.type = kind
                    entry.size = len(expected[name])
                    handle.addfile(entry, io.BytesIO(expected[name]))
                with self.assertRaises(ValueError):
                    release.verify_archive(archive, expected)

    def test_linux_architecture_mismatch_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "skyroads-sdl"
            binary.write_bytes(b"\x7fELF\x02\x01" + bytes(12) + (62).to_bytes(2, "little"))
            release.check_binary(binary, "linux-x64")
            with self.assertRaisesRegex(ValueError, "architecture"):
                release.check_binary(binary, "linux-arm64")


if __name__ == "__main__":
    unittest.main()
