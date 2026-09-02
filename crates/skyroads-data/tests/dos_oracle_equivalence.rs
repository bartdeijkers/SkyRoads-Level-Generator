//! Equivalence checks that lock the native shipped runtime tables to ground
//! truth captured from the original `SKYROADS.EXE` under DOSBox-X.
//!
//! The fixture is produced by `tools/skyroads_dos_oracle.py` and committed at
//! `fixtures/dos-gameplay-renderer/<preset>/<checkpoint>/fixture.json`. Reading
//! it here (instead of hardcoding values) means the native tables and the DOS
//! capture cannot silently drift apart.

use std::path::PathBuf;

use serde_json::Value;
use skyroads_data::{
    gameplay_palette, gameplay_palettes, load_image_archive_path, load_roads_lzs_path,
    load_trekdat_lzs_path, shipped_runtime_tables, DASHBOARD_PALETTE_START, SHIP_PALETTE_START,
    WORLD_PALETTE_START,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/skyroads-data; the fixtures live at repo root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/dos-gameplay-renderer")
}

fn load_fixture(preset: &str, checkpoint: &str) -> Value {
    let path = fixture_root()
        .join(preset)
        .join(checkpoint)
        .join("fixture.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read fixture {}: {err}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|err| panic!("failed to parse fixture {}: {err}", path.display()))
}

/// Find the named dump object inside a captured checkpoint fixture.
fn dump<'a>(fixture: &'a Value, dump_name: &str) -> &'a Value {
    fixture["dumps"]
        .as_array()
        .expect("fixture.dumps must be an array")
        .iter()
        .find(|dump| dump["name"] == dump_name)
        .unwrap_or_else(|| panic!("fixture is missing the `{dump_name}` dump"))
}

fn u64_array(value: &Value, field: &str) -> Vec<u64> {
    value[field]
        .as_array()
        .unwrap_or_else(|| panic!("dump field `{field}` must be an array"))
        .iter()
        .map(|item| {
            item.as_u64()
                .expect("array entry must be a non-negative integer")
        })
        .collect()
}

/// Checkpoints whose renderer-input tables must equal the native shipped tables.
/// `renderer_entry` is captured at road-draw entry (its frame is the pre-draw
/// menu, but its tables are valid), and `frame_02` is a real, cross-run-stable
/// Road 0 gameplay frame. Both must agree with the native tables.
const TABLE_CHECKPOINTS: &[(&str, &str)] = &[
    ("road0-initial-frame", "renderer_entry"),
    ("road0-first-five-renderer-frames", "frame_02"),
];

#[test]
fn shipped_tile_class_table_matches_dos_capture() {
    let native: Vec<u64> = shipped_runtime_tables()
        .tile_class_by_low3
        .values
        .iter()
        .map(|&v| v as u64)
        .collect();

    for (preset, checkpoint) in TABLE_CHECKPOINTS {
        let fixture = load_fixture(preset, checkpoint);
        let dos_values = u64_array(dump(&fixture, "tile_class_by_low3"), "values");
        assert_eq!(
            native, dos_values,
            "native tile_class_by_low3 must match the DOS capture at {preset}/{checkpoint}"
        );
    }
}

#[test]
fn shipped_draw_dispatch_targets_match_dos_capture() {
    let native: Vec<u64> = shipped_runtime_tables()
        .draw_dispatch_by_type
        .entries
        .iter()
        .map(|entry| entry.target as u64)
        .collect();

    for (preset, checkpoint) in TABLE_CHECKPOINTS {
        let fixture = load_fixture(preset, checkpoint);
        let dos_targets = u64_array(dump(&fixture, "draw_dispatch_by_type"), "targets");
        assert_eq!(
            native, dos_targets,
            "native draw_dispatch_by_type targets must match the DOS capture at {preset}/{checkpoint} (kinds 0..5 plus the shared noop)"
        );
    }
}

/// Lock the cross-run-deterministic property used to trust the gameplay-frame
/// ground truth: the early Road 0 road frame is steady, so `frame_02` and
/// `frame_03` hash identically. Both runs that produced these fixtures agreed.
#[test]
fn road0_steady_gameplay_frame_is_stable() {
    let f2 = load_fixture("road0-first-five-renderer-frames", "frame_02");
    let f3 = load_fixture("road0-first-five-renderer-frames", "frame_03");
    let h2 = f2["frame_sha256"].as_str().expect("frame_02 frame_sha256");
    let h3 = f3["frame_sha256"].as_str().expect("frame_03 frame_sha256");
    assert_eq!(
        h2, h3,
        "the steady early Road 0 frame must hash identically at frame_02 and frame_03"
    );
    // Distinct from the pre-draw menu frame captured at renderer entry.
    let menu = load_fixture("road0-initial-frame", "renderer_entry");
    let menu_hash = menu["frame_sha256"].as_str().expect("menu frame_sha256");
    assert_ne!(
        h2, menu_hash,
        "the gameplay frame must differ from the pre-draw level-select frame"
    );
}

/// The three-run deterministic Road 0 gameplay capture must contain a complete
/// 256-entry, 6-bit VGA DAC palette.
#[test]
fn recovered_gameplay_palette_is_well_formed() {
    let path = fixture_root().join("road0-steady-neutral/steady-neutral/palette.vga6");
    let palette = std::fs::read(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    assert_eq!(palette.len(), 256 * 3);
    for (component_index, component) in palette.into_iter().enumerate() {
        assert!(
            component <= 63,
            "palette component {component_index} value {component} exceeds 6-bit VGA range"
        );
    }
}

/// Native Road 1 presentation must use the exact active DOS DAC palette.
#[test]
fn native_gameplay_palette_matches_dos_for_road1() {
    let path = fixture_root().join("road0-steady-neutral/steady-neutral/palette.vga6");
    let dos = std::fs::read(path).expect("read captured palette");
    let native = gameplay_palette(repo_root(), 1).expect("assemble native gameplay palette");

    for (index, dos_rgb) in dos.chunks_exact(3).enumerate() {
        let dos_6bit = [dos_rgb[0], dos_rgb[1], dos_rgb[2]];
        let native_6bit = [
            native[index].r / 4,
            native[index].g / 4,
            native[index].b / 4,
        ];
        assert_eq!(
            native_6bit, dos_6bit,
            "native gameplay palette index {index} must match the recovered DOS palette"
        );
    }
}

/// The Road 5 live DAC capture proves the palette concatenation for a second
/// road and world, independently of the Road 1 calibration fixture.
#[test]
fn native_gameplay_palette_matches_dos_for_road5() {
    let path = fixture_root().join("road5-dispatch-kinds2-4/dispatch-kinds2-4/palette.vga6");
    let dos = std::fs::read(path).expect("read Road 5 captured palette");
    let native = gameplay_palette(repo_root(), 5).expect("assemble Road 5 gameplay palette");

    let native_6bit = native
        .iter()
        .flat_map(|color| [color.r / 4, color.g / 4, color.b / 4])
        .collect::<Vec<_>>();
    assert_eq!(native_6bit, dos);
}

#[test]
fn every_shipped_gameplay_palette_preserves_its_dos_source_banks() {
    let root = repo_root();
    let roads = load_roads_lzs_path(root.join("ROADS.LZS")).expect("load roads");
    let palettes = gameplay_palettes(&root).expect("assemble all gameplay palettes");
    let ship = load_image_archive_path(root.join("CARS.LZS"))
        .expect("load ship art")
        .frames[0][0]
        .palette
        .clone();
    let dashboard = load_image_archive_path(root.join("DASHBRD.LZS"))
        .expect("load dashboard art")
        .frames[0][0]
        .palette
        .clone();

    assert_eq!(palettes.len(), roads.roads.len());
    for (road_index, (road, palette)) in roads.roads.iter().zip(&palettes).enumerate() {
        let world_index = if road_index == 0 {
            0
        } else {
            (road_index - 1) / 3
        };
        let world = load_image_archive_path(root.join(format!("WORLD{world_index}.LZS")))
            .unwrap_or_else(|error| panic!("load world {world_index}: {error}"));
        let world_palette = &world.frames[0][0].palette;
        let road_colors = road
            .palette_vga
            .chunks_exact(3)
            .map(|rgb| skyroads_data::RgbColor::new(rgb[0] * 4, rgb[1] * 4, rgb[2] * 4))
            .collect::<Vec<_>>();

        assert_eq!(&palette[..SHIP_PALETTE_START as usize], road_colors);
        assert_eq!(
            &palette[SHIP_PALETTE_START as usize..DASHBOARD_PALETTE_START as usize],
            ship.colors
        );
        assert_eq!(
            &palette[DASHBOARD_PALETTE_START as usize..WORLD_PALETTE_START as usize],
            dashboard.colors
        );
        assert_eq!(
            &palette[WORLD_PALETTE_START as usize..],
            world_palette.colors
        );
    }
}

/// The native road loader must reproduce the exact road-descriptor window the DOS
/// renderer reads at gameplay start. The captured Road 0 session is the menu
/// default "Road 1" (native road index 1); at `current_row 0` the DOS renderer
/// reads the row-0 descriptor window (`active_road_window`). This locks the
/// road-data pipeline that feeds the renderer to DOS ground truth.
#[test]
fn native_road_row0_matches_dos_active_road_window() {
    const CAPTURED_ROAD_INDEX: usize = 1;

    let fixture = load_fixture("road0-initial-frame", "renderer_entry");
    let dos_words = u64_array(dump(&fixture, "active_road_window"), "word_values");

    let roads = load_roads_lzs_path(repo_root().join("ROADS.LZS")).expect("load ROADS.LZS");
    let native: Vec<u64> = roads.roads[CAPTURED_ROAD_INDEX].rows[0]
        .iter()
        .map(|&w| w as u64)
        .collect();

    assert_eq!(
        native, dos_words,
        "native road {CAPTURED_ROAD_INDEX} row 0 must match the DOS active road window"
    );
}

/// The native TREKDAT expansion must reproduce the DOS renderer's active pointer
/// grid. At the captured checkpoint the renderer's TREKDAT slot is 0, whose
/// expanded record carries a 312-word pointer table; this must match the captured
/// `active_trekdat_pointer_grid` (head words + nonzero count). This locks the
/// TREKDAT pipeline that feeds the road rasterizer to DOS ground truth.
#[test]
fn native_trekdat_pointer_grid_matches_dos_slot0() {
    const CAPTURED_SLOT: usize = 0;

    let fixture = load_fixture("road0-initial-frame", "renderer_entry");
    let grid = dump(&fixture, "active_trekdat_pointer_grid");
    let dos_head = u64_array(grid, "first_pointer_words");
    let dos_nonzero = grid["nonzero_pointer_count"]
        .as_u64()
        .expect("nonzero count");

    let trekdat = load_trekdat_lzs_path(repo_root().join("TREKDAT.LZS")).expect("load TREKDAT.LZS");
    let pointer_table = &trekdat.records[CAPTURED_SLOT].pointer_table;

    let native_head: Vec<u64> = pointer_table
        .iter()
        .take(dos_head.len())
        .map(|&p| p as u64)
        .collect();
    let native_nonzero = pointer_table.iter().filter(|&&p| p != 0).count() as u64;

    assert_eq!(
        native_head, dos_head,
        "native TREKDAT slot {CAPTURED_SLOT} pointer grid head must match the DOS capture"
    );
    assert_eq!(
        native_nonzero, dos_nonzero,
        "native TREKDAT slot {CAPTURED_SLOT} nonzero pointer count must match the DOS capture"
    );
}
