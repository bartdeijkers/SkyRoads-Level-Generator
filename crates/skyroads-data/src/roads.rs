use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::{decompress_stream, Error, Result};

pub const ROAD_COLUMNS: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoadDescriptor {
    pub raw: u16,
    pub low_byte: u8,
    pub high_byte: u8,
    pub dispatch_kind: u8,
    pub dispatch_variant_low3: u8,
    pub high_flags: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoadSample {
    pub road_index: usize,
    pub row_index: usize,
    pub column_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispatchSample {
    pub road_index: usize,
    pub row_index: usize,
    pub column_index: usize,
    pub raw: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoadDescriptorEntry {
    pub descriptor: RoadDescriptor,
    pub count: usize,
    pub roads: Vec<usize>,
    pub samples: Vec<RoadSample>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchKindEntry {
    pub dispatch_kind: u8,
    pub count: usize,
    pub roads: Vec<usize>,
    pub descriptor_count: usize,
    pub descriptors: Vec<u16>,
    pub samples: Vec<DispatchSample>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoadDescriptorCatalog {
    pub used_dispatch_kinds: Vec<u8>,
    pub dispatch_kinds: Vec<DispatchKindEntry>,
    pub descriptors: Vec<RoadDescriptorEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoadEntry {
    pub index: usize,
    pub offset: u16,
    pub compressed_size: usize,
    pub unpacked_size: u16,
    pub gravity: u16,
    pub fuel: u16,
    pub oxygen: u16,
    pub palette_vga: Vec<u8>,
    pub widths: [u8; 3],
    pub raw_tiles: Vec<u8>,
    pub rows: Vec<[u16; ROAD_COLUMNS]>,
    pub dispatch_kind_counts: BTreeMap<u8, usize>,
    pub descriptor_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoadsArchive {
    pub source_len: usize,
    pub roads: Vec<RoadEntry>,
    pub descriptor_catalog: RoadDescriptorCatalog,
}

impl RoadsArchive {
    pub fn road_count(&self) -> usize {
        self.roads.len()
    }

    pub fn used_dispatch_kinds(&self) -> &[u8] {
        &self.descriptor_catalog.used_dispatch_kinds
    }

    pub fn distinct_descriptor_count(&self) -> usize {
        self.descriptor_catalog.descriptors.len()
    }
}

pub fn analyze_road_descriptor(value: u16) -> RoadDescriptor {
    let low_byte = (value & 0x00FF) as u8;
    let high_byte = (value >> 8) as u8;
    RoadDescriptor {
        raw: value,
        low_byte,
        high_byte,
        dispatch_kind: high_byte & 0x0F,
        dispatch_variant_low3: high_byte & 0x07,
        high_flags: high_byte >> 4,
    }
}

pub fn load_roads_lzs_path(path: impl AsRef<Path>) -> Result<RoadsArchive> {
    let data = fs::read(path)?;
    load_roads_lzs_bytes(&data)
}

pub fn load_roads_lzs_bytes(data: &[u8]) -> Result<RoadsArchive> {
    let first_offset = read_u16(data, 0)?;
    if first_offset % 4 != 0 {
        return Err(Error::invalid_format(format!(
            "unexpected ROADS header size {first_offset}"
        )));
    }

    let entry_count = usize::from(first_offset / 4);
    let mut entries = Vec::with_capacity(entry_count);
    for index in 0..entry_count {
        let offset = read_u16(data, index * 4)?;
        let unpacked_size = read_u16(data, index * 4 + 2)?;
        entries.push((offset, unpacked_size));
    }

    let mut roads = Vec::with_capacity(entry_count);
    for (index, (offset, unpacked_size)) in entries.iter().copied().enumerate() {
        let next_offset = entries
            .get(index + 1)
            .map(|entry| usize::from(entry.0))
            .unwrap_or(data.len());
        let offset = usize::from(offset);
        if next_offset < offset || next_offset > data.len() {
            return Err(Error::invalid_format(format!(
                "road {index} has invalid slice bounds: start={offset} end={next_offset}"
            )));
        }

        let road_blob = &data[offset..next_offset];
        if road_blob.len() < 225 {
            return Err(Error::invalid_format(format!(
                "road {index} is too small to contain metadata and compression widths"
            )));
        }

        let gravity = read_u16(road_blob, 0)?;
        let fuel = read_u16(road_blob, 2)?;
        let oxygen = read_u16(road_blob, 4)?;
        let palette_vga = road_blob[6..222].to_vec();
        let widths = [road_blob[222], road_blob[223], road_blob[224]];
        let (raw_tiles, _) = decompress_stream(
            road_blob,
            225,
            Some(usize::from(unpacked_size)),
            (widths[0], widths[1], widths[2]),
        )?;

        if raw_tiles.len() % 2 != 0 {
            return Err(Error::invalid_format(format!(
                "road {index} decompressed to an odd number of bytes"
            )));
        }

        let values: Vec<u16> = raw_tiles
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        if values.len() % ROAD_COLUMNS != 0 {
            return Err(Error::invalid_format(format!(
                "road {index} decompressed to {} cells, not a multiple of {ROAD_COLUMNS}",
                values.len()
            )));
        }

        let mut rows = Vec::with_capacity(values.len() / ROAD_COLUMNS);
        for chunk in values.chunks_exact(ROAD_COLUMNS) {
            rows.push([
                chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6],
            ]);
        }

        let mut dispatch_kind_counts = BTreeMap::new();
        let mut descriptors = BTreeSet::new();
        for &value in &values {
            let analyzed = analyze_road_descriptor(value);
            *dispatch_kind_counts
                .entry(analyzed.dispatch_kind)
                .or_insert(0) += 1;
            descriptors.insert(value);
        }

        roads.push(RoadEntry {
            index,
            offset: offset as u16,
            compressed_size: road_blob.len(),
            unpacked_size,
            gravity,
            fuel,
            oxygen,
            palette_vga,
            widths,
            raw_tiles,
            rows,
            dispatch_kind_counts,
            descriptor_count: descriptors.len(),
        });
    }

    let descriptor_catalog = build_road_descriptor_catalog(&roads);
    Ok(RoadsArchive {
        source_len: data.len(),
        roads,
        descriptor_catalog,
    })
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    if offset + 2 > data.len() {
        return Err(Error::UnexpectedEof("u16"));
    }
    Ok(u16::from_le_bytes([data[offset], data[offset + 1]]))
}

fn build_road_descriptor_catalog(roads: &[RoadEntry]) -> RoadDescriptorCatalog {
    #[derive(Default)]
    struct DescriptorAccum {
        count: usize,
        roads: BTreeSet<usize>,
        samples: Vec<RoadSample>,
    }

    #[derive(Default)]
    struct DispatchAccum {
        count: usize,
        roads: BTreeSet<usize>,
        descriptors: BTreeSet<u16>,
        samples: Vec<DispatchSample>,
    }

    let mut descriptor_counts: BTreeMap<u16, DescriptorAccum> = BTreeMap::new();
    let mut dispatch_counts: BTreeMap<u8, DispatchAccum> = BTreeMap::new();

    for road in roads {
        for (row_index, row) in road.rows.iter().enumerate() {
            for (column_index, &value) in row.iter().enumerate() {
                let analyzed = analyze_road_descriptor(value);
                let descriptor_entry = descriptor_counts.entry(value).or_default();
                descriptor_entry.count += 1;
                descriptor_entry.roads.insert(road.index);
                if descriptor_entry.samples.len() < 8 {
                    descriptor_entry.samples.push(RoadSample {
                        road_index: road.index,
                        row_index,
                        column_index,
                    });
                }

                let dispatch_entry = dispatch_counts.entry(analyzed.dispatch_kind).or_default();
                dispatch_entry.count += 1;
                dispatch_entry.roads.insert(road.index);
                dispatch_entry.descriptors.insert(value);
                if dispatch_entry.samples.len() < 8 {
                    dispatch_entry.samples.push(DispatchSample {
                        road_index: road.index,
                        row_index,
                        column_index,
                        raw: value,
                    });
                }
            }
        }
    }

    let descriptors = descriptor_counts
        .into_iter()
        .map(|(raw, accum)| RoadDescriptorEntry {
            descriptor: analyze_road_descriptor(raw),
            count: accum.count,
            roads: accum.roads.into_iter().collect(),
            samples: accum.samples,
        })
        .collect::<Vec<_>>();

    let dispatch_kinds = dispatch_counts
        .into_iter()
        .map(|(dispatch_kind, accum)| DispatchKindEntry {
            dispatch_kind,
            count: accum.count,
            roads: accum.roads.into_iter().collect(),
            descriptor_count: accum.descriptors.len(),
            descriptors: accum.descriptors.into_iter().collect(),
            samples: accum.samples,
        })
        .collect::<Vec<_>>();

    let used_dispatch_kinds = dispatch_kinds
        .iter()
        .map(|entry| entry.dispatch_kind)
        .collect::<Vec<_>>();

    RoadDescriptorCatalog {
        used_dispatch_kinds,
        dispatch_kinds,
        descriptors,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{analyze_road_descriptor, load_roads_lzs_path};

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn descriptor_bitfields_match_verified_model() {
        let descriptor = analyze_road_descriptor(0x4A37);
        assert_eq!(descriptor.low_byte, 0x37);
        assert_eq!(descriptor.high_byte, 0x4A);
        assert_eq!(descriptor.dispatch_kind, 0x0A);
        assert_eq!(descriptor.dispatch_variant_low3, 0x02);
        assert_eq!(descriptor.high_flags, 0x04);
    }

    #[test]
    fn shipped_roads_match_verified_counts() {
        let roads = load_roads_lzs_path(repo_root().join("ROADS.LZS")).unwrap();
        assert_eq!(roads.road_count(), 31);
        assert_eq!(roads.used_dispatch_kinds(), &[0, 1, 2, 3, 4, 5]);
        assert_eq!(roads.distinct_descriptor_count(), 170);

        let counts = roads
            .descriptor_catalog
            .dispatch_kinds
            .iter()
            .map(|entry| (entry.dispatch_kind, entry.count))
            .collect::<Vec<_>>();
        assert_eq!(
            counts,
            vec![
                (0, 25_781),
                (1, 987),
                (2, 2_132),
                (3, 268),
                (4, 1_079),
                (5, 189),
            ]
        );
    }
}
