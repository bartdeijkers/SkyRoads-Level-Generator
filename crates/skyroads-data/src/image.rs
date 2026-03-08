use std::fs;
use std::path::Path;

use crate::{decompress_stream, Error, Result};

pub const SCREEN_WIDTH: u16 = 320;
pub const SCREEN_HEIGHT: u16 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePalette {
    pub colors: Vec<RgbColor>,
    pub aux_data: Vec<u8>,
}

impl ImagePalette {
    pub fn color_count(&self) -> usize {
        self.colors.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFrame {
    pub offset: u16,
    pub x_offset: u16,
    pub y_offset: u16,
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
    pub palette: ImagePalette,
    pub transparent_zero: bool,
}

impl ImageFrame {
    pub fn pixel_count(&self) -> usize {
        self.pixels.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageArchiveKind {
    ImageSet,
    Animation { declared_frame_count: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageArchive {
    pub source_len: usize,
    pub kind: ImageArchiveKind,
    pub frames: Vec<Vec<ImageFrame>>,
}

impl ImageArchive {
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn total_fragment_count(&self) -> usize {
        self.frames.iter().map(Vec::len).sum()
    }
}

pub fn load_image_archive_path(path: impl AsRef<Path>) -> Result<ImageArchive> {
    let data = fs::read(path)?;
    load_image_archive_bytes(&data)
}

pub fn load_image_archive_bytes(data: &[u8]) -> Result<ImageArchive> {
    if data.starts_with(b"ANIM") {
        return load_anim_archive(data);
    }
    if data.starts_with(b"CMAP") {
        return load_image_set_archive(data);
    }
    Err(Error::invalid_format(
        "image archive does not begin with CMAP or ANIM",
    ))
}

fn load_image_set_archive(data: &[u8]) -> Result<ImageArchive> {
    let mut cursor = 0usize;
    let mut current_palette = None::<ImagePalette>;
    let mut frames = Vec::new();

    while cursor < data.len() {
        let chunk = read_ident(data, cursor)?;
        match chunk {
            b"CMAP" => {
                let (palette, consumed) = parse_cmap(data, cursor + 4)?;
                current_palette = Some(palette);
                cursor += 4 + consumed;
            }
            b"PICT" => {
                let palette = current_palette
                    .clone()
                    .ok_or_else(|| Error::invalid_format("PICT chunk appeared before CMAP"))?;
                let (frame, consumed) = parse_pict(data, cursor + 4, palette)?;
                frames.push(vec![frame]);
                cursor += 4 + consumed;
            }
            other => {
                return Err(Error::invalid_format(format!(
                    "unexpected image chunk {other:?} at 0x{cursor:x}"
                )));
            }
        }
    }

    Ok(ImageArchive {
        source_len: data.len(),
        kind: ImageArchiveKind::ImageSet,
        frames,
    })
}

fn load_anim_archive(data: &[u8]) -> Result<ImageArchive> {
    if data.len() < 10 {
        return Err(Error::invalid_format("ANIM archive is truncated"));
    }
    let declared_frame_count = read_u16(data, 4)?;
    let mut cursor = 6usize;
    let ident = read_ident(data, cursor)?;
    if ident != b"CMAP" {
        return Err(Error::invalid_format(
            "ANIM archive does not contain CMAP after header",
        ));
    }
    let (palette, consumed) = parse_cmap(data, cursor + 4)?;
    cursor += 4 + consumed;

    let mut frames = Vec::with_capacity(usize::from(declared_frame_count));
    for frame_index in 0..usize::from(declared_frame_count) {
        if cursor + 2 > data.len() {
            return Err(Error::invalid_format(format!(
                "ANIM frame {frame_index} is truncated"
            )));
        }
        let part_count = usize::from(read_u16(data, cursor)?);
        cursor += 2;
        let mut fragments = Vec::with_capacity(part_count);
        for _ in 0..part_count {
            let ident = read_ident(data, cursor)?;
            if ident != b"PICT" {
                return Err(Error::invalid_format(format!(
                    "ANIM frame {frame_index} expected PICT, found {ident:?}"
                )));
            }
            let (frame, pict_consumed) = parse_pict(data, cursor + 4, palette.clone())?;
            fragments.push(frame);
            cursor += 4 + pict_consumed;
        }
        frames.push(fragments);
    }

    Ok(ImageArchive {
        source_len: data.len(),
        kind: ImageArchiveKind::Animation {
            declared_frame_count,
        },
        frames,
    })
}

fn parse_cmap(data: &[u8], offset: usize) -> Result<(ImagePalette, usize)> {
    if offset >= data.len() {
        return Err(Error::UnexpectedEof("CMAP count"));
    }
    let color_count = usize::from(data[offset]);
    let colors_start = offset + 1;
    let colors_end = colors_start + color_count * 3;
    let aux_end = colors_end + color_count * 2;
    if aux_end > data.len() {
        return Err(Error::UnexpectedEof("CMAP payload"));
    }

    let mut colors = Vec::with_capacity(color_count);
    for index in 0..color_count {
        let base = colors_start + index * 3;
        colors.push(RgbColor::new(
            data[base].saturating_mul(4),
            data[base + 1].saturating_mul(4),
            data[base + 2].saturating_mul(4),
        ));
    }

    Ok((
        ImagePalette {
            colors,
            aux_data: data[colors_end..aux_end].to_vec(),
        },
        1 + color_count * 5,
    ))
}

fn parse_pict(data: &[u8], offset: usize, palette: ImagePalette) -> Result<(ImageFrame, usize)> {
    if offset + 9 > data.len() {
        return Err(Error::UnexpectedEof("PICT header"));
    }

    let screen_offset = read_u16(data, offset)?;
    let height = read_u16(data, offset + 2)?;
    let width = read_u16(data, offset + 4)?;
    let widths = (data[offset + 6], data[offset + 7], data[offset + 8]);
    let pixel_count = usize::from(width.max(1)) * usize::from(height.max(1));
    let (pixels, consumed) = decompress_stream(data, offset + 9, Some(pixel_count), widths)?;
    let actual_height = if height == 0 { 1 } else { height };

    Ok((
        ImageFrame {
            offset: screen_offset,
            x_offset: screen_offset % SCREEN_WIDTH,
            y_offset: screen_offset / SCREEN_WIDTH,
            width,
            height: actual_height,
            pixels,
            palette,
            transparent_zero: true,
        },
        9 + consumed,
    ))
}

fn read_ident(data: &[u8], offset: usize) -> Result<&[u8; 4]> {
    data.get(offset..offset + 4)
        .ok_or(Error::UnexpectedEof("image chunk ident"))?
        .try_into()
        .map_err(|_| Error::UnexpectedEof("image chunk ident"))
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

    use super::{load_image_archive_path, ImageArchiveKind};

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn shipped_image_archives_match_known_counts() {
        let intro = load_image_archive_path(repo_root().join("INTRO.LZS")).unwrap();
        assert_eq!(intro.kind, ImageArchiveKind::ImageSet);
        assert_eq!(intro.frame_count(), 10);
        assert_eq!(intro.total_fragment_count(), 10);

        let anim = load_image_archive_path(repo_root().join("ANIM.LZS")).unwrap();
        assert_eq!(
            anim.kind,
            ImageArchiveKind::Animation {
                declared_frame_count: 100
            }
        );
        assert_eq!(anim.frame_count(), 100);
        assert_eq!(anim.total_fragment_count(), 221);

        let main_menu = load_image_archive_path(repo_root().join("MAINMENU.LZS")).unwrap();
        assert_eq!(main_menu.frame_count(), 3);
    }
}
