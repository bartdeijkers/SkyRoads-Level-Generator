use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::{decompress_stream, Error, Result};

pub const TREKDAT_POINTER_ROWS: usize = 13;
pub const TREKDAT_POINTER_COLUMNS: usize = 24;
pub const TREKDAT_POINTER_COUNT: usize = TREKDAT_POINTER_ROWS * TREKDAT_POINTER_COLUMNS;
pub const TREKDAT_POINTER_TABLE_BYTES: usize = TREKDAT_POINTER_COUNT * 2;
pub const TREKDAT_DOS_DRAW_ROWS: usize = 11;
pub const TREKDAT_DOS_DRAW_COLUMNS: usize = 4;
pub const TREKDAT_DOS_POINTERS_PER_CELL: usize = 6;
pub const TREKDAT_SHAPE_ROWS: usize = 0x410;
pub const TREKDAT_SHAPE_BASE: u32 = 10240;
pub const TREKDAT_VIEWPORT_WIDTH: u16 = 320;
pub const TREKDAT_VIEWPORT_HEIGHT: u16 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrekdatSpan {
    pub x: u16,
    pub y: u16,
    pub width: u8,
    pub offset: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrekdatBbox {
    pub x0: u16,
    pub y0: u16,
    pub x1: u16,
    pub y1: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrekdatShape {
    pub start_offset: u16,
    pub size: usize,
    pub color: u8,
    pub base_ptr: u32,
    pub span_count: usize,
    pub nonzero_padding_count: usize,
    pub bbox: Option<TrekdatBbox>,
    pub spans: Vec<TrekdatSpan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrekdatCellPointers {
    pub pointers: [u16; TREKDAT_DOS_POINTERS_PER_CELL],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrekdatPointerRow {
    pub cells: [TrekdatCellPointers; TREKDAT_DOS_DRAW_COLUMNS],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrekdatDosPointerLayout {
    pub rows: [TrekdatPointerRow; TREKDAT_POINTER_ROWS],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrekdatRecord {
    pub index: usize,
    pub file_offset: usize,
    pub next_file_offset: usize,
    pub compressed_size: usize,
    pub load_buff_end: u16,
    pub bytes_to_read: u16,
    pub load_offset: usize,
    pub widths: [u8; 3],
    pub payload: Vec<u8>,
    pub expanded: Vec<u8>,
    pub pointer_table: Vec<u16>,
    pub shapes: BTreeMap<u16, TrekdatShape>,
}

impl TrekdatRecord {
    pub fn unique_pointer_count(&self) -> usize {
        self.shapes.len()
    }

    pub fn total_span_count(&self) -> usize {
        self.shapes.values().map(|shape| shape.span_count).sum()
    }

    pub fn pointer_min(&self) -> u16 {
        self.pointer_table.iter().copied().min().unwrap_or(0)
    }

    pub fn pointer_max(&self) -> u16 {
        self.pointer_table.iter().copied().max().unwrap_or(0)
    }

    pub fn dos_pointer_layout(&self) -> TrekdatDosPointerLayout {
        let rows = std::array::from_fn(|row_index| {
            let row_start = row_index * TREKDAT_POINTER_COLUMNS;
            let row_slice = &self.pointer_table[row_start..row_start + TREKDAT_POINTER_COLUMNS];
            let cells = std::array::from_fn(|cell_index| {
                let cell_start = cell_index * TREKDAT_DOS_POINTERS_PER_CELL;
                let mut pointers = [0u16; TREKDAT_DOS_POINTERS_PER_CELL];
                pointers.copy_from_slice(
                    &row_slice[cell_start..cell_start + TREKDAT_DOS_POINTERS_PER_CELL],
                );
                TrekdatCellPointers { pointers }
            });
            TrekdatPointerRow { cells }
        });
        TrekdatDosPointerLayout { rows }
    }

    pub fn shape_at_offset(&self, start_offset: u16) -> Option<TrekdatShape> {
        parse_trekdat_shape(&self.expanded, start_offset).ok()
    }

    pub fn next_shape_offset(&self, start_offset: u16) -> Option<u16> {
        let shape = self.shape_at_offset(start_offset)?;
        let next_offset = usize::from(shape.start_offset) + shape.size;
        if next_offset + 3 > self.expanded.len() {
            return None;
        }
        Some(next_offset as u16)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrekdatArchive {
    pub source_len: usize,
    pub records: Vec<TrekdatRecord>,
}

impl TrekdatArchive {
    pub fn record_count(&self) -> usize {
        self.records.len()
    }
}

pub fn load_trekdat_lzs_path(path: impl AsRef<Path>) -> Result<TrekdatArchive> {
    let data = fs::read(path)?;
    load_trekdat_lzs_bytes(&data)
}

pub fn load_trekdat_lzs_bytes(data: &[u8]) -> Result<TrekdatArchive> {
    if data.len() < 7 {
        return Err(Error::invalid_format(
            "TREKDAT.LZS is too small to contain the observed TREKDAT header",
        ));
    }

    let mut records = Vec::new();
    let mut offset = 0usize;
    let mut record_index = 0usize;

    while offset < data.len() {
        if offset + 7 > data.len() {
            return Err(Error::invalid_format(format!(
                "TREKDAT record {record_index} is truncated at 0x{offset:x}"
            )));
        }
        let load_buff_end = read_u16(data, offset)?;
        let bytes_to_read = read_u16(data, offset + 2)?;
        let widths = [data[offset + 4], data[offset + 5], data[offset + 6]];
        let (payload, consumed) = decompress_stream(
            data,
            offset + 7,
            Some(usize::from(bytes_to_read)),
            (widths[0], widths[1], widths[2]),
        )?;
        let (expanded, load_offset) = expand_trekdat_record(load_buff_end, &payload)?;
        let pointer_table = parse_pointer_table(&expanded, record_index)?;
        let mut shapes = BTreeMap::new();
        for &start_offset in &pointer_table {
            shapes
                .entry(start_offset)
                .or_insert(parse_trekdat_shape(&expanded, start_offset)?);
        }
        let next_file_offset = offset + 7 + consumed;
        records.push(TrekdatRecord {
            index: record_index,
            file_offset: offset,
            next_file_offset,
            compressed_size: next_file_offset - offset,
            load_buff_end,
            bytes_to_read,
            load_offset,
            widths,
            payload,
            expanded,
            pointer_table,
            shapes,
        });
        offset = next_file_offset;
        record_index += 1;
    }

    Ok(TrekdatArchive {
        source_len: data.len(),
        records,
    })
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    if offset + 2 > data.len() {
        return Err(Error::UnexpectedEof("u16"));
    }
    Ok(u16::from_le_bytes([data[offset], data[offset + 1]]))
}

fn expand_trekdat_record(load_buff_end: u16, payload: &[u8]) -> Result<(Vec<u8>, usize)> {
    let load_buff_end = usize::from(load_buff_end);
    let load_offset = load_buff_end.checked_sub(payload.len()).ok_or_else(|| {
        Error::invalid_format(format!(
            "TREKDAT record expands to a negative load offset: load_buff_end={load_buff_end} payload_len={}",
            payload.len()
        ))
    })?;

    let mut working = vec![0_u8; load_offset];
    working.extend_from_slice(payload);
    let mut src_ptr = load_offset;
    let mut output = Vec::with_capacity(load_buff_end);

    if src_ptr + TREKDAT_POINTER_TABLE_BYTES > working.len() {
        return Err(Error::invalid_format(
            "TREKDAT record is too short for its pointer table",
        ));
    }
    output.extend_from_slice(&working[src_ptr..src_ptr + TREKDAT_POINTER_TABLE_BYTES]);
    src_ptr += TREKDAT_POINTER_TABLE_BYTES;

    for _ in 0..TREKDAT_SHAPE_ROWS {
        if src_ptr + 3 > working.len() {
            return Err(Error::invalid_format(
                "TREKDAT record ended while copying shape headers",
            ));
        }
        output.extend_from_slice(&working[src_ptr..src_ptr + 3]);
        src_ptr += 3;

        loop {
            if src_ptr >= working.len() {
                return Err(Error::invalid_format(
                    "TREKDAT record ended while copying shape spans",
                ));
            }
            let value = working[src_ptr];
            output.push(value);
            src_ptr += 1;
            if value == 0xFF {
                break;
            }
            if src_ptr >= working.len() {
                return Err(Error::invalid_format(
                    "TREKDAT record ended while copying span width",
                ));
            }
            output.push(working[src_ptr]);
            src_ptr += 1;
            output.push(0);
        }
    }

    if output.len() != load_buff_end {
        return Err(Error::invalid_format(format!(
            "TREKDAT expanded size mismatch: expected {load_buff_end}, produced {}",
            output.len()
        )));
    }

    Ok((output, load_offset))
}

fn parse_pointer_table(expanded: &[u8], record_index: usize) -> Result<Vec<u16>> {
    let mut pointer_table = Vec::with_capacity(TREKDAT_POINTER_COUNT);
    for entry_offset in (0..TREKDAT_POINTER_TABLE_BYTES).step_by(2) {
        let pointer = read_u16(expanded, entry_offset)?;
        if usize::from(pointer) < TREKDAT_POINTER_TABLE_BYTES {
            return Err(Error::invalid_format(format!(
                "TREKDAT record {record_index} contains a pointer into the table area"
            )));
        }
        if usize::from(pointer) >= expanded.len() {
            return Err(Error::invalid_format(format!(
                "TREKDAT record {record_index} contains an out-of-range pointer"
            )));
        }
        pointer_table.push(pointer);
    }
    Ok(pointer_table)
}

fn parse_trekdat_shape(expanded: &[u8], start_offset: u16) -> Result<TrekdatShape> {
    let start_offset_usize = usize::from(start_offset);
    if start_offset_usize + 3 > expanded.len() {
        return Err(Error::invalid_format(format!(
            "TREKDAT shape offset 0x{start_offset:x} is out of range"
        )));
    }

    let color = expanded[start_offset_usize];
    let base_ptr = TREKDAT_SHAPE_BASE + u32::from(read_u16(expanded, start_offset_usize + 1)?);
    let mut cursor = start_offset_usize + 3;
    let mut ptr = base_ptr;
    let mut spans = Vec::new();
    let mut min_x = TREKDAT_VIEWPORT_WIDTH;
    let mut max_x = 0_u16;
    let mut min_y = TREKDAT_VIEWPORT_HEIGHT;
    let mut max_y = 0_u16;
    let mut saw_any_span = false;
    let mut nonzero_padding_count = 0usize;

    loop {
        if cursor >= expanded.len() {
            return Err(Error::invalid_format(format!(
                "TREKDAT shape at 0x{start_offset:x} is truncated"
            )));
        }
        let offset = expanded[cursor];
        cursor += 1;
        if offset == 0xFF {
            break;
        }
        if cursor + 2 > expanded.len() {
            return Err(Error::invalid_format(format!(
                "TREKDAT span at 0x{start_offset:x} is truncated"
            )));
        }
        let width = expanded[cursor];
        let padding = expanded[cursor + 1];
        cursor += 2;
        if padding != 0 {
            nonzero_padding_count += 1;
        }
        let ptr2 = ptr.checked_sub(u32::from(offset)).ok_or_else(|| {
            Error::invalid_format(format!(
                "TREKDAT span underflow at shape 0x{start_offset:x}: ptr={ptr} offset={offset}"
            ))
        })?;
        let x = (ptr2 % u32::from(TREKDAT_VIEWPORT_WIDTH)) as u16;
        let y = (ptr2 / u32::from(TREKDAT_VIEWPORT_WIDTH)) as u16;
        spans.push(TrekdatSpan {
            x,
            y,
            width,
            offset,
        });
        if width > 0 {
            let x1 = x + u16::from(width) - 1;
            if !saw_any_span {
                min_x = x;
                max_x = x1;
                min_y = y;
                max_y = y;
                saw_any_span = true;
            } else {
                min_x = min_x.min(x);
                max_x = max_x.max(x1);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
        ptr += u32::from(TREKDAT_VIEWPORT_WIDTH);
    }

    let bbox = if saw_any_span {
        Some(TrekdatBbox {
            x0: min_x,
            y0: min_y,
            x1: max_x,
            y1: max_y,
            width: max_x - min_x + 1,
            height: max_y - min_y + 1,
        })
    } else {
        None
    };

    Ok(TrekdatShape {
        start_offset,
        size: cursor - start_offset_usize,
        color,
        base_ptr,
        span_count: spans.len(),
        nonzero_padding_count,
        bbox,
        spans,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::load_trekdat_lzs_path;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn shipped_trekdat_matches_verified_layout() {
        let trekdat = load_trekdat_lzs_path(repo_root().join("TREKDAT.LZS")).unwrap();
        assert_eq!(trekdat.record_count(), 8);

        let expanded_sizes = trekdat
            .records
            .iter()
            .map(|record| record.load_buff_end)
            .collect::<Vec<_>>();
        assert_eq!(
            expanded_sizes,
            vec![24716, 25775, 26324, 26702, 27278, 26780, 26399, 26153]
        );

        let compressed_sizes = trekdat
            .records
            .iter()
            .map(|record| record.compressed_size)
            .collect::<Vec<_>>();
        assert_eq!(
            compressed_sizes,
            vec![11368, 12190, 12397, 12376, 12592, 12502, 12403, 12320]
        );

        let unique_pointer_counts = trekdat
            .records
            .iter()
            .map(|record| record.unique_pointer_count())
            .collect::<Vec<_>>();
        assert_eq!(
            unique_pointer_counts,
            vec![312, 312, 312, 312, 312, 312, 312, 312]
        );

        let total_span_counts = trekdat
            .records
            .iter()
            .map(|record| record.total_span_count())
            .collect::<Vec<_>>();
        assert_eq!(
            total_span_counts,
            vec![1788, 1855, 1886, 1935, 1989, 1985, 1963, 1910]
        );

        let layout = trekdat.records[0].dos_pointer_layout();
        assert_eq!(
            layout.rows[0].cells[0].pointers,
            [624, 636, 640, 648, 660, 692]
        );
        assert_eq!(
            layout.rows[11].cells[0].pointers,
            [19939, 20146, 20201, 20377, 20560, 20997]
        );
        assert_eq!(
            layout.rows[12].cells[3].pointers,
            [24636, 24648, 24652, 24660, 24672, 24704]
        );
        let first_shape = trekdat.records[0].shape_at_offset(944).unwrap();
        assert_eq!(first_shape.color, 1);
        assert_eq!(first_shape.span_count, 3);
        let next_shape = trekdat.records[0]
            .shape_at_offset(trekdat.records[0].next_shape_offset(944).unwrap())
            .unwrap();
        assert_eq!(next_shape.color, 31);
    }
}
