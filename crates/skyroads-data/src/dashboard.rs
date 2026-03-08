use std::fs;
use std::path::Path;

use crate::image::{RgbColor, SCREEN_WIDTH};
use crate::{Error, Result};

pub const DASHBOARD_COLORS: [RgbColor; 3] = [
    RgbColor::new(0, 0, 0),
    RgbColor::new(97, 0, 93),
    RgbColor::new(113, 0, 101),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HudFragment {
    pub position: u16,
    pub x: u16,
    pub y: u16,
    pub width: u8,
    pub height: u8,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HudFragmentPack {
    pub source_len: usize,
    pub header_words: u16,
    pub fragments: Vec<HudFragment>,
}

impl HudFragmentPack {
    pub fn fragment_count(&self) -> usize {
        self.fragments.len()
    }
}

pub fn load_dashboard_dat_path(path: impl AsRef<Path>) -> Result<HudFragmentPack> {
    let data = fs::read(path)?;
    load_dashboard_dat_bytes(&data)
}

pub fn load_dashboard_dat_bytes(data: &[u8]) -> Result<HudFragmentPack> {
    if data.len() < 4 {
        return Err(Error::invalid_format(
            "dashboard DAT is too small to contain a header",
        ));
    }

    let probe = read_u16(data, 2)?;
    let header_words = if probe == 0x2C { 0x22 } else { 0x0A };
    let mut cursor = usize::from(header_words) * 2;
    if cursor > data.len() {
        return Err(Error::invalid_format(
            "dashboard DAT header exceeds source length",
        ));
    }

    let mut fragments = Vec::new();
    while cursor < data.len() {
        if cursor + 4 > data.len() {
            return Err(Error::invalid_format(
                "dashboard DAT fragment header is truncated",
            ));
        }
        let position = read_u16(data, cursor)?;
        let width = data[cursor + 2];
        let height = data[cursor + 3];
        cursor += 4;
        let pixel_count = usize::from(width) * usize::from(height);
        if cursor + pixel_count > data.len() {
            return Err(Error::invalid_format(
                "dashboard DAT fragment payload is truncated",
            ));
        }
        fragments.push(HudFragment {
            position,
            x: position % SCREEN_WIDTH,
            y: position / SCREEN_WIDTH,
            width,
            height,
            pixels: data[cursor..cursor + pixel_count].to_vec(),
        });
        cursor += pixel_count;
    }

    Ok(HudFragmentPack {
        source_len: data.len(),
        header_words,
        fragments,
    })
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or(Error::UnexpectedEof("u16"))?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::load_dashboard_dat_path;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn shipped_dashboard_packs_match_known_fragment_counts() {
        let oxy = load_dashboard_dat_path(repo_root().join("OXY_DISP.DAT")).unwrap();
        assert_eq!(oxy.header_words, 10);
        assert_eq!(oxy.fragment_count(), 10);

        let fuel = load_dashboard_dat_path(repo_root().join("FUL_DISP.DAT")).unwrap();
        assert_eq!(fuel.header_words, 10);
        assert_eq!(fuel.fragment_count(), 10);

        let speed = load_dashboard_dat_path(repo_root().join("SPEED.DAT")).unwrap();
        assert_eq!(speed.header_words, 34);
        assert_eq!(speed.fragment_count(), 34);
    }
}
