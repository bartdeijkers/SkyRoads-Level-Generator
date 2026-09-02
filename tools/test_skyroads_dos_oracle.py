import json
import tempfile
import unittest
from pathlib import Path

from tools.skyroads_dos_oracle import (
    MemoryWrite,
    PRESETS,
    ROAD0_LAUNCH_ADDKEY,
    SHIPPED_RENDERER_ENTRY_ADDRESS,
    VGA_FRAME_DUMP_NAME,
    VGA_PALETTE_DUMP_NAME,
    build_launch_plan,
    build_phase_file_trace,
    build_derived_dump_specs,
    build_dosbox_command,
    fixture_bundle_from_checkpoint,
    gameplay_launch_auto_keys,
    go_menu_navigation_keys,
    parse_file_trace,
    parse_vga_dac_palette,
    apply_memory_writes,
    sanitize_path_component,
    set_gameplay_key_state,
)


class SkyroadsDosOracleTests(unittest.TestCase):
    def test_vga_dac_palette_parser_recovers_all_rgb_components(self) -> None:
        lines = ["LOG: VGA DAC palette (RGB):"]
        expected = bytearray()
        for row_index in range(0, 256, 8):
            colors = []
            for color_offset in range(8):
                value = (row_index + color_offset) % 64
                expected.extend((value, value + 1, value + 2))
                separator = "-" if color_offset == 3 else " "
                colors.append(f"{value:02x}{value + 1:02x}{value + 2:02x}{separator}")
            lines.append(f"LOG: {row_index:02x}: " + "".join(colors))

        palette = parse_vga_dac_palette("\n".join(lines))

        self.assertEqual(palette, bytes(expected))

    def test_gameplay_key_state_writes_pressed_bits_and_releases_other_keys(self) -> None:
        class RecordingSession:
            def __init__(self) -> None:
                self.commands = []

            def send_line(self, command: str) -> None:
                self.commands.append(command)

        session = RecordingSession()

        set_gameplay_key_state(session, 0x0E92, ("up", "left", "space"))

        self.assertEqual(
            session.commands,
            ["SM 0E92:0BA2 80 00 80 00 00 00 00 00 00 80"],
        )

    def test_memory_writes_resolve_the_requested_runtime_segment(self) -> None:
        class RecordingSession:
            def __init__(self) -> None:
                self.commands = []

            def send_line(self, command: str) -> None:
                self.commands.append(command)

        session = RecordingSession()
        writes = (
            MemoryWrite(offset=0x9628, data=bytes.fromhex("00 80 36 00")),
            MemoryWrite(offset=0x044A, data=b"\x01\x02", segment_register="SS"),
        )

        apply_memory_writes(session, {"ds": 0x0E92, "ss": 0x1234}, writes)

        self.assertEqual(
            session.commands,
            [
                "SM 17F4:0008 00 80 36 00",
                "SM 1278:000A 01 02",
            ],
        )

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
            {
                "name": "ship_render_state",
                "active_sprite_offset": 0x7620,
                "active_sprite_segment": 0x567B,
            },
        ]

        derived = build_derived_dump_specs(dump_results)

        self.assertEqual(len(derived), 3)
        self.assertEqual(derived[0].name, "active_road_window")
        self.assertEqual(derived[0].segment, "DS")
        self.assertEqual(derived[0].offset, "16C4")
        self.assertEqual(derived[0].length, 0x0E)
        self.assertEqual(derived[1].name, "active_trekdat_pointer_grid")
        self.assertEqual(derived[1].segment, "4ABC")
        self.assertEqual(derived[1].offset, "0000")
        self.assertEqual(derived[1].length, 0x0270)
        self.assertEqual(derived[2].name, "active_ship_sprite")
        self.assertEqual(derived[2].segment, "567B")
        self.assertEqual(derived[2].offset, "7620")
        self.assertEqual(derived[2].length, 29 * 24)

    def test_fixture_bundle_copies_raw_vga_frame_and_palette(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_root = Path(temp_dir)
            capture_root = temp_root / "capture"
            capture_root.mkdir()
            frame_path = capture_root / "vga_frame.bin"
            palette_path = capture_root / "vga_palette_6bit.bin"
            frame_path.write_bytes(bytes([7]) * (320 * 200))
            palette_path.write_bytes(bytes(range(64)) * 12)
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
                        "path": str(frame_path),
                    },
                    {
                        "name": VGA_PALETTE_DUMP_NAME,
                        "address": "1000:5182",
                        "length": 256 * 3,
                        "sha256": "c" * 64,
                        "entry_count": 256,
                        "component_bits": 6,
                        "encoding": "rgb",
                        "path": str(palette_path),
                    },
                ],
            }
            fixture = fixture_bundle_from_checkpoint(
                temp_root / "fixtures",
                "road0-initial-frame",
                checkpoint,
            )

            self.assertEqual(fixture["checkpoint_name"], "frame_00")
            self.assertEqual(fixture["frame_sha256"], "b" * 64)

            fixture_path = Path(fixture["fixture_path"])
            self.assertTrue(fixture_path.exists())
            payload = json.loads(fixture_path.read_text(encoding="ascii"))
            self.assertEqual(payload["bundle_version"], 2)
            self.assertEqual(payload["preset"], "road0-initial-frame")
            self.assertEqual(payload["frame_sha256"], "b" * 64)
            self.assertEqual(payload["dumps"][1]["name"], VGA_FRAME_DUMP_NAME)
            self.assertEqual(payload["frame_file"], "frame.indices")
            self.assertEqual(payload["palette_file"], "palette.vga6")
            self.assertEqual(
                (fixture_path.parent / payload["frame_file"]).read_bytes(),
                frame_path.read_bytes(),
            )
            self.assertEqual(
                (fixture_path.parent / payload["palette_file"]).read_bytes(),
                palette_path.read_bytes(),
            )

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

    def test_go_menu_navigation_reaches_dispatch_inventory_roads(self) -> None:
        self.assertEqual(go_menu_navigation_keys(1), ())
        self.assertEqual(go_menu_navigation_keys(5), ("down",) * 4)
        self.assertEqual(go_menu_navigation_keys(9), ("down",) * 8)
        self.assertEqual(
            go_menu_navigation_keys(20),
            ("right", "down", "down", "down", "down"),
        )
        self.assertEqual(
            go_menu_navigation_keys(26),
            ("right",) + ("down",) * 10,
        )

    def test_dispatch_preset_launches_select_the_requested_road(self) -> None:
        road20 = PRESETS["road20-dispatch-kind1"]
        road5 = PRESETS["road5-dispatch-kinds2-4"]
        road9 = PRESETS["road9-dispatch-kind5"]
        road26 = PRESETS["road26-dispatch-kind3"]

        self.assertEqual(
            road20.guest_launch_sequence,
            type(road20.guest_launch_sequence)(gameplay_launch_auto_keys(20)),
        )
        self.assertEqual(
            road5.guest_launch_sequence,
            type(road5.guest_launch_sequence)(gameplay_launch_auto_keys(5)),
        )
        self.assertEqual(
            road9.guest_launch_sequence,
            type(road9.guest_launch_sequence)(gameplay_launch_auto_keys(9)),
        )
        self.assertEqual(
            road26.guest_launch_sequence,
            type(road26.guest_launch_sequence)(gameplay_launch_auto_keys(26)),
        )
        self.assertTrue(
            all(len(preset.checkpoints[0].frame_bios_keys) == 8 for preset in (road20, road5, road9))
        )
        self.assertEqual(len(road26.checkpoints[0].frame_bios_keys), 64)
        self.assertTrue(
            all(keys == ("up",) for keys in road26.checkpoints[0].frame_bios_keys)
        )
        self.assertEqual(
            road20.guest_launch_sequence.shell_commands()[-1],
            "ADDKEY p15750 right down down down down enter",
        )
        self.assertEqual(
            road5.guest_launch_sequence.shell_commands()[-1],
            "ADDKEY p15750 down down down down enter",
        )
        self.assertEqual(
            road9.guest_launch_sequence.shell_commands()[-1],
            "ADDKEY p15750 down down down down down down down down enter",
        )
        self.assertEqual(
            road26.guest_launch_sequence.shell_commands()[-1],
            "ADDKEY p15750 right down down down down down down down down down down enter",
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

    def test_gameplay_breakpoint_is_installed_before_exe_startup(self) -> None:
        breakpoint = PRESETS["road0-initial-frame"].breakpoints[0]

        self.assertEqual(breakpoint.address, SHIPPED_RENDERER_ENTRY_ADDRESS)
        self.assertEqual(breakpoint.image_offset, 0x2D03)

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
        self.assertEqual(len(airborne.frame_bios_keys), 10)
        self.assertTrue(all(keys == ("up",) for keys in airborne.frame_bios_keys[:8]))
        self.assertEqual(airborne.frame_bios_keys[8], ("up", "space"))
        self.assertEqual(airborne.frame_bios_keys[9], ("up",))
        self.assertEqual(
            len(PRESETS["road1-shadow-variant3"].checkpoints[0].frame_bios_keys),
            11,
        )
        self.assertEqual(
            len(PRESETS["road1-shadow-variant4"].checkpoints[0].frame_bios_keys),
            12,
        )
        self.assertEqual(
            len(PRESETS["road26-shadow-variant2"].checkpoints[0].frame_bios_keys),
            11,
        )
        self.assertEqual(
            PRESETS["road26-shadow-variant2"]
            .checkpoints[0]
            .framebuffer_speed_visible_count,
            1,
        )
        self.assertTrue(
            any(
                dump.name == "sprite_cycle_counter"
                and dump.debugger_address() == "DS:160C"
                for dump in PRESETS["road0-steady-neutral"].dumps
            )
        )

        terminal_scan = PRESETS["road2-terminal-scan"]
        self.assertEqual(len(terminal_scan.checkpoints), 21)
        self.assertEqual(len(terminal_scan.checkpoints[0].frame_bios_keys), 30)
        self.assertTrue(
            all(
                checkpoint.frame_bios_keys == (("up",),)
                for checkpoint in terminal_scan.checkpoints[1:]
            )
        )
        explosion_scan = PRESETS["road30-explosion-scan"]
        self.assertEqual(len(explosion_scan.checkpoints), 26)
        self.assertEqual(len(explosion_scan.checkpoints[0].frame_bios_keys), 40)
        self.assertTrue(
            any(
                dump.name == "dashboard_speed_visible_count"
                and dump.debugger_address() == "DS:41CA"
                for dump in PRESETS["road0-steady-neutral"].dumps
            )
        )

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

    def test_gomenu_selection_presets_stop_before_gameplay_and_encode_arrow_keys(self) -> None:
        default = PRESETS["gomenu-default-selection"]
        right = PRESETS["gomenu-right-selection"]
        down = PRESETS["gomenu-down-selection"]

        self.assertEqual(default.breakpoints, ())
        self.assertEqual(default.dumps, ())
        self.assertEqual(default.checkpoints[0].resume_command, "vrt")
        self.assertEqual(default.checkpoints[0].bios_keys, ())
        self.assertEqual(right.checkpoints[0].bios_keys, ("right",))
        self.assertEqual(down.checkpoints[0].bios_keys, ("down",))

        plan = build_launch_plan(
            right,
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
