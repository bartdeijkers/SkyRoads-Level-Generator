use std::fs;
use std::path::Path;

use crate::{Error, Result};

pub const EXE_READER_SEGMENT_BASE: u16 = 0x66E;
const RUNTIME_TILE_CLASS_OFFSET: u16 = 0x0B77;
const RUNTIME_DISPATCH_OFFSET: u16 = 0x0B7F;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExeRelocation {
    pub index: usize,
    pub offset: u16,
    pub segment: u16,
    pub image_offset: usize,
    pub file_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExeRuntimeU8Table {
    pub offset: u16,
    pub image_offset: usize,
    pub file_offset: usize,
    pub values: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExeDispatchEntry {
    pub index: usize,
    pub target: u16,
    pub target_label: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExeRuntimeDispatchTable {
    pub offset: u16,
    pub image_offset: usize,
    pub file_offset: usize,
    pub entries: Vec<ExeDispatchEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExeRuntimeTables {
    pub tile_class_by_low3: ExeRuntimeU8Table,
    pub draw_dispatch_by_type: ExeRuntimeDispatchTable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyroadsExe {
    pub declared_file_size: usize,
    pub header_bytes: usize,
    pub image_size: usize,
    pub relocation_count: u16,
    pub min_alloc: u16,
    pub max_alloc: u16,
    pub ss: u16,
    pub sp: u16,
    pub checksum: u16,
    pub ip: u16,
    pub cs: u16,
    pub relocation_table_offset: u16,
    pub overlay: u16,
    pub relocations: Vec<ExeRelocation>,
    pub entry_image_offset: usize,
    pub entry_file_offset: usize,
    pub exe_reader_base_image_offset: usize,
    pub exe_reader_base_file_offset: usize,
    pub image: Vec<u8>,
    pub runtime_tables: ExeRuntimeTables,
}

pub fn load_skyroads_exe_path(path: impl AsRef<Path>) -> Result<SkyroadsExe> {
    let data = fs::read(path)?;
    load_skyroads_exe_bytes(&data)
}

pub fn load_skyroads_exe_bytes(data: &[u8]) -> Result<SkyroadsExe> {
    if data.len() < 28 || &data[..2] != b"MZ" {
        return Err(Error::invalid_format(
            "SKYROADS.EXE is not a recognized MZ executable",
        ));
    }

    let last_page_bytes = read_u16(data, 2)?;
    let pages = read_u16(data, 4)?;
    let relocation_count = read_u16(data, 6)?;
    let header_paragraphs = read_u16(data, 8)?;
    let min_alloc = read_u16(data, 10)?;
    let max_alloc = read_u16(data, 12)?;
    let ss = read_u16(data, 14)?;
    let sp = read_u16(data, 16)?;
    let checksum = read_u16(data, 18)?;
    let ip = read_u16(data, 20)?;
    let cs = read_u16(data, 22)?;
    let relocation_table_offset = read_u16(data, 24)?;
    let overlay = read_u16(data, 26)?;

    let declared_file_size = (usize::from(pages) - 1) * 512
        + usize::from(if last_page_bytes == 0 {
            512
        } else {
            last_page_bytes
        });
    let header_bytes = usize::from(header_paragraphs) * 16;
    if declared_file_size > data.len() {
        return Err(Error::invalid_format(format!(
            "SKYROADS.EXE header declares {declared_file_size} bytes, but file is only {} bytes",
            data.len()
        )));
    }
    if header_bytes > declared_file_size {
        return Err(Error::invalid_format(format!(
            "SKYROADS.EXE header is larger than declared file size: {header_bytes} > {declared_file_size}"
        )));
    }
    let image_size = declared_file_size - header_bytes;
    let image = data[header_bytes..header_bytes + image_size].to_vec();

    let mut relocations = Vec::with_capacity(usize::from(relocation_count));
    for index in 0..usize::from(relocation_count) {
        let entry_offset = usize::from(relocation_table_offset) + index * 4;
        if entry_offset + 4 > data.len() {
            return Err(Error::invalid_format(format!(
                "SKYROADS.EXE relocation {index} is truncated"
            )));
        }
        let offset = read_u16(data, entry_offset)?;
        let segment = read_u16(data, entry_offset + 2)?;
        let image_offset = usize::from(segment) * 16 + usize::from(offset);
        relocations.push(ExeRelocation {
            index,
            offset,
            segment,
            image_offset,
            file_offset: header_bytes + image_offset,
        });
    }

    let entry_image_offset = usize::from(cs) * 16 + usize::from(ip);
    let entry_file_offset = header_bytes + entry_image_offset;
    let exe_reader_base_image_offset = usize::from(EXE_READER_SEGMENT_BASE) * 16;
    let exe_reader_base_file_offset = header_bytes + exe_reader_base_image_offset;
    let runtime_tables = extract_runtime_tables(data, header_bytes, exe_reader_base_file_offset)?;

    Ok(SkyroadsExe {
        declared_file_size,
        header_bytes,
        image_size,
        relocation_count,
        min_alloc,
        max_alloc,
        ss,
        sp,
        checksum,
        ip,
        cs,
        relocation_table_offset,
        overlay,
        relocations,
        entry_image_offset,
        entry_file_offset,
        exe_reader_base_image_offset,
        exe_reader_base_file_offset,
        image,
        runtime_tables,
    })
}

fn extract_runtime_tables(
    data: &[u8],
    header_bytes: usize,
    exe_reader_base_file_offset: usize,
) -> Result<ExeRuntimeTables> {
    let tile_class_by_low3 = {
        let offset = RUNTIME_TILE_CLASS_OFFSET;
        let file_offset = exe_reader_base_file_offset + usize::from(offset);
        let end = file_offset + 8;
        if end > data.len() {
            return Err(Error::invalid_format(
                "tile_class_by_low3 table extends past SKYROADS.EXE",
            ));
        }
        ExeRuntimeU8Table {
            offset,
            image_offset: file_offset - header_bytes,
            file_offset,
            values: data[file_offset..end].to_vec(),
        }
    };

    let draw_dispatch_by_type = {
        let offset = RUNTIME_DISPATCH_OFFSET;
        let file_offset = exe_reader_base_file_offset + usize::from(offset);
        let end = file_offset + (16 * 2);
        if end > data.len() {
            return Err(Error::invalid_format(
                "draw_dispatch_by_type table extends past SKYROADS.EXE",
            ));
        }
        let mut entries = Vec::with_capacity(16);
        for index in 0..16 {
            let target = read_u16(data, file_offset + index * 2)?;
            entries.push(ExeDispatchEntry {
                index,
                target,
                target_label: dispatch_label(target),
            });
        }
        ExeRuntimeDispatchTable {
            offset,
            image_offset: file_offset - header_bytes,
            file_offset,
            entries,
        }
    };

    Ok(ExeRuntimeTables {
        tile_class_by_low3,
        draw_dispatch_by_type,
    })
}

fn dispatch_label(target: u16) -> Option<&'static str> {
    match target {
        0x2E50 => Some("draw_type_0"),
        0x303D => Some("draw_type_1"),
        0x2E9F => Some("draw_type_2"),
        0x2EE1 => Some("draw_type_3"),
        0x2F3C => Some("draw_type_4"),
        0x2FB0 => Some("draw_type_5"),
        0x3AAD => Some("noop"),
        _ => None,
    }
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    if offset + 2 > data.len() {
        return Err(Error::UnexpectedEof("u16"));
    }
    Ok(u16::from_le_bytes([data[offset], data[offset + 1]]))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::load_skyroads_exe_path;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn shipped_exe_matches_verified_runtime_tables() {
        let exe = load_skyroads_exe_path(repo_root().join("SKYROADS.EXE")).unwrap();
        assert_eq!(exe.header_bytes, 512);
        assert_eq!(exe.image_size, 29960);
        assert_eq!(exe.entry_file_offset, 25296);
        assert_eq!(exe.exe_reader_base_file_offset, 26848);
        assert_eq!(exe.relocations.len(), 2);
        assert_eq!(exe.relocations[0].file_offset, 15534);
        assert_eq!(exe.relocations[1].file_offset, 25297);
        assert_eq!(
            exe.runtime_tables.tile_class_by_low3.values,
            vec![1, 2, 3, 3, 4, 4, 1, 1]
        );
        let dispatch_targets = exe
            .runtime_tables
            .draw_dispatch_by_type
            .entries
            .iter()
            .map(|entry| entry.target)
            .collect::<Vec<_>>();
        assert_eq!(
            dispatch_targets,
            vec![
                0x2E50, 0x303D, 0x2E9F, 0x2EE1, 0x2F3C, 0x2FB0, 0x3AAD, 0x3AAD, 0x3AAD, 0x3AAD,
                0x3AAD, 0x3AAD, 0x3AAD, 0x3AAD, 0x3AAD, 0x3AAD,
            ]
        );
    }
}
