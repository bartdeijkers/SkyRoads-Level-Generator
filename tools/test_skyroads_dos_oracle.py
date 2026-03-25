import json
import tempfile
import unittest
from pathlib import Path

from tools.skyroads_dos_oracle import (
    VGA_FRAME_DUMP_NAME,
    build_derived_dump_specs,
    fixture_bundle_from_checkpoint,
    sanitize_path_component,
)


class SkyroadsDosOracleTests(unittest.TestCase):
    def test_sanitize_path_component_keeps_fixture_paths_stable(self) -> None:
        self.assertEqual(sanitize_path_component(" frame 00 / renderer entry "), "frame-00-renderer-entry")
        self.assertEqual(sanitize_path_component(".."), "checkpoint")

    def test_build_derived_dump_specs_uses_renderer_state_and_active_slot(self) -> None:
        dump_results = [
            {
                "name": "renderer_state",
                "road_row_group": 3,
                "trekdat_slot": 2,
            },
            {
                "name": "trekdat_segment_table",
                "segments": [0x4000, 0x4001, 0x4ABC, 0x4003],
            },
        ]

        derived = build_derived_dump_specs(dump_results)

        self.assertEqual(len(derived), 2)
        self.assertEqual(derived[0].name, "active_road_window")
        self.assertEqual(derived[0].segment, "DS")
        self.assertEqual(derived[0].offset, "16C4")
        self.assertEqual(derived[0].length, 0x0E)
        self.assertEqual(derived[1].name, "active_trekdat_pointer_grid")
        self.assertEqual(derived[1].segment, "4ABC")
        self.assertEqual(derived[1].offset, "0000")
        self.assertEqual(derived[1].length, 0x0270)

    def test_fixture_bundle_promotes_vga_sha_to_frame_hash(self) -> None:
        checkpoint = {
            "checkpoint_name": "frame_00",
            "breakpoint_name": "renderer_entry",
            "hit_index": 1,
            "registers": {"cs": 0x0824, "ip": 0x2D03, "ds": 0x1000, "ss": 0x2000},
            "dumps": [
                {
                    "name": "renderer_state",
                    "address": "1000:0E36",
                    "length": 0x20,
                    "sha256": "a" * 64,
                    "current_row": 24,
                    "road_row_group": 3,
                    "trekdat_slot": 0,
                },
                {
                    "name": VGA_FRAME_DUMP_NAME,
                    "address": "A000:0000",
                    "length": 320 * 200,
                    "sha256": "b" * 64,
                    "width": 320,
                    "height": 200,
                    "row_stride": 320,
                },
            ],
        }

        with tempfile.TemporaryDirectory() as temp_dir:
            fixture = fixture_bundle_from_checkpoint(
                Path(temp_dir),
                "road0-initial-frame",
                checkpoint,
            )

            self.assertEqual(fixture["checkpoint_name"], "frame_00")
            self.assertEqual(fixture["frame_sha256"], "b" * 64)

            fixture_path = Path(fixture["fixture_path"])
            self.assertTrue(fixture_path.exists())
            payload = json.loads(fixture_path.read_text(encoding="ascii"))
            self.assertEqual(payload["bundle_version"], 1)
            self.assertEqual(payload["preset"], "road0-initial-frame")
            self.assertEqual(payload["frame_sha256"], "b" * 64)
            self.assertEqual(payload["dumps"][1]["name"], VGA_FRAME_DUMP_NAME)


if __name__ == "__main__":
    unittest.main()
