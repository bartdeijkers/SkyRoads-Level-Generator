#!/usr/bin/env python3
"""Extract documented SkyRoads resource formats into portable debug assets."""

from __future__ import annotations

import argparse
import json
import struct
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


@dataclass
class PaletteChunk:
    offset: int
    count: int
    colors_vga: bytes
    aux_data: bytes

    def colors_rgb888(self) -> list[tuple[int, int, int]]:
        rgb = []
        for index in range(0, len(self.colors_vga), 3):
            r, g, b = self.colors_vga[index : index + 3]
            rgb.append(tuple((channel * 255) // 63 for channel in (r, g, b)))
        return rgb


@dataclass
class ImageFrame:
    offset: int
    width: int
    height: int
    unknown: int
    widths: tuple[int, int, int]
    palette_index: int
    pixels: bytes
    next_offset: int


class BitReader:
    def __init__(self, data: bytes, offset: int) -> None:
        self._data = data
        self.byte_offset = offset
        self.bit_offset = 0

    def read_bits(self, count: int) -> int:
        value = 0
        for _ in range(count):
            if self.byte_offset >= len(self._data):
                raise EOFError("unexpected end of compressed stream")
            bit = (self._data[self.byte_offset] >> (7 - self.bit_offset)) & 1
            value = (value << 1) | bit
            self.bit_offset += 1
            if self.bit_offset == 8:
                self.bit_offset = 0
                self.byte_offset += 1
        return value

    def bytes_consumed(self, start_offset: int) -> int:
        extra = 1 if self.bit_offset else 0
        return (self.byte_offset + extra) - start_offset


def copy_from_history(
    output: bytearray,
    distance: int,
    count: int,
    limit: int,
) -> None:
    if distance <= 0 or distance > len(output):
        raise ValueError(f"invalid back-reference distance {distance}")
    for _ in range(count):
        if len(output) >= limit:
            break
        output.append(output[-distance])


def decompress_stream(
    data: bytes,
    offset: int,
    expected_size: int | None,
    widths: tuple[int, int, int],
) -> tuple[bytes, int]:
    width1, width2, width3 = widths
    reader = BitReader(data, offset)
    output = bytearray()

    try:
        while expected_size is None or len(output) < expected_size:
            prefix = reader.read_bits(1)
            if prefix == 0:
                distance = reader.read_bits(width2) + 2
                count = reader.read_bits(width1) + 2
                copy_from_history(output, distance, count, expected_size or (len(output) + count))
                continue

            prefix = reader.read_bits(1)
            if prefix == 0:
                distance = reader.read_bits(width3) + 2 + (1 << width2)
                count = reader.read_bits(width1) + 2
                copy_from_history(output, distance, count, expected_size or (len(output) + count))
                continue

            output.append(reader.read_bits(8))
    except EOFError:
        if expected_size is not None:
            raise

    return bytes(output), reader.bytes_consumed(offset)


def parse_cmap(data: bytes, offset: int) -> tuple[PaletteChunk, int]:
    if data[offset : offset + 4] != b"CMAP":
        raise ValueError(f"expected CMAP at 0x{offset:x}")
    count = data[offset + 4]
    palette_offset = offset + 5
    colors_end = palette_offset + count * 3
    chunk_end = colors_end + count * 2
    if chunk_end > len(data):
        raise ValueError(f"truncated CMAP at 0x{offset:x}")
    return (
        PaletteChunk(
            offset=offset,
            count=count,
            colors_vga=data[palette_offset:colors_end],
            aux_data=data[colors_end:chunk_end],
        ),
        chunk_end,
    )


def image_stream_offset(data: bytes) -> int | None:
    if data.startswith(b"CMAP"):
        return 0
    if data.startswith(b"ANIM") and data[6:10] == b"CMAP":
        return 6
    return None


def parse_images(path: Path) -> tuple[list[PaletteChunk], list[ImageFrame], list[int]]:
    data = path.read_bytes()
    palettes: list[PaletteChunk] = []
    frames: list[ImageFrame] = []
    skipped_offsets: list[int] = []
    current_palette = -1
    offset = image_stream_offset(data)
    if offset is None:
        raise ValueError(f"{path.name} does not contain a recognized image stream")

    while offset < len(data):
        signature = data[offset : offset + 4]
        if signature == b"CMAP":
            palette, offset = parse_cmap(data, offset)
            palettes.append(palette)
            current_palette = len(palettes) - 1
            continue

        if signature == b"PICT":
            if current_palette < 0:
                raise ValueError(f"PICT before CMAP in {path.name} at 0x{offset:x}")
            if offset + 13 > len(data):
                raise ValueError(f"truncated PICT header in {path.name} at 0x{offset:x}")

            unknown, height, width = struct.unpack_from("<HHH", data, offset + 4)
            widths = tuple(data[offset + 10 : offset + 13])
            pixels, consumed = decompress_stream(
                data=data,
                offset=offset + 13,
                expected_size=width * height,
                widths=widths,
            )
            next_offset = offset + 13 + consumed
            frames.append(
                ImageFrame(
                    offset=offset,
                    width=width,
                    height=height,
                    unknown=unknown,
                    widths=widths,
                    palette_index=current_palette,
                    pixels=pixels,
                    next_offset=next_offset,
                )
            )
            offset = next_offset
            continue

        skipped_offsets.append(offset)
        offset += 1

    return palettes, frames, skipped_offsets


def write_ppm(path: Path, width: int, height: int, pixels: bytes, palette: PaletteChunk) -> None:
    rgb_palette = palette.colors_rgb888()
    output = bytearray()
    for value in pixels:
        if value < len(rgb_palette):
            output.extend(rgb_palette[value])
        else:
            output.extend((0, 0, 0))
    header = f"P6\n{width} {height}\n255\n".encode("ascii")
    path.write_bytes(header + output)


def write_json(path: Path, payload: object) -> None:
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="ascii")


def write_text(path: Path, text: str) -> None:
    path.write_text(text, encoding="ascii")


TREKDAT_POINTER_ROWS = 13
TREKDAT_POINTER_COLUMNS = 24
TREKDAT_POINTER_COUNT = TREKDAT_POINTER_ROWS * TREKDAT_POINTER_COLUMNS
TREKDAT_POINTER_TABLE_BYTES = TREKDAT_POINTER_COUNT * 2
TREKDAT_SHAPE_ROWS = 0x410
TREKDAT_SHAPE_BASE = 10240
TREKDAT_VIEWPORT_WIDTH = 320
TREKDAT_VIEWPORT_HEIGHT = 200
PCM_SAMPLE_RATE = 8000
DEMO_TILE_POSITION_STEP = 0x666 / 0x10000
DAT_COLOR_MAP = (
    (0, 0, 0),
    (97, 0, 93),
    (113, 0, 101),
)
EXE_READER_SEGMENT_BASE = 0x66E
EXE_PHYSICS_SEGMENT = 0x1A2
EXE_DIRECT_IMAGE_SPECS = (
    {
        "name": "NUMBERS",
        "stream_offset": 0x13C,
        "count": 10,
        "x": 0,
        "y": 0,
        "width": 4,
        "height": 5,
    },
    {
        "name": "JUMPMASTER",
        "stream_offset": 0x204,
        "count": 2,
        "x": 203,
        "y": 156,
        "width": 26,
        "height": 5,
    },
)
EXE_KNOWN_LOCATIONS = (
    ("frame_start", EXE_PHYSICS_SEGMENT, 0x2308),
    ("touch_effect", EXE_PHYSICS_SEGMENT, 0x2369),
    ("move_before_constraint", EXE_PHYSICS_SEGMENT, 0x26C7),
    ("move_after_constraint", EXE_PHYSICS_SEGMENT, 0x26CD),
    ("oxygen_deplete", EXE_PHYSICS_SEGMENT, 0x2A23),
    ("fuel_deplete", EXE_PHYSICS_SEGMENT, 0x2A4E),
)
EXE_CONSTANT_PROBES = (
    ("DEMO_STEP", 0x0666),
    ("FULL_RESOURCE", 0x7530),
    ("RESOURCE_WARN", 0x6978),
    ("MAX_ZVEL", 0x2AAA),
    ("TREKDAT_ROWS", 0x0410),
    ("TREKDAT_PTR_BYTES", 0x0270),
)
EXE_RUNTIME_TABLE_SPECS = (
    {
        "name": "tile_class_by_low3",
        "offset": 0x0B77,
        "count": 8,
        "kind": "u8",
    },
    {
        "name": "draw_dispatch_by_type",
        "offset": 0x0B7F,
        "count": 16,
        "kind": "u16",
    },
)
EXE_RENDER_DISPATCH_LABELS = {
    0x2E50: "draw_type_0",
    0x303D: "draw_type_1",
    0x2E9F: "draw_type_2",
    0x2EE1: "draw_type_3",
    0x2F3C: "draw_type_4",
    0x2FB0: "draw_type_5",
    0x3AAD: "noop",
}
EXE_RENDER_BACKENDS = (
    {
        "selector": "[0x36] == 1",
        "setup": 0x348B,
        "emit_a": 0x3137,
        "emit_b": 0x3174,
        "advance": 0x323F,
        "normalize": None,
    },
    {
        "selector": "[0x36] != 1",
        "setup": 0x3462,
        "emit_a": 0x3083,
        "emit_b": 0x30D9,
        "advance": 0x31BF,
        "normalize": 0x3A23,
    },
)


def read_u16(data: bytes, offset: int) -> int:
    return struct.unpack_from("<H", data, offset)[0]


def write_rgb_ppm(path: Path, width: int, height: int, pixels: bytes) -> None:
    header = f"P6\n{width} {height}\n255\n".encode("ascii")
    path.write_bytes(header + pixels)


def trekdat_debug_rgb(value: int) -> tuple[int, int, int]:
    red = 64 + ((value * 53) % 160)
    green = 64 + ((value * 97) % 160)
    blue = 64 + ((value * 29) % 160)
    return red, green, blue


def expand_trekdat_record(load_buff_end: int, payload: bytes) -> tuple[bytes, int]:
    load_offset = load_buff_end - len(payload)
    if load_offset < 0:
        raise ValueError(
            f"TREKDAT record expands to a negative load offset: {load_buff_end=} {len(payload)=}"
        )

    working = bytearray(load_offset)
    working.extend(payload)
    src_ptr = load_offset
    output = bytearray()

    if src_ptr + TREKDAT_POINTER_TABLE_BYTES > len(working):
        raise ValueError("TREKDAT record is too short for its pointer table")
    output.extend(working[src_ptr : src_ptr + TREKDAT_POINTER_TABLE_BYTES])
    src_ptr += TREKDAT_POINTER_TABLE_BYTES

    for _ in range(TREKDAT_SHAPE_ROWS):
        if src_ptr + 3 > len(working):
            raise ValueError("TREKDAT record ended while copying shape headers")
        output.extend(working[src_ptr : src_ptr + 3])
        src_ptr += 3

        while True:
            if src_ptr >= len(working):
                raise ValueError("TREKDAT record ended while copying shape spans")
            value = working[src_ptr]
            output.append(value)
            src_ptr += 1
            if value == 0xFF:
                break
            if src_ptr >= len(working):
                raise ValueError("TREKDAT record ended while copying span width")
            output.append(working[src_ptr])
            src_ptr += 1
            output.append(0)

    if len(output) != load_buff_end:
        raise ValueError(
            f"TREKDAT expanded size mismatch: expected {load_buff_end}, produced {len(output)}"
        )
    return bytes(output), load_offset


def parse_trekdat_shape(expanded: bytes, start_offset: int) -> dict[str, object]:
    if start_offset + 3 > len(expanded):
        raise ValueError(f"TREKDAT shape offset 0x{start_offset:x} is out of range")

    color = expanded[start_offset]
    base_ptr = TREKDAT_SHAPE_BASE + read_u16(expanded, start_offset + 1)
    cursor = start_offset + 3
    ptr = base_ptr
    spans = []
    min_x = TREKDAT_VIEWPORT_WIDTH
    max_x = -1
    min_y = TREKDAT_VIEWPORT_HEIGHT
    max_y = -1
    nonzero_padding = 0

    while True:
        if cursor >= len(expanded):
            raise ValueError(f"TREKDAT shape at 0x{start_offset:x} is truncated")
        offset = expanded[cursor]
        cursor += 1
        if offset == 0xFF:
            break
        if cursor + 2 > len(expanded):
            raise ValueError(f"TREKDAT span at 0x{start_offset:x} is truncated")
        width = expanded[cursor]
        padding = expanded[cursor + 1]
        cursor += 2
        if padding != 0:
            nonzero_padding += 1
        ptr2 = ptr - offset
        x = ptr2 % TREKDAT_VIEWPORT_WIDTH
        y = ptr2 // TREKDAT_VIEWPORT_WIDTH
        spans.append(
            {
                "x": x,
                "y": y,
                "width": width,
                "offset": offset,
            }
        )
        if width > 0:
            min_x = min(min_x, x)
            max_x = max(max_x, x + width - 1)
            min_y = min(min_y, y)
            max_y = max(max_y, y)
        ptr += TREKDAT_VIEWPORT_WIDTH

    bbox = None
    if spans:
        bbox = {
            "x0": min_x,
            "y0": min_y,
            "x1": max_x,
            "y1": max_y,
            "width": max_x - min_x + 1,
            "height": max_y - min_y + 1,
        }

    return {
        "start_offset": start_offset,
        "size": cursor - start_offset,
        "color": color,
        "base_ptr": base_ptr,
        "span_count": len(spans),
        "nonzero_padding_count": nonzero_padding,
        "bbox": bbox,
        "spans": spans,
    }


def render_trekdat_record_sheet(
    pointer_table: list[int], shapes: dict[int, dict[str, object]], output_path: Path
) -> None:
    cell_width = 64
    cell_height = 40
    width = TREKDAT_POINTER_COLUMNS * cell_width
    height = TREKDAT_POINTER_ROWS * cell_height
    pixels = bytearray(width * height * 3)

    for y in range(height):
        for x in range(width):
            index = (y * width + x) * 3
            pixels[index : index + 3] = b"\x08\x08\x08"
    for row in range(TREKDAT_POINTER_ROWS + 1):
        y = min(row * cell_height, height - 1)
        for x in range(width):
            index = (y * width + x) * 3
            pixels[index : index + 3] = b"\x20\x20\x20"
    for column in range(TREKDAT_POINTER_COLUMNS + 1):
        x = min(column * cell_width, width - 1)
        for y in range(height):
            index = (y * width + x) * 3
            pixels[index : index + 3] = b"\x20\x20\x20"

    for shape_index, start_offset in enumerate(pointer_table):
        row, column = divmod(shape_index, TREKDAT_POINTER_COLUMNS)
        origin_x = column * cell_width
        origin_y = row * cell_height
        color = trekdat_debug_rgb(int(shapes[start_offset]["color"]))
        for span in shapes[start_offset]["spans"]:
            span_x = int(span["x"]) // 5
            span_y = int(span["y"]) // 5
            span_w = max(1, (int(span["width"]) + 4) // 5)
            if span_y < 0 or span_y >= cell_height:
                continue
            x0 = max(0, min(cell_width, span_x))
            x1 = max(0, min(cell_width, span_x + span_w))
            y = origin_y + span_y
            for x in range(origin_x + x0, origin_x + x1):
                index = (y * width + x) * 3
                pixels[index : index + 3] = bytes(color)

    write_rgb_ppm(output_path, width, height, bytes(pixels))


def parse_trekdat_records(data: bytes) -> list[dict[str, object]]:
    records = []
    offset = 0
    record_index = 0

    while offset < len(data):
        if offset + 7 > len(data):
            raise ValueError(f"TREKDAT record {record_index} is truncated at 0x{offset:x}")
        load_buff_end, bytes_to_read = struct.unpack_from("<HH", data, offset)
        widths = tuple(data[offset + 4 : offset + 7])
        payload, consumed = decompress_stream(
            data=data,
            offset=offset + 7,
            expected_size=bytes_to_read,
            widths=widths,
        )
        expanded, load_offset = expand_trekdat_record(load_buff_end, payload)
        pointer_table = [
            read_u16(expanded, entry_offset)
            for entry_offset in range(0, TREKDAT_POINTER_TABLE_BYTES, 2)
        ]
        if len(pointer_table) != TREKDAT_POINTER_COUNT:
            raise ValueError(f"TREKDAT record {record_index} pointer table has wrong size")
        if any(pointer < TREKDAT_POINTER_TABLE_BYTES for pointer in pointer_table):
            raise ValueError(f"TREKDAT record {record_index} contains a pointer into the table area")
        if any(pointer >= len(expanded) for pointer in pointer_table):
            raise ValueError(f"TREKDAT record {record_index} contains an out-of-range pointer")
        next_offset = offset + 7 + consumed
        records.append(
            {
                "index": record_index,
                "file_offset": offset,
                "next_file_offset": next_offset,
                "compressed_size": next_offset - offset,
                "load_buff_end": load_buff_end,
                "bytes_to_read": bytes_to_read,
                "load_offset": load_offset,
                "widths": widths,
                "payload": payload,
                "expanded": expanded,
                "pointer_table": pointer_table,
            }
        )
        offset = next_offset
        record_index += 1

    return records


def parse_muzax_song_headers(data: bytes) -> list[dict[str, int]]:
    if len(data) < 6:
        raise ValueError("MUZAX.LZS is too small to contain a song table")
    song_table_size = read_u16(data, 0)
    if song_table_size % 6:
        raise ValueError(f"MUZAX.LZS song table size is not a multiple of 6: {song_table_size}")
    if song_table_size > len(data):
        raise ValueError(f"MUZAX.LZS song table size is out of range: {song_table_size}")

    headers = []
    for index in range(song_table_size // 6):
        start_pos, num_instruments, uncompressed_length = struct.unpack_from("<HHH", data, index * 6)
        headers.append(
            {
                "index": index,
                "start_pos": start_pos,
                "num_instruments": num_instruments,
                "uncompressed_length": uncompressed_length,
            }
        )
    return headers


def parse_muzax_oscillator(block: bytes) -> dict[str, object]:
    tremolo, key_scale_level, attack_rate, sustain_level, wave_form = block
    return {
        "raw_hex": block.hex(),
        "tremolo": bool(tremolo & 0x80),
        "vibrato": bool(tremolo & 0x40),
        "sound_sustaining": bool(tremolo & 0x20),
        "key_scaling": bool(tremolo & 0x10),
        "multiplication": tremolo & 0x0F,
        "key_scale_level": key_scale_level >> 6,
        "output_level": key_scale_level & 0x3F,
        "attack_rate": attack_rate >> 4,
        "decay_rate": attack_rate & 0x0F,
        "sustain_level": sustain_level >> 4,
        "release_rate": sustain_level & 0x0F,
        "wave_form": wave_form & 0x07,
    }


def parse_muzax_instruments(data: bytes, count: int) -> list[dict[str, object]]:
    instruments = []
    for index in range(count):
        start = index * 16
        block = data[start : start + 16]
        if len(block) != 16:
            raise ValueError(f"MUZAX instrument {index} is truncated")
        instruments.append(
            {
                "index": index,
                "raw_hex": block.hex(),
                "operator_a": parse_muzax_oscillator(block[0:5]),
                "operator_b": parse_muzax_oscillator(block[5:10]),
                "channel_config": block[10],
                "tail_hex": block[11:16].hex(),
            }
        )
    return instruments


def summarize_muzax_commands(data: bytes, head_count: int = 32) -> dict[str, object]:
    command_count = len(data) // 2
    function_counts = [0] * 8
    head = []
    for index in range(command_count):
        low = data[index * 2]
        high = data[index * 2 + 1]
        function_type = low & 0x07
        channel = low >> 4
        function_counts[function_type] += 1
        if len(head) < head_count:
            head.append(
                {
                    "index": index,
                    "low": low,
                    "high": high,
                    "function_type": function_type,
                    "channel": channel,
                }
            )
    return {
        "byte_length": len(data),
        "odd_trailing_byte": len(data) % 2,
        "command_count": command_count,
        "function_counts": function_counts,
        "head": head,
    }


def write_wav_u8_pcm(path: Path, data: bytes, sample_rate: int = PCM_SAMPLE_RATE) -> None:
    byte_rate = sample_rate
    block_align = 1
    header = struct.pack(
        "<4sI4s4sIHHIIHH4sI",
        b"RIFF",
        36 + len(data),
        b"WAVE",
        b"fmt ",
        16,
        1,
        1,
        sample_rate,
        byte_rate,
        block_align,
        8,
        b"data",
        len(data),
    )
    path.write_bytes(header + data)


def parse_sfx_offsets(data: bytes) -> list[int]:
    if len(data) < 2:
        raise ValueError("SFX.SND is too small to contain an offset table")
    first_offset = read_u16(data, 0)
    if first_offset % 2:
        raise ValueError(f"SFX.SND first offset is not aligned: {first_offset}")
    if first_offset > len(data):
        raise ValueError(f"SFX.SND first offset is out of range: {first_offset}")

    offsets = [read_u16(data, offset) for offset in range(0, first_offset, 2)]
    if offsets[0] != first_offset:
        raise ValueError(
            f"SFX.SND offset table does not point to its first payload: {offsets[0]} != {first_offset}"
        )
    if offsets != sorted(offsets):
        raise ValueError("SFX.SND offsets are not monotonically increasing")
    return offsets


def inspect_sounds(source_root: Path) -> dict[str, object] | None:
    sounds = {}

    intro_path = source_root / "INTRO.SND"
    if intro_path.exists():
        intro_size = intro_path.stat().st_size
        sounds["intro"] = {
            "source": intro_path.name,
            "sample_rate": PCM_SAMPLE_RATE,
            "sample_count": intro_size,
            "duration_seconds": intro_size / PCM_SAMPLE_RATE,
        }

    sfx_path = source_root / "SFX.SND"
    if sfx_path.exists():
        data = sfx_path.read_bytes()
        offsets = parse_sfx_offsets(data)
        effects = []
        for index, start in enumerate(offsets):
            end = offsets[index + 1] if index + 1 < len(offsets) else len(data)
            effects.append(
                {
                    "index": index,
                    "start": start,
                    "end": end,
                    "length": end - start,
                }
            )
        sounds["sfx"] = {
            "source": sfx_path.name,
            "sample_rate": PCM_SAMPLE_RATE,
            "effect_count": len(effects),
            "effects": effects,
        }

    return sounds or None


def export_sounds(source_root: Path, output_root: Path) -> dict[str, object] | None:
    sounds = inspect_sounds(source_root)
    if sounds is None:
        return None

    sounds_dir = output_root / "sounds"
    sounds_dir.mkdir(parents=True, exist_ok=True)

    intro_info = sounds.get("intro")
    if intro_info is not None:
        intro_bytes = (source_root / "INTRO.SND").read_bytes()
        intro_raw_path = sounds_dir / "intro.raw.u8"
        intro_wav_path = sounds_dir / "intro.wav"
        intro_raw_path.write_bytes(intro_bytes)
        write_wav_u8_pcm(intro_wav_path, intro_bytes)
        intro_info["raw_path"] = intro_raw_path.name
        intro_info["wav_path"] = intro_wav_path.name

    sfx_info = sounds.get("sfx")
    if sfx_info is not None:
        sfx_dir = sounds_dir / "sfx"
        sfx_dir.mkdir(parents=True, exist_ok=True)
        data = (source_root / "SFX.SND").read_bytes()
        for effect in sfx_info["effects"]:
            index = int(effect["index"])
            start = int(effect["start"])
            end = int(effect["end"])
            payload = data[start:end]
            raw_path = sfx_dir / f"effect_{index:02d}.raw.u8"
            wav_path = sfx_dir / f"effect_{index:02d}.wav"
            raw_path.write_bytes(payload)
            write_wav_u8_pcm(wav_path, payload)
            effect["raw_path"] = str(raw_path.relative_to(sounds_dir))
            effect["wav_path"] = str(wav_path.relative_to(sounds_dir))

    write_json(sounds_dir / "manifest.json", sounds)
    return sounds


def decode_demo_entry(index: int, value: int) -> dict[str, object]:
    return {
        "index": index,
        "byte": value,
        "accelerate_decelerate": (value & 0x03) - 1,
        "left_right": ((value >> 2) & 0x03) - 1,
        "jump": bool((value >> 4) & 0x01),
        "tile_position": index * DEMO_TILE_POSITION_STEP,
    }


def inspect_demo(source_root: Path) -> dict[str, object] | None:
    demo_path = source_root / "DEMO.REC"
    if not demo_path.exists():
        return None

    data = demo_path.read_bytes()
    entries = [decode_demo_entry(index, value) for index, value in enumerate(data)]
    accel_counts: dict[str, int] = {}
    steer_counts: dict[str, int] = {}
    jump_counts = {"false": 0, "true": 0}
    for entry in entries:
        accel_key = str(entry["accelerate_decelerate"])
        steer_key = str(entry["left_right"])
        accel_counts[accel_key] = accel_counts.get(accel_key, 0) + 1
        steer_counts[steer_key] = steer_counts.get(steer_key, 0) + 1
        jump_counts["true" if entry["jump"] else "false"] += 1

    return {
        "source": demo_path.name,
        "byte_count": len(data),
        "approx_tile_length": len(data) * DEMO_TILE_POSITION_STEP,
        "accelerate_decelerate_counts": accel_counts,
        "left_right_counts": steer_counts,
        "jump_counts": jump_counts,
    }


def export_demo(source_root: Path, output_root: Path) -> dict[str, object] | None:
    demo_summary = inspect_demo(source_root)
    if demo_summary is None:
        return None

    demo_path = source_root / "DEMO.REC"
    data = demo_path.read_bytes()
    entries = [decode_demo_entry(index, value) for index, value in enumerate(data)]

    demo_dir = output_root / "demo"
    demo_dir.mkdir(parents=True, exist_ok=True)
    raw_path = demo_dir / "demo.rec.bin"
    entries_path = demo_dir / "inputs.json"
    raw_path.write_bytes(data)
    write_json(entries_path, entries)

    manifest = dict(demo_summary)
    manifest["raw_path"] = raw_path.name
    manifest["inputs_path"] = entries_path.name
    write_json(demo_dir / "manifest.json", manifest)
    return manifest


def parse_dashboard_dat(name: str, data: bytes) -> dict[str, object]:
    if len(data) < 4:
        raise ValueError(f"{name} is too small to contain a dashboard header")

    skip_word = read_u16(data, 0)
    probe_word = read_u16(data, 2)
    header_words = 0x22 if probe_word == 0x2C else 0x0A
    header_bytes = header_words * 2
    if header_bytes > len(data):
        raise ValueError(f"{name} header extends past EOF")

    fragments = []
    offset = header_bytes
    index = 0
    while offset < len(data):
        if offset + 4 > len(data):
            raise ValueError(f"{name} fragment {index} is truncated")
        position = read_u16(data, offset)
        width = data[offset + 2]
        height = data[offset + 3]
        offset += 4
        pixel_count = width * height
        if offset + pixel_count > len(data):
            raise ValueError(f"{name} fragment {index} pixel data is truncated")
        pixels = data[offset : offset + pixel_count]
        offset += pixel_count
        fragments.append(
            {
                "index": index,
                "position": position,
                "x": position % TREKDAT_VIEWPORT_WIDTH,
                "y": position // TREKDAT_VIEWPORT_WIDTH,
                "width": width,
                "height": height,
                "pixels": pixels,
            }
        )
        index += 1

    return {
        "source": name,
        "size_bytes": len(data),
        "skip_word": skip_word,
        "probe_word": probe_word,
        "header_words": header_words,
        "header_values": [read_u16(data, index) for index in range(0, header_bytes, 2)],
        "fragments": fragments,
    }


def render_dashboard_fragment(fragment: dict[str, object]) -> bytes:
    width = int(fragment["width"])
    height = int(fragment["height"])
    source = bytes(fragment["pixels"])
    output = bytearray(width * height * 3)
    for index, value in enumerate(source):
        color = DAT_COLOR_MAP[value] if value < len(DAT_COLOR_MAP) else (255, 255, 0)
        output[index * 3 : index * 3 + 3] = bytes(color)
    return bytes(output)


def render_dashboard_composite(fragments: list[dict[str, object]]) -> bytes:
    output = bytearray(TREKDAT_VIEWPORT_WIDTH * TREKDAT_VIEWPORT_HEIGHT * 3)
    for fragment in fragments:
        frag_x = int(fragment["x"])
        frag_y = int(fragment["y"])
        frag_w = int(fragment["width"])
        frag_h = int(fragment["height"])
        pixels = bytes(fragment["pixels"])
        for y in range(frag_h):
            for x in range(frag_w):
                value = pixels[y * frag_w + x]
                if value == 0:
                    continue
                dst_x = frag_x + x
                dst_y = frag_y + y
                if not (0 <= dst_x < TREKDAT_VIEWPORT_WIDTH and 0 <= dst_y < TREKDAT_VIEWPORT_HEIGHT):
                    continue
                color = DAT_COLOR_MAP[value] if value < len(DAT_COLOR_MAP) else (255, 255, 0)
                offset = (dst_y * TREKDAT_VIEWPORT_WIDTH + dst_x) * 3
                output[offset : offset + 3] = bytes(color)
    return bytes(output)


def inspect_dashboard_dats(source_root: Path) -> dict[str, object] | None:
    summaries = []
    for name in ("OXY_DISP.DAT", "FUL_DISP.DAT", "SPEED.DAT"):
        path = source_root / name
        if not path.exists():
            continue
        parsed = parse_dashboard_dat(name, path.read_bytes())
        summaries.append(
            {
                "source": name,
                "size_bytes": int(parsed["size_bytes"]),
                "header_words": int(parsed["header_words"]),
                "fragment_count": len(parsed["fragments"]),
            }
        )
    if not summaries:
        return None
    return {"files": summaries}


def export_dashboard_dats(source_root: Path, output_root: Path) -> dict[str, object] | None:
    dat_summary = inspect_dashboard_dats(source_root)
    if dat_summary is None:
        return None

    dats_dir = output_root / "dats"
    dats_dir.mkdir(parents=True, exist_ok=True)
    file_manifest = []

    for name in ("OXY_DISP.DAT", "FUL_DISP.DAT", "SPEED.DAT"):
        path = source_root / name
        if not path.exists():
            continue
        parsed = parse_dashboard_dat(name, path.read_bytes())
        stem = path.stem.lower()
        file_dir = dats_dir / stem
        file_dir.mkdir(parents=True, exist_ok=True)
        composite_path = file_dir / "composite.ppm"
        composite_rgb = render_dashboard_composite(list(parsed["fragments"]))
        write_rgb_ppm(
            composite_path,
            TREKDAT_VIEWPORT_WIDTH,
            TREKDAT_VIEWPORT_HEIGHT,
            composite_rgb,
        )

        fragment_manifest = []
        for fragment in parsed["fragments"]:
            fragment_index = int(fragment["index"])
            raw_path = file_dir / f"fragment_{fragment_index:02d}.indices.bin"
            ppm_path = file_dir / f"fragment_{fragment_index:02d}.ppm"
            raw_pixels = bytes(fragment["pixels"])
            raw_path.write_bytes(raw_pixels)
            write_rgb_ppm(ppm_path, int(fragment["width"]), int(fragment["height"]), render_dashboard_fragment(fragment))
            fragment_manifest.append(
                {
                    "index": fragment_index,
                    "position": int(fragment["position"]),
                    "x": int(fragment["x"]),
                    "y": int(fragment["y"]),
                    "width": int(fragment["width"]),
                    "height": int(fragment["height"]),
                    "indices_path": raw_path.name,
                    "ppm_path": ppm_path.name,
                }
            )

        manifest = {
            "source": name,
            "size_bytes": int(parsed["size_bytes"]),
            "skip_word": int(parsed["skip_word"]),
            "probe_word": int(parsed["probe_word"]),
            "header_words": int(parsed["header_words"]),
            "header_values": list(parsed["header_values"]),
            "composite_path": composite_path.name,
            "fragments": fragment_manifest,
        }
        write_json(file_dir / "manifest.json", manifest)
        file_manifest.append(
            {
                "source": name,
                "header_words": int(parsed["header_words"]),
                "fragment_count": len(fragment_manifest),
                "manifest_path": str((file_dir / "manifest.json").relative_to(dats_dir)),
                "composite_path": str(composite_path.relative_to(dats_dir)),
            }
        )

    result = {"files": file_manifest}
    write_json(dats_dir / "manifest.json", result)
    return result


def parse_mz_exe(data: bytes) -> dict[str, object]:
    if len(data) < 28 or data[:2] != b"MZ":
        raise ValueError("SKYROADS.EXE is not a recognized MZ executable")

    last_page_bytes = read_u16(data, 2)
    pages = read_u16(data, 4)
    relocation_count = read_u16(data, 6)
    header_paragraphs = read_u16(data, 8)
    min_alloc = read_u16(data, 10)
    max_alloc = read_u16(data, 12)
    ss = read_u16(data, 14)
    sp = read_u16(data, 16)
    checksum = read_u16(data, 18)
    ip = read_u16(data, 20)
    cs = read_u16(data, 22)
    relocation_table_offset = read_u16(data, 24)
    overlay = read_u16(data, 26)

    declared_file_size = (pages - 1) * 512 + (last_page_bytes or 512)
    header_bytes = header_paragraphs * 16
    if declared_file_size > len(data):
        raise ValueError(
            f"SKYROADS.EXE header declares {declared_file_size} bytes, but file is only {len(data)} bytes"
        )
    image_size = declared_file_size - header_bytes
    image = data[header_bytes : header_bytes + image_size]

    relocations = []
    for index in range(relocation_count):
        entry_offset = relocation_table_offset + index * 4
        if entry_offset + 4 > len(data):
            raise ValueError(f"SKYROADS.EXE relocation {index} is truncated")
        offset, segment = struct.unpack_from("<HH", data, entry_offset)
        image_offset = segment * 16 + offset
        relocations.append(
            {
                "index": index,
                "offset": offset,
                "segment": segment,
                "image_offset": image_offset,
                "file_offset": header_bytes + image_offset,
            }
        )

    entry_image_offset = cs * 16 + ip
    entry_file_offset = header_bytes + entry_image_offset
    exe_reader_base_image_offset = EXE_READER_SEGMENT_BASE * 16
    exe_reader_base_file_offset = header_bytes + exe_reader_base_image_offset

    return {
        "declared_file_size": declared_file_size,
        "header_bytes": header_bytes,
        "image_size": image_size,
        "relocation_count": relocation_count,
        "min_alloc": min_alloc,
        "max_alloc": max_alloc,
        "ss": ss,
        "sp": sp,
        "checksum": checksum,
        "ip": ip,
        "cs": cs,
        "relocation_table_offset": relocation_table_offset,
        "overlay": overlay,
        "relocations": relocations,
        "entry_image_offset": entry_image_offset,
        "entry_file_offset": entry_file_offset,
        "exe_reader_base_image_offset": exe_reader_base_image_offset,
        "exe_reader_base_file_offset": exe_reader_base_file_offset,
        "image": image,
    }


def build_exe_direct_palette(source_root: Path) -> list[tuple[int, int, int]]:
    palettes, _, _ = parse_images(source_root / "DASHBRD.LZS")
    dash_palette = palettes[0].colors_rgb888()
    palette = [(0, 0, 0), dash_palette[5]]
    while len(palette) < 256:
        palette.append(dash_palette[6])
    return palette


def write_indexed_ppm_custom(
    path: Path, width: int, height: int, pixels: bytes, palette: list[tuple[int, int, int]]
) -> None:
    output = bytearray()
    fallback = palette[-1] if palette else (0, 0, 0)
    for value in pixels:
        output.extend(palette[value] if value < len(palette) else fallback)
    write_rgb_ppm(path, width, height, bytes(output))


def find_u16_hits(data: bytes, value: int) -> list[int]:
    needle = struct.pack("<H", value)
    hits = []
    start = 0
    while True:
        index = data.find(needle, start)
        if index == -1:
            break
        hits.append(index)
        start = index + 1
    return hits


def exe_runtime_file_offset(parsed: dict[str, object], offset: int) -> int:
    return int(parsed["exe_reader_base_file_offset"]) + offset


def read_exe_runtime_bytes(parsed: dict[str, object], data: bytes, offset: int, size: int) -> tuple[bytes, int]:
    file_offset = exe_runtime_file_offset(parsed, offset)
    end = file_offset + size
    if end > len(data):
        raise ValueError(
            f"runtime data slice offset=0x{offset:04X} size=0x{size:X} extends past SKYROADS.EXE"
        )
    return data[file_offset:end], file_offset


def extract_exe_runtime_tables(parsed: dict[str, object], data: bytes) -> dict[str, object]:
    tables: dict[str, object] = {}
    for spec in EXE_RUNTIME_TABLE_SPECS:
        count = int(spec["count"])
        kind = str(spec["kind"])
        size = count if kind == "u8" else count * 2
        raw, file_offset = read_exe_runtime_bytes(parsed, data, int(spec["offset"]), size)
        image_offset = file_offset - int(parsed["header_bytes"])
        if kind == "u8":
            tables[str(spec["name"])] = {
                "offset": int(spec["offset"]),
                "image_offset": image_offset,
                "file_offset": file_offset,
                "kind": kind,
                "values": list(raw),
            }
        else:
            entries = []
            for index in range(count):
                target = read_u16(raw, index * 2)
                entry = {
                    "index": index,
                    "target": target,
                }
                label = EXE_RENDER_DISPATCH_LABELS.get(target)
                if label is not None:
                    entry["target_label"] = label
                entries.append(entry)
            tables[str(spec["name"])] = {
                "offset": int(spec["offset"]),
                "image_offset": image_offset,
                "file_offset": file_offset,
                "kind": kind,
                "entries": entries,
            }
    return tables


def format_hexdump(data: bytes, start: int, length: int, width: int = 16) -> str:
    lines = []
    end = min(len(data), start + length)
    for offset in range(start, end, width):
        chunk = data[offset : min(end, offset + width)]
        hex_part = " ".join(f"{byte:02x}" for byte in chunk)
        ascii_part = "".join(chr(byte) if 32 <= byte < 127 else "." for byte in chunk)
        lines.append(f"{offset:04x}: {hex_part:<{width * 3 - 1}}  {ascii_part}")
    return "\n".join(lines)


def inspect_exe(source_root: Path) -> dict[str, object] | None:
    exe_path = source_root / "SKYROADS.EXE"
    if not exe_path.exists():
        return None

    data = exe_path.read_bytes()
    parsed = parse_mz_exe(data)
    runtime_tables = extract_exe_runtime_tables(parsed, data)
    return {
        "source": exe_path.name,
        "header_bytes": int(parsed["header_bytes"]),
        "image_size": int(parsed["image_size"]),
        "entry_cs_ip": f"{int(parsed['cs']):04x}:{int(parsed['ip']):04x}",
        "entry_file_offset": int(parsed["entry_file_offset"]),
        "relocation_count": int(parsed["relocation_count"]),
        "exe_reader_base_file_offset": int(parsed["exe_reader_base_file_offset"]),
        "exe_reader_base_image_offset": int(parsed["exe_reader_base_image_offset"]),
        "embedded_images": [spec["name"] for spec in EXE_DIRECT_IMAGE_SPECS],
        "constant_probe_counts": {
            label: len(find_u16_hits(data, value))
            for label, value in EXE_CONSTANT_PROBES
        },
        "runtime_tables": list(runtime_tables),
    }


def export_exe(source_root: Path, output_root: Path) -> dict[str, object] | None:
    exe_path = source_root / "SKYROADS.EXE"
    if not exe_path.exists():
        return None

    data = exe_path.read_bytes()
    parsed = parse_mz_exe(data)
    image = bytes(parsed.pop("image"))

    exe_dir = output_root / "exe"
    exe_dir.mkdir(parents=True, exist_ok=True)
    image_path = exe_dir / "load_module.bin"
    image_path.write_bytes(image)

    direct_palette = build_exe_direct_palette(source_root)
    embedded_dir = exe_dir / "embedded_images"
    embedded_dir.mkdir(parents=True, exist_ok=True)
    embedded_manifest = []
    for spec in EXE_DIRECT_IMAGE_SPECS:
        image_dir = embedded_dir / spec["name"].lower()
        image_dir.mkdir(parents=True, exist_ok=True)
        file_offset = int(parsed["exe_reader_base_file_offset"]) + int(spec["stream_offset"])
        byte_count = int(spec["count"]) * int(spec["width"]) * int(spec["height"])
        raw = data[file_offset : file_offset + byte_count]
        if len(raw) != byte_count:
            raise ValueError(f"{spec['name']} embedded image data is truncated in SKYROADS.EXE")

        frames = []
        frame_size = int(spec["width"]) * int(spec["height"])
        for index in range(int(spec["count"])):
            pixels = raw[index * frame_size : (index + 1) * frame_size]
            indices_path = image_dir / f"frame_{index:02d}.indices.bin"
            ppm_path = image_dir / f"frame_{index:02d}.ppm"
            indices_path.write_bytes(pixels)
            write_indexed_ppm_custom(
                ppm_path,
                int(spec["width"]),
                int(spec["height"]),
                pixels,
                direct_palette,
            )
            frames.append(
                {
                    "index": index,
                    "indices_path": indices_path.name,
                    "ppm_path": ppm_path.name,
                }
            )

        manifest = {
            "name": spec["name"],
            "stream_offset": int(spec["stream_offset"]),
            "file_offset": file_offset,
            "count": int(spec["count"]),
            "x": int(spec["x"]),
            "y": int(spec["y"]),
            "width": int(spec["width"]),
            "height": int(spec["height"]),
            "frames": frames,
        }
        write_json(image_dir / "manifest.json", manifest)
        embedded_manifest.append(
            {
                "name": spec["name"],
                "manifest_path": str((image_dir / "manifest.json").relative_to(exe_dir)),
                "file_offset": file_offset,
                "count": int(spec["count"]),
                "width": int(spec["width"]),
                "height": int(spec["height"]),
            }
        )

    known_locations = []
    for label, segment, offset in EXE_KNOWN_LOCATIONS:
        image_offset = segment * 16 + offset
        file_offset = int(parsed["header_bytes"]) + image_offset
        known_locations.append(
            {
                "label": label,
                "segment": segment,
                "offset": offset,
                "image_offset": image_offset,
                "file_offset": file_offset,
                "bytes_hex_head": data[file_offset : file_offset + 32].hex(),
            }
        )

    constant_hits = {}
    for label, value in EXE_CONSTANT_PROBES:
        hits = []
        for file_offset in find_u16_hits(data, value):
            hits.append(
                {
                    "file_offset": file_offset,
                    "image_offset": file_offset - int(parsed["header_bytes"])
                    if file_offset >= int(parsed["header_bytes"])
                    else None,
                }
            )
        constant_hits[label] = {
            "value": value,
            "hits": hits,
        }

    runtime_tables = extract_exe_runtime_tables(parsed, data)
    runtime_tables_path = exe_dir / "runtime_tables.json"
    write_json(runtime_tables_path, runtime_tables)

    render_backends = []
    for backend in EXE_RENDER_BACKENDS:
        backend_entry = {
            "selector": str(backend["selector"]),
            "setup": int(backend["setup"]),
            "emit_a": int(backend["emit_a"]),
            "emit_b": int(backend["emit_b"]),
            "advance": int(backend["advance"]),
        }
        normalize = backend["normalize"]
        if normalize is not None:
            backend_entry["normalize"] = int(normalize)
        render_backends.append(backend_entry)

    reports_dir = exe_dir / "reports"
    reports_dir.mkdir(parents=True, exist_ok=True)

    demo_report = """# Demo Index Routine

High-confidence findings from the `0x0666` demo divisor region in `SKYROADS.EXE`.

- File offsets `0x0C4A`, `0x0C73`, and `0x0CA0` each load literal `0x0666`.
- Each sequence reads the current Z position from `DS:9628` and `DS:962A`.
- Each sequence calls a shared helper, then uses the returned index to read a byte from `DS:[BX + 0x962E]`.
- The first sequence stores `(byte & 3) - 1` to `DS:933C`.
- The second sequence shifts right twice, then stores `((byte >> 2) & 3) - 1` to `DS:9600`.
- The third sequence shifts right four times, masks to one bit, and stores it to `DS:5488`.

This matches the current `DEMO.REC` interpretation:

- accelerate/decelerate: `(al & 3) - 1`
- left/right: `((al >> 2) & 3) - 1`
- jump: `((al >> 4) & 1)`

## Hex Dump

```text
""" + format_hexdump(data, 0x0C20, 0xC0) + """
```
"""
    write_text(reports_dir / "demo_index.md", demo_report)

    trekdat_report = """# TREKDAT Expansion Routine

High-confidence findings from the TREKDAT loader region in `SKYROADS.EXE`.

- The routine at file offset `0x3C78` loads literal `0x0410`, matching the 1040-row expansion loop.
- The same region contains `0x0138` followed by `f3 a5`, which is the classic `rep movsw` pattern for copying `0x138` words = 624 bytes.
- After that copy, the code repeatedly copies 3 bytes, then copies `(value, width)` pairs while inserting a zero byte until it reaches `0xFF`.
- That matches the current reconstructed TREKDAT expansion model exactly:
  - copy 624-byte pointer table
  - repeat 1040 times:
    - copy 3-byte header
    - copy bytes until `0xFF`
    - after each non-`0xFF` byte pair, insert `0x00`

The earlier routine starting around `0x3C23` also uses `0x0410` and `0x0270`; it is likely part of the same TREKDAT decode path, but its exact role is still lower-confidence.

## Hex Dump

```text
""" + format_hexdump(data, 0x3C10, 0xC0) + """
```
"""
    write_text(reports_dir / "trekdat_expand.md", trekdat_report)

    physics_report = """# Physics Constant Anchors

Useful executable-side anchors for the current DOS build.

- Oxygen depletion routine note `LOC 2A23` maps to file offset `0x4643`.
- Fuel depletion routine note `LOC 2A4E` maps to file offset `0x466E`.
- The full resource constant `0x7530` appears in the same broad physics area.
- The low-resource warning threshold `0x6978` appears at file offsets `0x1CE4` and `0x1CEF`.
- The max forward speed constant `0x2AAA` appears repeatedly, including `0x1D9E`, `0x1DA9`, `0x26FC`, and `0x2707`.

## Hex Dump

```text
""" + format_hexdump(data, 0x4620, 0x80) + """
```
"""
    write_text(reports_dir / "physics_anchors.md", physics_report)

    tile_class_values = runtime_tables["tile_class_by_low3"]["values"]
    draw_dispatch_entries = runtime_tables["draw_dispatch_by_type"]["entries"]
    dispatch_lines = []
    for entry in draw_dispatch_entries:
        line = f"- type {int(entry['index']):X}: `0x{int(entry['target']):04X}`"
        label = entry.get("target_label")
        if label is not None:
            line += f" (`{label}`)"
        dispatch_lines.append(line)
    backend_lines = []
    for backend in render_backends:
        line = (
            f"- {backend['selector']}: "
            f"`setup=0x{backend['setup']:04X}` "
            f"`emit_a=0x{backend['emit_a']:04X}` "
            f"`emit_b=0x{backend['emit_b']:04X}` "
            f"`advance=0x{backend['advance']:04X}`"
        )
        normalize = backend.get("normalize")
        if normalize is not None:
            line += f" `normalize=0x{int(normalize):04X}`"
        backend_lines.append(line)

    renderer_report = f"""# TREKDAT Renderer Path

High-confidence static findings from the renderer/TREKDAT path in `SKYROADS.EXE`.

- The runtime data/stack segment starts at image offset `0x{int(parsed["exe_reader_base_image_offset"]):04X}`.
- The startup loader zeroes `DS:54B4` at image offset `0x00DD` and then runs an 8-record loop starting at `0x00E6`.
- Each loop iteration:
  - calls `0x64F0` twice to fetch record size values
  - allocates a destination segment
  - stores that segment to `SS:[0x0E82 + 2 * index]`
  - calls `0x6660` to expand one `TREKDAT` record into that segment
- The initialization path at `0x2CB4` calls `0x3A7A`, which walks the same 8-entry segment table and performs the in-place `624-byte table + 1040 rows` expansion.
- In the backend where `[0x36] != 1`, the init path also calls `0x3A23`, which performs the secondary row-normalization pass across those same 8 records.

## Main Draw Loop

- The road draw routine begins at image offset `0x2D03`.
- It computes the coarse road row as `DS:0x0E36 >> 3`.
- It computes the active `TREKDAT` ring slot as `DS:0x0E36 & 7`.
- It selects the expanded `TREKDAT` segment from `SS:[0x0E82 + 2 * slot]`.
- It selects road bytes from `0x1638 + (row_group * 0x0E) + 0x62`.
- The low nibble of the second road byte dispatches through the 16-entry table at `SS:0x0B7F`.

## Initialized Runtime Tables

- `DS:0x0B77` tile-class table: `{", ".join(str(value) for value in tile_class_values)}`
- `SS:0x0B7F` draw dispatch table:
{chr(10).join(dispatch_lines)}

## Backend Helper Sets

{chr(10).join(backend_lines)}
"""
    write_text(reports_dir / "renderer_path.md", renderer_report)

    manifest = {
        "source": exe_path.name,
        "file_size": len(data),
        "header_bytes": int(parsed["header_bytes"]),
        "image_size": int(parsed["image_size"]),
        "entry_cs_ip": f"{int(parsed['cs']):04x}:{int(parsed['ip']):04x}",
        "entry_image_offset": int(parsed["entry_image_offset"]),
        "entry_file_offset": int(parsed["entry_file_offset"]),
        "stack_ss_sp": f"{int(parsed['ss']):04x}:{int(parsed['sp']):04x}",
        "min_alloc": int(parsed["min_alloc"]),
        "max_alloc": int(parsed["max_alloc"]),
        "checksum": int(parsed["checksum"]),
        "overlay": int(parsed["overlay"]),
        "relocation_table_offset": int(parsed["relocation_table_offset"]),
        "relocations": list(parsed["relocations"]),
        "exe_reader_base_segment": EXE_READER_SEGMENT_BASE,
        "exe_reader_base_image_offset": int(parsed["exe_reader_base_image_offset"]),
        "exe_reader_base_file_offset": int(parsed["exe_reader_base_file_offset"]),
        "load_module_path": image_path.name,
        "embedded_images": embedded_manifest,
        "known_locations": known_locations,
        "constant_hits": constant_hits,
        "runtime_tables_path": runtime_tables_path.name,
        "runtime_tables": runtime_tables,
        "render_backends": render_backends,
        "reports": [
            "reports/demo_index.md",
            "reports/trekdat_expand.md",
            "reports/physics_anchors.md",
            "reports/renderer_path.md",
        ],
    }
    write_json(exe_dir / "manifest.json", manifest)
    return {
        "source": exe_path.name,
        "entry_cs_ip": manifest["entry_cs_ip"],
        "image_size": manifest["image_size"],
        "relocation_count": len(manifest["relocations"]),
        "embedded_images": [item["name"] for item in embedded_manifest],
    }


def export_image_resource(source_path: Path, output_root: Path) -> dict[str, object]:
    palettes, frames, skipped_offsets = parse_images(source_path)
    resource_dir = output_root / source_path.stem.lower()
    resource_dir.mkdir(parents=True, exist_ok=True)

    palette_manifest = []
    for index, palette in enumerate(palettes):
        vga_path = resource_dir / f"palette_{index:03d}.vga.bin"
        aux_path = resource_dir / f"palette_{index:03d}.aux.bin"
        vga_path.write_bytes(palette.colors_vga)
        aux_path.write_bytes(palette.aux_data)
        palette_manifest.append(
            {
                "index": index,
                "offset": palette.offset,
                "count": palette.count,
                "vga_path": vga_path.name,
                "aux_path": aux_path.name,
            }
        )

    frame_manifest = []
    for index, frame in enumerate(frames):
        indices_path = resource_dir / f"frame_{index:03d}.indices.bin"
        ppm_path = resource_dir / f"frame_{index:03d}.ppm"
        indices_path.write_bytes(frame.pixels)
        write_ppm(ppm_path, frame.width, frame.height, frame.pixels, palettes[frame.palette_index])
        frame_manifest.append(
            {
                "index": index,
                "offset": frame.offset,
                "next_offset": frame.next_offset,
                "width": frame.width,
                "height": frame.height,
                "unknown": frame.unknown,
                "widths": list(frame.widths),
                "palette_index": frame.palette_index,
                "indices_path": indices_path.name,
                "ppm_path": ppm_path.name,
            }
        )

    manifest = {
        "source": source_path.name,
        "size_bytes": source_path.stat().st_size,
        "palettes": palette_manifest,
        "frames": frame_manifest,
        "skipped_bytes": skipped_offsets,
    }
    write_json(resource_dir / "manifest.json", manifest)
    return {
        "source": source_path.name,
        "palette_count": len(palettes),
        "frame_count": len(frames),
        "skipped_bytes": len(skipped_offsets),
    }


def iter_rows(values: Iterable[int], width: int) -> list[list[int]]:
    flat = list(values)
    return [flat[index : index + width] for index in range(0, len(flat), width)]


def analyze_road_descriptor(value: int) -> dict[str, int]:
    low_byte = value & 0xFF
    high_byte = (value >> 8) & 0xFF
    return {
        "raw": value,
        "low_byte": low_byte,
        "high_byte": high_byte,
        "dispatch_kind": high_byte & 0x0F,
        "dispatch_variant_low3": high_byte & 0x07,
        "high_flags": high_byte >> 4,
    }


def build_road_descriptor_catalog(roads: list[dict[str, object]]) -> dict[str, object]:
    descriptor_counts: Counter[int] = Counter()
    dispatch_kind_counts: Counter[int] = Counter()
    descriptor_roads: dict[int, set[int]] = defaultdict(set)
    dispatch_kind_roads: dict[int, set[int]] = defaultdict(set)
    descriptor_samples: dict[int, list[dict[str, int]]] = defaultdict(list)
    dispatch_kind_samples: dict[int, list[dict[str, int]]] = defaultdict(list)

    for road in roads:
        road_index = int(road["index"])
        rows = road["rows"]
        for row_index, row in enumerate(rows):
            for column_index, value in enumerate(row):
                analyzed = analyze_road_descriptor(int(value))
                raw = analyzed["raw"]
                dispatch_kind = analyzed["dispatch_kind"]
                descriptor_counts[raw] += 1
                dispatch_kind_counts[dispatch_kind] += 1
                descriptor_roads[raw].add(road_index)
                dispatch_kind_roads[dispatch_kind].add(road_index)
                if len(descriptor_samples[raw]) < 8:
                    descriptor_samples[raw].append(
                        {
                            "road_index": road_index,
                            "row_index": row_index,
                            "column_index": column_index,
                        }
                    )
                if len(dispatch_kind_samples[dispatch_kind]) < 8:
                    dispatch_kind_samples[dispatch_kind].append(
                        {
                            "road_index": road_index,
                            "row_index": row_index,
                            "column_index": column_index,
                            "raw": raw,
                        }
                    )

    descriptor_entries = []
    for raw in sorted(descriptor_counts):
        analyzed = analyze_road_descriptor(raw)
        descriptor_entries.append(
            {
                **analyzed,
                "count": descriptor_counts[raw],
                "roads": sorted(descriptor_roads[raw]),
                "samples": descriptor_samples[raw],
            }
        )

    dispatch_entries = []
    for dispatch_kind in sorted(dispatch_kind_counts):
        members = [
            entry
            for entry in descriptor_entries
            if int(entry["dispatch_kind"]) == dispatch_kind
        ]
        dispatch_entries.append(
            {
                "dispatch_kind": dispatch_kind,
                "count": dispatch_kind_counts[dispatch_kind],
                "roads": sorted(dispatch_kind_roads[dispatch_kind]),
                "descriptor_count": len(members),
                "descriptors": [
                    {
                        "raw": int(entry["raw"]),
                        "count": int(entry["count"]),
                        "low_byte": int(entry["low_byte"]),
                        "high_byte": int(entry["high_byte"]),
                    }
                    for entry in members[:32]
                ],
                "samples": dispatch_kind_samples[dispatch_kind],
            }
        )

    lines = [
        "# Road Descriptor Summary",
        "",
        "- Renderer dispatch is driven by the low nibble of the descriptor's second byte.",
        f"- Used dispatch kinds in shipped road data: `{', '.join(str(entry['dispatch_kind']) for entry in dispatch_entries)}`",
        f"- Distinct raw descriptors: `{len(descriptor_entries)}`",
        "",
        "## Dispatch Kind Counts",
        "",
    ]
    for entry in dispatch_entries:
        lines.append(
            f"- kind `{entry['dispatch_kind']}`: `{entry['count']}` cells across `{len(entry['roads'])}` roads"
        )
    lines.extend(
        [
            "",
            "## Kind Samples",
            "",
        ]
    )
    for entry in dispatch_entries:
        top = ", ".join(
            f"0x{int(item['raw']):04X} ({int(item['count'])})" for item in entry["descriptors"][:8]
        )
        lines.append(f"- kind `{entry['dispatch_kind']}` top descriptors: {top}")

    return {
        "used_dispatch_kinds": [int(entry["dispatch_kind"]) for entry in dispatch_entries],
        "dispatch_kind_counts": {str(key): value for key, value in sorted(dispatch_kind_counts.items())},
        "dispatch_kinds": dispatch_entries,
        "descriptor_count": len(descriptor_entries),
        "descriptors": descriptor_entries,
        "summary_markdown": "\n".join(lines) + "\n",
    }


def export_roads(source_path: Path, output_root: Path) -> dict[str, object]:
    data = source_path.read_bytes()
    first_offset = struct.unpack_from("<H", data, 0)[0]
    if first_offset % 4:
        raise ValueError(f"unexpected ROADS header size {first_offset}")

    entry_count = first_offset // 4
    entries = [struct.unpack_from("<HH", data, index * 4) for index in range(entry_count)]
    roads_dir = output_root / "roads"
    roads_dir.mkdir(parents=True, exist_ok=True)

    summary = []
    road_catalog_inputs = []
    for index, (offset, unpacked_size) in enumerate(entries):
        next_offset = entries[index + 1][0] if index + 1 < len(entries) else len(data)
        road_blob = data[offset:next_offset]
        if len(road_blob) < 225:
            raise ValueError(f"road {index} is too small to contain metadata and compression widths")

        gravity, fuel, oxygen = struct.unpack_from("<HHH", road_blob, 0)
        palette_vga = road_blob[6:222]
        widths = tuple(road_blob[222:225])
        raw_tiles, _ = decompress_stream(
            data=road_blob,
            offset=225,
            expected_size=unpacked_size,
            widths=widths,
        )

        if len(raw_tiles) % 2:
            raise ValueError(f"road {index} decompressed to an odd number of bytes")
        values = struct.unpack(f"<{len(raw_tiles) // 2}H", raw_tiles)
        rows = iter_rows(values, 7)

        prefix = roads_dir / f"road_{index:02d}"
        (prefix.with_suffix(".palette.vga.bin")).write_bytes(palette_vga)
        (prefix.with_suffix(".tiles.bin")).write_bytes(raw_tiles)
        write_json(
            prefix.with_suffix(".json"),
            {
                "index": index,
                "offset": offset,
                "compressed_size": len(road_blob),
                "unpacked_size": unpacked_size,
                "gravity": gravity,
                "fuel": fuel,
                "oxygen": oxygen,
                "widths": list(widths),
                "row_count": len(rows),
                "rows": rows,
                "dispatch_kind_counts": {
                    str(key): value
                    for key, value in sorted(Counter(((value >> 8) & 0x0F) for value in values).items())
                },
                "descriptor_count": len(set(values)),
            },
        )
        road_catalog_inputs.append(
            {
                "index": index,
                "rows": rows,
            }
        )
        summary.append(
            {
                "index": index,
                "offset": offset,
                "compressed_size": len(road_blob),
                "unpacked_size": unpacked_size,
                "row_count": len(rows),
                "gravity": gravity,
                "fuel": fuel,
                "oxygen": oxygen,
                "dispatch_kind_counts": {
                    str(key): value
                    for key, value in sorted(Counter(((value >> 8) & 0x0F) for value in values).items())
                },
                "descriptor_count": len(set(values)),
            }
        )

    descriptor_catalog = build_road_descriptor_catalog(road_catalog_inputs)
    write_json(roads_dir / "descriptor_catalog.json", descriptor_catalog)
    write_text(roads_dir / "descriptor_summary.md", str(descriptor_catalog["summary_markdown"]))

    write_json(
        roads_dir / "manifest.json",
        {
            "source": source_path.name,
            "entry_count": entry_count,
            "roads": summary,
            "descriptor_catalog_path": "descriptor_catalog.json",
            "descriptor_summary_path": "descriptor_summary.md",
            "used_dispatch_kinds": descriptor_catalog["used_dispatch_kinds"],
        },
    )
    return {
        "source": source_path.name,
        "road_count": entry_count,
        "used_dispatch_kinds": descriptor_catalog["used_dispatch_kinds"],
    }


def export_trekdat(source_path: Path, output_root: Path) -> dict[str, object]:
    data = source_path.read_bytes()
    if len(data) < 7:
        raise ValueError(f"{source_path.name} is too small to contain the observed TREKDAT header")

    records = parse_trekdat_records(data)

    trekdat_dir = output_root / "trekdat"
    records_dir = trekdat_dir / "records"
    records_dir.mkdir(parents=True, exist_ok=True)

    record_manifest = []
    for record in records:
        index = int(record["index"])
        prefix = records_dir / f"record_{index:02d}"
        payload_path = prefix.with_suffix(".decompressed.bin")
        expanded_path = prefix.with_suffix(".expanded.bin")
        pointer_table_path = prefix.with_suffix(".pointer_table.json")
        shapes_path = prefix.with_suffix(".shapes.json")
        preview_path = prefix.with_suffix(".preview.ppm")

        payload = bytes(record["payload"])
        expanded = bytes(record["expanded"])
        pointer_table = list(record["pointer_table"])
        payload_path.write_bytes(payload)
        expanded_path.write_bytes(expanded)

        pointer_rows = iter_rows(pointer_table, TREKDAT_POINTER_COLUMNS)
        unique_shapes = {
            start_offset: parse_trekdat_shape(expanded, start_offset) for start_offset in sorted(set(pointer_table))
        }
        shape_entries = []
        total_span_count = 0
        total_nonzero_padding = 0
        for shape_index, start_offset in enumerate(pointer_table):
            row, column = divmod(shape_index, TREKDAT_POINTER_COLUMNS)
            shape_entry = dict(unique_shapes[start_offset])
            shape_entry["table_index"] = shape_index
            shape_entry["table_row"] = row
            shape_entry["table_column"] = column
            shape_entries.append(shape_entry)
            total_span_count += int(shape_entry["span_count"])
            total_nonzero_padding += int(shape_entry["nonzero_padding_count"])

        write_json(pointer_table_path, pointer_rows)
        write_json(shapes_path, shape_entries)
        render_trekdat_record_sheet(pointer_table, unique_shapes, preview_path)

        record_manifest.append(
            {
                "index": index,
                "file_offset": int(record["file_offset"]),
                "next_file_offset": int(record["next_file_offset"]),
                "compressed_size": int(record["compressed_size"]),
                "load_buff_end": int(record["load_buff_end"]),
                "bytes_to_read": int(record["bytes_to_read"]),
                "load_offset": int(record["load_offset"]),
                "widths": list(record["widths"]),
                "payload_size": len(payload),
                "expanded_size": len(expanded),
                "pointer_rows": TREKDAT_POINTER_ROWS,
                "pointer_columns": TREKDAT_POINTER_COLUMNS,
                "pointer_min": min(pointer_table),
                "pointer_max": max(pointer_table),
                "unique_pointer_count": len(unique_shapes),
                "total_span_count": total_span_count,
                "total_nonzero_padding": total_nonzero_padding,
                "decompressed_path": str(payload_path.relative_to(trekdat_dir)),
                "expanded_path": str(expanded_path.relative_to(trekdat_dir)),
                "pointer_table_path": str(pointer_table_path.relative_to(trekdat_dir)),
                "shapes_path": str(shapes_path.relative_to(trekdat_dir)),
                "preview_path": str(preview_path.relative_to(trekdat_dir)),
            }
        )

    manifest = {
        "source": source_path.name,
        "compressed_size": len(data),
        "record_count": len(records),
        "pointer_rows": TREKDAT_POINTER_ROWS,
        "pointer_columns": TREKDAT_POINTER_COLUMNS,
        "records": record_manifest,
    }
    write_json(trekdat_dir / "manifest.json", manifest)
    return {
        "source": source_path.name,
        "record_count": len(records),
        "expanded_sizes": [int(record["load_buff_end"]) for record in records],
        "widths": [list(record["widths"]) for record in records],
    }


def export_muzax(source_path: Path, output_root: Path) -> dict[str, object]:
    data = source_path.read_bytes()
    headers = parse_muzax_song_headers(data)
    song_table_size = read_u16(data, 0)
    muzax_dir = output_root / "muzax"
    songs_dir = muzax_dir / "songs"
    muzax_dir.mkdir(parents=True, exist_ok=True)
    songs_dir.mkdir(parents=True, exist_ok=True)
    (muzax_dir / "song_table.bin").write_bytes(data[:song_table_size])

    next_starts = []
    for index, header in enumerate(headers):
        next_start = len(data)
        for later in headers[index + 1 :]:
            if later["start_pos"] != 0:
                next_start = later["start_pos"]
                break
        next_starts.append(next_start)

    song_manifest = []
    for header, next_start in zip(headers, next_starts):
        index = header["index"]
        prefix = songs_dir / f"song_{index:02d}"
        entry = dict(header)
        entry["is_empty"] = (
            header["start_pos"] == 0
            and header["num_instruments"] == 0
            and header["uncompressed_length"] == 0
        )
        if entry["is_empty"]:
            entry["next_song_start"] = next_start
            song_manifest.append(entry)
            continue

        start_pos = header["start_pos"]
        if start_pos + 3 > len(data):
            raise ValueError(f"MUZAX song {index} starts out of range at 0x{start_pos:x}")
        widths = tuple(data[start_pos : start_pos + 3])
        payload, consumed = decompress_stream(
            data=data,
            offset=start_pos + 3,
            expected_size=header["uncompressed_length"],
            widths=widths,
        )
        instrument_bytes = header["num_instruments"] * 16
        if instrument_bytes > len(payload):
            raise ValueError(
                f"MUZAX song {index} instrument region exceeds payload: {instrument_bytes} > {len(payload)}"
            )
        instruments_blob = payload[:instrument_bytes]
        commands_blob = payload[instrument_bytes:]
        payload_path = prefix.with_suffix(".payload.bin")
        instruments_path = prefix.with_suffix(".instruments.bin")
        commands_path = prefix.with_suffix(".commands.bin")
        analysis_path = prefix.with_suffix(".json")
        payload_path.write_bytes(payload)
        instruments_path.write_bytes(instruments_blob)
        commands_path.write_bytes(commands_blob)
        song_analysis = {
            "index": index,
            "start_pos": start_pos,
            "next_song_start": next_start,
            "compressed_end": start_pos + 3 + consumed,
            "compressed_size": 3 + consumed,
            "widths": list(widths),
            "num_instruments": header["num_instruments"],
            "uncompressed_length": header["uncompressed_length"],
            "instrument_bytes": instrument_bytes,
            "command_bytes": len(commands_blob),
            "instruments": parse_muzax_instruments(instruments_blob, header["num_instruments"]),
            "command_summary": summarize_muzax_commands(commands_blob),
        }
        write_json(analysis_path, song_analysis)
        entry.update(
            {
                "next_song_start": next_start,
                "compressed_end": start_pos + 3 + consumed,
                "compressed_size": 3 + consumed,
                "widths": list(widths),
                "instrument_bytes": instrument_bytes,
                "command_bytes": len(commands_blob),
                "payload_path": str(payload_path.relative_to(muzax_dir)),
                "instruments_path": str(instruments_path.relative_to(muzax_dir)),
                "commands_path": str(commands_path.relative_to(muzax_dir)),
                "analysis_path": str(analysis_path.relative_to(muzax_dir)),
            }
        )
        song_manifest.append(entry)

    write_json(
        muzax_dir / "manifest.json",
        {
            "source": source_path.name,
            "compressed_size": len(data),
            "song_table_size": song_table_size,
            "song_count": len(headers),
            "populated_song_count": sum(not song["is_empty"] for song in song_manifest),
            "songs": song_manifest,
        },
    )
    return {
        "source": source_path.name,
        "song_table_size": song_table_size,
        "song_count": len(headers),
        "populated_song_count": sum(not song["is_empty"] for song in song_manifest),
    }


def inspect_trekdat(source_path: Path) -> dict[str, object]:
    data = source_path.read_bytes()
    if len(data) < 7:
        raise ValueError(f"{source_path.name} is too small to contain the observed TREKDAT header")

    records = parse_trekdat_records(data)
    return {
        "source": source_path.name,
        "record_count": len(records),
        "pointer_rows": TREKDAT_POINTER_ROWS,
        "pointer_columns": TREKDAT_POINTER_COLUMNS,
        "records": [
            {
                "index": int(record["index"]),
                "file_offset": int(record["file_offset"]),
                "compressed_size": int(record["compressed_size"]),
                "load_buff_end": int(record["load_buff_end"]),
                "bytes_to_read": int(record["bytes_to_read"]),
                "widths": list(record["widths"]),
            }
            for record in records
        ],
    }


def inspect_muzax(source_path: Path) -> dict[str, object]:
    data = source_path.read_bytes()
    headers = parse_muzax_song_headers(data)
    return {
        "source": source_path.name,
        "song_table_size": read_u16(data, 0),
        "song_count": len(headers),
        "populated_song_count": sum(
            not (
                header["start_pos"] == 0
                and header["num_instruments"] == 0
                and header["uncompressed_length"] == 0
            )
            for header in headers
        ),
        "songs": headers,
    }


def summarize(source_root: Path) -> dict[str, object]:
    image_like = []
    for path in sorted(source_root.glob("*.LZS")):
        if image_stream_offset(path.read_bytes()) is not None:
            palettes, frames, _ = parse_images(path)
            image_like.append(
                {
                    "source": path.name,
                    "palette_count": len(palettes),
                    "frame_count": len(frames),
                }
            )

    summary = {
        "source_root": str(source_root),
        "image_resources": image_like,
        "roads_present": (source_root / "ROADS.LZS").exists(),
        "trekdat": inspect_trekdat(source_root / "TREKDAT.LZS")
        if (source_root / "TREKDAT.LZS").exists()
        else None,
        "muzax": inspect_muzax(source_root / "MUZAX.LZS")
        if (source_root / "MUZAX.LZS").exists()
        else None,
        "sounds": inspect_sounds(source_root),
        "demo": inspect_demo(source_root),
        "dats": inspect_dashboard_dats(source_root),
        "exe": inspect_exe(source_root),
        "unknown_lzs": [
            path.name
            for path in sorted(source_root.glob("*.LZS"))
            if image_stream_offset(path.read_bytes()) is None
            and path.name not in {"ROADS.LZS", "TREKDAT.LZS", "MUZAX.LZS"}
        ],
    }
    print(json.dumps(summary, indent=2, sort_keys=True))
    return summary


def extract_all(source_root: Path, output_root: Path) -> None:
    image_summary = []
    for path in sorted(source_root.glob("*.LZS")):
        if path.name == "ROADS.LZS":
            continue
        if image_stream_offset(path.read_bytes()) is None:
            continue
        image_summary.append(export_image_resource(path, output_root / "images"))

    roads_summary = None
    roads_path = source_root / "ROADS.LZS"
    if roads_path.exists():
        roads_summary = export_roads(roads_path, output_root)

    trekdat_summary = None
    trekdat_path = source_root / "TREKDAT.LZS"
    if trekdat_path.exists():
        trekdat_summary = export_trekdat(trekdat_path, output_root)

    muzax_summary = None
    muzax_path = source_root / "MUZAX.LZS"
    if muzax_path.exists():
        muzax_summary = export_muzax(muzax_path, output_root)

    exe_summary = export_exe(source_root, output_root)
    sounds_summary = export_sounds(source_root, output_root)
    demo_summary = export_demo(source_root, output_root)
    dats_summary = export_dashboard_dats(source_root, output_root)

    manifest = {
        "source_root": str(source_root),
        "images": image_summary,
        "roads": roads_summary,
        "trekdat": trekdat_summary,
        "muzax": muzax_summary,
        "exe": exe_summary,
        "sounds": sounds_summary,
        "demo": demo_summary,
        "dats": dats_summary,
    }
    write_json(output_root / "manifest.json", manifest)
    print(json.dumps(manifest, indent=2, sort_keys=True))


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument(
        "--source",
        type=Path,
        default=Path("."),
        help="Directory containing original SkyRoads assets.",
    )
    common.add_argument(
        "--output",
        type=Path,
        default=Path("extracted"),
        help="Directory to write extracted assets into.",
    )
    subparsers.add_parser(
        "summary",
        parents=[common],
        help="Print a summary of recognized resource files.",
    )
    subparsers.add_parser(
        "extract",
        parents=[common],
        help="Extract documented image, road, renderer, music, sound, demo, DAT, and EXE resources.",
    )
    return parser


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()
    source_root = args.source.resolve()
    output_root = args.output.resolve()

    if args.command == "summary":
        summarize(source_root)
        return

    output_root.mkdir(parents=True, exist_ok=True)
    extract_all(source_root, output_root)


if __name__ == "__main__":
    main()
