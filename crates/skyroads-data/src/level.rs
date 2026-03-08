use crate::{RoadEntry, RoadsArchive, ROAD_COLUMNS};

pub const LEVEL_TILE_STRIDE_X: f64 = 46.0;
pub const LEVEL_CENTER_X: f64 = 0x8000 as f64 / 0x80 as f64;
pub const LEVEL_MIN_X: f64 = 0x2F80 as f64 / 0x80 as f64;
pub const LEVEL_MAX_X: f64 = 0xD080 as f64 / 0x80 as f64;
pub const GROUND_Y: f64 = 0x2800 as f64 / 0x80 as f64;

const EMPTY_COLLISION_MIN_Y: f64 = 0x1E80 as f64 / 0x80 as f64;
const EMPTY_COLLISION_MAX_Y: f64 = 80.0;
const TUNNEL_ENTRY_MIN_Y: f64 = 0x2180 as f64 / 0x80 as f64;
const TUNNEL_BASE_Y: f64 = 68.0;
const X_OFFSET: f64 = 95.0;
const PROBE_RADIUS_X: f64 = 14.0;

const TUNNEL_CEILS: [u8; 38] = [
    0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
    0x20, 0x1F, 0x1F, 0x1F, 0x1F, 0x1F, 0x1E, 0x1E, 0x1E, 0x1D, 0x1D, 0x1D, 0x1C, 0x1B, 0x1A, 0x19,
    0x18, 0x16, 0x14, 0x12, 0x11, 0x0E,
];
const TUNNEL_LOWS: [u8; 30] = [
    0x10, 0x10, 0x10, 0x10, 0x0F, 0x0E, 0x0D, 0x0B, 0x08, 0x07, 0x06, 0x05, 0x03, 0x03, 0x03, 0x03,
    0x03, 0x03, 0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchEffect {
    None,
    Accelerate,
    Decelerate,
    Kill,
    Slide,
    RefillOxygen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelCell {
    pub raw_descriptor: u16,
    pub color_index_low: u8,
    pub color_index_high: u8,
    pub flags: u8,
    pub has_tunnel: bool,
    pub has_tile: bool,
    pub tile_effect: TouchEffect,
    pub cube_height: Option<u16>,
    pub cube_effect: TouchEffect,
}

impl LevelCell {
    pub const EMPTY: Self = Self {
        raw_descriptor: 0,
        color_index_low: 0,
        color_index_high: 0,
        flags: 0,
        has_tunnel: false,
        has_tile: false,
        tile_effect: TouchEffect::None,
        cube_height: None,
        cube_effect: TouchEffect::None,
    };

    pub fn is_empty(&self) -> bool {
        !self.has_tunnel && !self.has_tile && self.cube_height.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Level {
    pub road_index: usize,
    pub name: String,
    pub gravity: u16,
    pub fuel: u16,
    pub oxygen: u16,
    pub cells: Vec<[LevelCell; ROAD_COLUMNS]>,
}

impl Level {
    pub fn width(&self) -> usize {
        ROAD_COLUMNS
    }

    pub fn length(&self) -> usize {
        self.cells.len()
    }

    pub fn gravity_acceleration(&self) -> f64 {
        -((self.gravity as f64 * 0x1680 as f64 / 0x190 as f64).floor()) / 0x80 as f64
    }

    pub fn row(&self, index: usize) -> Option<&[LevelCell; ROAD_COLUMNS]> {
        self.cells.get(index)
    }

    pub fn cell_at_indices(&self, x_index: usize, z_index: usize) -> LevelCell {
        self.cells
            .get(z_index)
            .and_then(|row| row.get(x_index))
            .copied()
            .unwrap_or(LevelCell::EMPTY)
    }

    pub fn get_cell(&self, x_pos: f64, _y_pos: f64, z_pos: f64) -> LevelCell {
        let mut x = x_pos - X_OFFSET;
        if !(0.0..=322.0).contains(&x) {
            return LevelCell::EMPTY;
        }

        let z = ((z_pos * 8.0).floor() / 8.0).floor();
        x /= LEVEL_TILE_STRIDE_X;
        let x_index = x.floor();
        if x_index < 0.0 || z < 0.0 {
            return LevelCell::EMPTY;
        }

        self.cell_at_indices(x_index as usize, z as usize)
    }

    pub fn is_inside_tile(&self, x_pos: f64, y_pos: f64, z_pos: f64) -> bool {
        let left_tile = self.get_cell(x_pos - PROBE_RADIUS_X, y_pos, z_pos);
        let right_tile = self.get_cell(x_pos + PROBE_RADIUS_X, y_pos, z_pos);

        if left_tile.is_empty() && right_tile.is_empty() {
            return false;
        }

        if y_pos < EMPTY_COLLISION_MAX_Y && y_pos > EMPTY_COLLISION_MIN_Y {
            return true;
        }

        if y_pos < TUNNEL_ENTRY_MIN_Y {
            return false;
        }

        let (distance_from_center, var_a) = distance_from_center(x_pos);
        let center_tile = self.get_cell(x_pos, y_pos, z_pos);
        if is_inside_tile_y(y_pos, distance_from_center, center_tile) {
            return true;
        }

        let adjacent_tile = self.get_cell(x_pos + var_a, y_pos, z_pos);
        is_inside_tile_y(y_pos, 47.0 - distance_from_center, adjacent_tile)
    }

    pub fn is_inside_tunnel(&self, x_pos: f64, y_pos: f64, z_pos: f64) -> bool {
        let left_tile = self.get_cell(x_pos - PROBE_RADIUS_X, y_pos, z_pos);
        let right_tile = self.get_cell(x_pos + PROBE_RADIUS_X, y_pos, z_pos);

        if left_tile.is_empty() && right_tile.is_empty() {
            return false;
        }

        let (distance_from_center, var_a) = distance_from_center(x_pos);
        let center_tile = self.get_cell(x_pos, y_pos, z_pos);
        if is_inside_tunnel_y(y_pos, distance_from_center, center_tile) {
            return true;
        }

        let adjacent_tile = self.get_cell(x_pos + var_a, y_pos, z_pos);
        is_inside_tunnel_y(y_pos, 47.0 - distance_from_center, adjacent_tile)
    }
}

pub fn level_from_road_entry(road: &RoadEntry) -> Level {
    let mut cells = Vec::with_capacity(road.rows.len());
    for row in &road.rows {
        cells.push(std::array::from_fn(|column| {
            cell_from_descriptor(row[column])
        }));
    }

    Level {
        road_index: road.index,
        name: if road.index == 0 {
            "Demo Level".to_string()
        } else {
            format!("Level {}", road.index)
        },
        gravity: road.gravity,
        fuel: road.fuel,
        oxygen: road.oxygen,
        cells,
    }
}

pub fn levels_from_roads_archive(roads: &RoadsArchive) -> Vec<Level> {
    roads.roads.iter().map(level_from_road_entry).collect()
}

fn cell_from_descriptor(raw_descriptor: u16) -> LevelCell {
    let color_raw = (raw_descriptor & 0x00FF) as u8;
    let flags = (raw_descriptor >> 8) as u8;
    let color_index_low = color_raw & 0x0F;
    let color_index_high = color_raw >> 4;

    let cube_height = match flags & 0x06 {
        0x00 => None,
        0x02 => Some(100),
        0x04 => Some(120),
        other => panic!("unexpected cube height flag bits: {other}"),
    };

    LevelCell {
        raw_descriptor,
        color_index_low,
        color_index_high,
        flags,
        has_tunnel: (flags & 0x01) != 0,
        has_tile: color_index_low > 0,
        tile_effect: effect_for_color_index(color_index_low),
        cube_height,
        cube_effect: effect_for_color_index(color_index_high),
    }
}

fn effect_for_color_index(index: u8) -> TouchEffect {
    match index {
        10 => TouchEffect::Accelerate,
        12 => TouchEffect::Kill,
        9 => TouchEffect::RefillOxygen,
        8 => TouchEffect::Slide,
        2 => TouchEffect::Decelerate,
        _ => TouchEffect::None,
    }
}

fn distance_from_center(x_pos: f64) -> (f64, f64) {
    let mut distance_from_center = 23.0 - ((x_pos - 49.0) % 46.0);
    let mut var_a = -46.0;
    if distance_from_center < 0.0 {
        distance_from_center = 1.0 - distance_from_center;
        var_a = -var_a;
    }
    (distance_from_center, var_a)
}

fn is_inside_tile_y(y_pos: f64, distance_from_center: f64, cell: LevelCell) -> bool {
    let distance_index = distance_from_center.round();
    if distance_index > 37.0 {
        return false;
    }
    let distance_index = distance_index as usize;
    let y2 = y_pos - TUNNEL_BASE_Y;
    match (cell.has_tunnel, cell.cube_height) {
        (true, None) => {
            y2 > TUNNEL_LOWS[distance_index] as f64 && y2 < TUNNEL_CEILS[distance_index] as f64
        }
        (false, Some(cube_height)) => y_pos < cube_height as f64,
        (true, Some(cube_height)) => {
            y2 > TUNNEL_LOWS[distance_index] as f64 && y_pos < cube_height as f64
        }
        (false, None) => false,
    }
}

fn is_inside_tunnel_y(y_pos: f64, distance_from_center: f64, cell: LevelCell) -> bool {
    let distance_index = distance_from_center.round();
    if distance_index > 29.0 {
        return false;
    }
    let distance_index = distance_index as usize;
    let y2 = y_pos - TUNNEL_BASE_Y;
    cell.has_tunnel && cell.has_tile && y2 < TUNNEL_LOWS[distance_index] as f64 && y_pos >= 80.0
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::load_roads_lzs_path;

    use super::{level_from_road_entry, levels_from_roads_archive, TouchEffect, GROUND_Y};

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn levels_match_known_rows_and_effects() {
        let roads = load_roads_lzs_path(repo_root().join("ROADS.LZS")).unwrap();
        let levels = levels_from_roads_archive(&roads);
        assert_eq!(levels.len(), 31);
        assert_eq!(levels[0].name, "Demo Level");
        assert_eq!(levels[0].gravity, 8);
        assert_eq!(levels[0].fuel, 130);
        assert_eq!(levels[0].oxygen, 60);
        assert_eq!(levels[0].length(), 160);

        let road0 = level_from_road_entry(&roads.roads[0]);
        let cell_a = road0.cells[83][0];
        assert_eq!(cell_a.raw_descriptor, 0x0400);
        assert_eq!(cell_a.cube_height, Some(120));
        assert!(!cell_a.has_tile);
        assert!(!cell_a.has_tunnel);

        let road2 = level_from_road_entry(&roads.roads[2]);
        let cell_b = road2.cells[80][0];
        assert_eq!(cell_b.raw_descriptor, 0x0507);
        assert_eq!(cell_b.cube_height, Some(120));
        assert!(cell_b.has_tile);
        assert!(cell_b.has_tunnel);
        assert_eq!(cell_b.tile_effect, TouchEffect::None);

        assert_eq!(road0.get_cell(95.0, GROUND_Y, 83.2).raw_descriptor, 0x0400);
        assert_eq!(
            road0
                .get_cell(95.0 + 46.0 * 6.0, GROUND_Y, 83.2)
                .raw_descriptor,
            0x0400
        );
    }
}
