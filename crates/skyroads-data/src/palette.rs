//! Assembly of the DOS gameplay VGA palette.
//!
//! The active 256-entry DAC is a direct concatenation of four source palettes:
//! the selected road, ship, dashboard, and active world. DOS DAC captures for
//! playable Roads 1 and 5 prove both the order and exact component values.

use std::path::Path;

use crate::error::{Error, Result};
use crate::image::{load_image_archive_path, ImagePalette, RgbColor};
use crate::roads::{load_roads_lzs_path, RoadEntry};

/// Number of entries in a VGA DAC palette.
pub const VGA_PALETTE_SIZE: usize = 256;
/// Number of road indices at the start of the gameplay palette.
pub const ROAD_PALETTE_COLORS: usize = 72;
/// Number of ship indices loaded after the road bank.
pub const SHIP_PALETTE_COLORS: usize = 20;
/// First ship index in the active gameplay palette.
pub const SHIP_PALETTE_START: u8 = ROAD_PALETTE_COLORS as u8;
/// Number of dashboard indices loaded after the ship bank.
pub const DASHBOARD_PALETTE_COLORS: usize = 50;
/// First dashboard index in the active gameplay palette.
pub const DASHBOARD_PALETTE_START: u8 = (ROAD_PALETTE_COLORS + SHIP_PALETTE_COLORS) as u8;
/// Number of world indices in the final gameplay bank.
pub const WORLD_OVERLAY_COLORS: usize = 114;
/// First world index in the active gameplay palette.
pub const WORLD_PALETTE_START: u8 =
    (ROAD_PALETTE_COLORS + SHIP_PALETTE_COLORS + DASHBOARD_PALETTE_COLORS) as u8;

fn first_palette(archive_path: &Path) -> Result<ImagePalette> {
    let archive = load_image_archive_path(archive_path)?;
    archive
        .frames
        .iter()
        .flatten()
        .next()
        .map(|frame| frame.palette.clone())
        .ok_or_else(|| Error::invalid_format("image archive has no frames"))
}

fn world_index_for_road(road_index: usize) -> usize {
    if road_index == 0 {
        0
    } else {
        (road_index - 1) / 3
    }
}

fn road_palette(road: &RoadEntry) -> Result<Vec<RgbColor>> {
    let expected_bytes = ROAD_PALETTE_COLORS * 3;
    if road.palette_vga.len() != expected_bytes {
        return Err(Error::invalid_format(format!(
            "road {} palette has {} bytes, expected {expected_bytes}",
            road.index,
            road.palette_vga.len()
        )));
    }

    Ok(road
        .palette_vga
        .chunks_exact(3)
        .map(|rgb| RgbColor::new(rgb[0] * 4, rgb[1] * 4, rgb[2] * 4))
        .collect())
}

fn append_palette(
    destination: &mut Vec<RgbColor>,
    source: &ImagePalette,
    expected_colors: usize,
    source_name: &str,
) -> Result<()> {
    if source.colors.len() != expected_colors {
        return Err(Error::invalid_format(format!(
            "{source_name} palette has {} colors, expected {expected_colors}",
            source.colors.len()
        )));
    }
    destination.extend_from_slice(&source.colors);
    Ok(())
}

fn assemble_gameplay_palette(
    road: &RoadEntry,
    ship: &ImagePalette,
    dashboard: &ImagePalette,
    world: &ImagePalette,
) -> Result<Vec<RgbColor>> {
    let mut palette = road_palette(road)?;
    append_palette(&mut palette, ship, SHIP_PALETTE_COLORS, "ship")?;
    append_palette(
        &mut palette,
        dashboard,
        DASHBOARD_PALETTE_COLORS,
        "dashboard",
    )?;
    append_palette(&mut palette, world, WORLD_OVERLAY_COLORS, "world")?;

    if palette.len() != VGA_PALETTE_SIZE {
        return Err(Error::invalid_format(format!(
            "assembled gameplay palette has {} colors, expected {VGA_PALETTE_SIZE}",
            palette.len()
        )));
    }
    Ok(palette)
}

/// Build the exact DOS gameplay palette for every shipped road.
pub fn gameplay_palettes(source_root: impl AsRef<Path>) -> Result<Vec<Vec<RgbColor>>> {
    let root = source_root.as_ref();
    let roads = load_roads_lzs_path(root.join("ROADS.LZS"))?;
    let ship = first_palette(&root.join("CARS.LZS"))?;
    let dashboard = first_palette(&root.join("DASHBRD.LZS"))?;
    let worlds = (0..10)
        .map(|world_index| first_palette(&root.join(format!("WORLD{world_index}.LZS"))))
        .collect::<Result<Vec<_>>>()?;

    roads
        .roads
        .iter()
        .map(|road| {
            let world_index = world_index_for_road(road.index);
            let world = worlds.get(world_index).ok_or_else(|| {
                Error::invalid_format(format!(
                    "road {} maps to missing world {world_index}",
                    road.index
                ))
            })?;
            assemble_gameplay_palette(road, &ship, &dashboard, world)
        })
        .collect()
}

/// Build the exact DOS gameplay palette for one shipped road.
pub fn gameplay_palette(source_root: impl AsRef<Path>, road_index: usize) -> Result<Vec<RgbColor>> {
    gameplay_palettes(source_root)?
        .get(road_index)
        .cloned()
        .ok_or_else(|| Error::invalid_format(format!("missing road palette {road_index}")))
}
