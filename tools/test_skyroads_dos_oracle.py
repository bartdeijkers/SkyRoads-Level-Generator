import json
import tempfile
import unittest
from pathlib import Path

from tools.skyroads_dos_oracle import (
    PRESETS,
    ROAD0_LAUNCH_ADDKEY,
    VGA_FRAME_DUMP_NAME,
    build_launch_plan,
    build_phase_file_trace,
    build_derived_dump_specs,
    build_dosbox_command,
    fixture_bundle_from_checkpoint,
    parse_file_trace,
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

    def test_build_dosbox_command_inserts_pre_launch_commands_before_game(self) -> None:
        command = build_dosbox_command(
            Path("/usr/bin/dosbox-x"),
            Path("/tmp/source-root"),
            30,
            "max",
            ROAD0_LAUNCH_ADDKEY.shell_commands(),
        )

        self.assertEqual(command[0], "/usr/bin/dosbox-x")
        c_positions = [index for index, value in enumerate(command) if value == "-c"]
        shell_commands = [command[index + 1] for index in c_positions]
        self.assertEqual(
            shell_commands,
            [
                "cycles max",
                "mount c /tmp/source-root -nocachedir",
                "c:",
                "ADDKEY p6000 space",
                "ADDKEY p7000 space",
                "ADDKEY p8000 space",
                "ADDKEY p9000 enter",
                "ADDKEY p9750 enter",
                "ADDKEY p15750 enter",
                "skyroads.exe",
            ],
        )

    def test_road0_launch_addkey_commands_use_dosbox_button_names(self) -> None:
        self.assertEqual(
            ROAD0_LAUNCH_ADDKEY.shell_commands(),
            (
                "ADDKEY p6000 space",
                "ADDKEY p7000 space",
                "ADDKEY p8000 space",
                "ADDKEY p9000 enter",
                "ADDKEY p9750 enter",
                "ADDKEY p15750 enter",
            ),
        )

    def test_guest_addkey_launch_plan_skips_stages_for_gameplay_presets(self) -> None:
        plan = build_launch_plan(
            PRESETS["road0-initial-frame"],
            key_backend="powershell",
            no_auto_keys=False,
            no_bios_keys=False,
            no_stages=False,
        )

        self.assertEqual(plan.launch_input_backend, "guest-addkey")
        self.assertEqual(plan.bios_keys, ())
        self.assertEqual(plan.stages, ())
        self.assertEqual(plan.pre_launch_commands[-1], "ADDKEY p15750 enter")

    def test_gameplay_scenario_presets_encode_expected_frame_inputs(self) -> None:
        neutral = PRESETS["road0-steady-neutral"].checkpoints[0]
        throttle = PRESETS["road0-sustained-throttle"].checkpoints[0]
        left = PRESETS["road0-steady-left"].checkpoints[0]
        right = PRESETS["road0-steady-right"].checkpoints[0]
        airborne = PRESETS["road0-first-airborne"].checkpoints[0]

        self.assertIsNotNone(PRESETS["road0-steady-neutral"].guest_launch_sequence)
        self.assertEqual(len(neutral.frame_bios_keys), 8)
        self.assertTrue(all(keys == () for keys in neutral.frame_bios_keys))
        self.assertEqual(len(throttle.frame_bios_keys), 24)
        self.assertTrue(all(keys == ("up",) for keys in throttle.frame_bios_keys))
        self.assertTrue(all(keys == ("up", "left") for keys in left.frame_bios_keys))
        self.assertTrue(all(keys == ("up", "right") for keys in right.frame_bios_keys))
        self.assertEqual(len(airborne.frame_bios_keys), 9)
        self.assertTrue(all(keys == ("up",) for keys in airborne.frame_bios_keys[:8]))
        self.assertEqual(airborne.frame_bios_keys[8], ("up", "space"))

    def test_post_confirm_vrt_scan_preset_uses_vrt_checkpoints_without_breakpoints(self) -> None:
        preset = PRESETS["road0-post-confirm-vrt-scan"]

        self.assertEqual(preset.breakpoints, ())
        self.assertEqual(preset.dumps, ())
        self.assertEqual(len(preset.checkpoints), 16)
        self.assertEqual(preset.checkpoints[0].resume_command, "vrt")
        self.assertEqual(preset.checkpoints[0].bios_keys, ("return",))
        self.assertTrue(all(checkpoint.resume_command == "vrt" for checkpoint in preset.checkpoints))
        self.assertTrue(all(checkpoint.bios_keys == () for checkpoint in preset.checkpoints[1:]))

        plan = build_launch_plan(
            preset,
            key_backend="powershell",
            no_auto_keys=False,
            no_bios_keys=False,
            no_stages=False,
        )
        self.assertEqual(plan.launch_input_backend, "guest-addkey")
        self.assertEqual(plan.bios_keys, ("space",))
        self.assertEqual(
            [stage.name for stage in plan.stages],
            ["after_first_space", "queue_second_space", "wait_for_menu", "queue_start_press"],
        )

    def test_parse_file_trace_keeps_relevant_startup_sequence_and_duplicate_open_events(self) -> None:
        raw_log = "\n".join(
            [
                "LOG:         72       FILES:file open command 2 file CON",
                "LOG:         92       FILES:Special file open command 80 file Z:\\AUTOEXEC.BAT",
                "LOG:       5014       FILES:file open command 0 file skyroads.exe",
                "LOG:      22114       FILES:file open command 0 file skyroads.cfg",
                "LOG:      24936       FILES:file open command 0 file muzax.lzs",
                "LOG:      25000       DEBUG FILES:Reading 128 bytes from muzax.lzs",
                "LOG:      26000       FILES:Closing file muzax.lzs",
                "LOG:   18114545       FILES:file open command 0 file mainmenu.lzs",
                "LOG:   18308016       FILES:file open command 0 file intro.lzs",
                "LOG:   19000000       FILES:file open command 0 file intro.lzs",
                "",
            ]
        )

        with tempfile.TemporaryDirectory() as temp_dir:
            log_path = Path(temp_dir) / "oracle.log"
            log_path.write_text(raw_log, encoding="utf-8")

            file_trace = parse_file_trace(log_path)

        assert file_trace is not None
        self.assertEqual(
            [item["name"] for item in file_trace["startup_sequence"]],
            ["skyroads.exe", "skyroads.cfg", "muzax.lzs", "mainmenu.lzs", "intro.lzs"],
        )
        self.assertEqual(
            [item["name"] for item in file_trace["startup_open_events"]],
            [
                "skyroads.exe",
                "skyroads.cfg",
                "muzax.lzs",
                "mainmenu.lzs",
                "intro.lzs",
                "intro.lzs",
            ],
        )
        self.assertEqual(file_trace["files"]["INTRO.LZS"]["open_count"], 2)
        self.assertEqual(file_trace["files"]["MUZAX.LZS"]["read_count"], 1)
        self.assertEqual(file_trace["files"]["MUZAX.LZS"]["total_bytes_read"], 128)

    def test_build_phase_file_trace_assigns_new_file_opens_to_failed_stage(self) -> None:
        file_trace = {
            "startup_open_events": [
                {"name": "skyroads.exe", "normalized_name": "SKYROADS.EXE", "open_line": 10, "open_tick": 1000},
                {"name": "mainmenu.lzs", "normalized_name": "MAINMENU.LZS", "open_line": 20, "open_tick": 2000},
                {"name": "cars.lzs", "normalized_name": "CARS.LZS", "open_line": 30, "open_tick": 3000},
                {"name": "gomenu.lzs", "normalized_name": "GOMENU.LZS", "open_line": 35, "open_tick": 3500},
            ]
        }
        phase_markers = [
            {"name": "wait_for_menu", "kind": "stage", "status": "completed", "log_line_count": 22, "log_byte_count": 220},
            {"name": "queue_confirm_press", "kind": "stage", "status": "failed", "log_line_count": 40, "log_byte_count": 400},
        ]

        phase_summaries = build_phase_file_trace(file_trace, phase_markers)

        self.assertEqual(len(phase_summaries), 2)
        self.assertEqual(phase_summaries[0]["opened_files"], ["skyroads.exe", "mainmenu.lzs"])
        self.assertEqual(phase_summaries[1]["opened_files"], ["cars.lzs", "gomenu.lzs"])
        self.assertEqual(phase_summaries[1]["status"], "failed")


if __name__ == "__main__":
    unittest.main()
