use std::fs;
use std::path::Path;

use crate::{Error, Result};

pub const SAMPLE_RATE_PCM_8K: u32 = 8000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pcm8Sample {
    pub sample_rate: u32,
    pub samples: Vec<u8>,
}

impl Pcm8Sample {
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    pub fn duration_seconds(&self) -> f64 {
        self.samples.len() as f64 / self.sample_rate as f64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfxEntry {
    pub index: usize,
    pub start: usize,
    pub end: usize,
    pub sample: Pcm8Sample,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfxBank {
    pub source_len: usize,
    pub sample_rate: u32,
    pub effects: Vec<SfxEntry>,
}

impl SfxBank {
    pub fn effect_count(&self) -> usize {
        self.effects.len()
    }
}

pub fn load_intro_snd_path(path: impl AsRef<Path>) -> Result<Pcm8Sample> {
    let data = fs::read(path)?;
    Ok(load_intro_snd_bytes(&data))
}

pub fn load_intro_snd_bytes(data: &[u8]) -> Pcm8Sample {
    Pcm8Sample {
        sample_rate: SAMPLE_RATE_PCM_8K,
        samples: data.to_vec(),
    }
}

pub fn load_sfx_snd_path(path: impl AsRef<Path>) -> Result<SfxBank> {
    let data = fs::read(path)?;
    load_sfx_snd_bytes(&data)
}

pub fn load_sfx_snd_bytes(data: &[u8]) -> Result<SfxBank> {
    if data.len() < 2 {
        return Err(Error::invalid_format(
            "SFX.SND is too small to contain an offset table",
        ));
    }

    let first_offset = usize::from(read_u16(data, 0)?);
    if first_offset % 2 != 0 {
        return Err(Error::invalid_format(format!(
            "SFX.SND first offset is not aligned: {first_offset}"
        )));
    }
    if first_offset > data.len() {
        return Err(Error::invalid_format(format!(
            "SFX.SND first offset is out of range: {first_offset}"
        )));
    }

    let mut offsets = Vec::with_capacity(first_offset / 2);
    for offset in (0..first_offset).step_by(2) {
        offsets.push(usize::from(read_u16(data, offset)?));
    }
    if offsets.first().copied().unwrap_or(0) != first_offset {
        return Err(Error::invalid_format(format!(
            "SFX.SND offset table does not point to its first payload: {} != {first_offset}",
            offsets.first().copied().unwrap_or(0)
        )));
    }
    if offsets.windows(2).any(|pair| pair[0] > pair[1]) {
        return Err(Error::invalid_format(
            "SFX.SND offsets are not monotonically increasing",
        ));
    }

    let mut effects = Vec::with_capacity(offsets.len());
    for (index, start) in offsets.iter().copied().enumerate() {
        let end = offsets.get(index + 1).copied().unwrap_or(data.len());
        if end > data.len() || start > end {
            return Err(Error::invalid_format(format!(
                "SFX.SND effect {index} has invalid range {start}..{end}"
            )));
        }
        effects.push(SfxEntry {
            index,
            start,
            end,
            sample: Pcm8Sample {
                sample_rate: SAMPLE_RATE_PCM_8K,
                samples: data[start..end].to_vec(),
            },
        });
    }

    Ok(SfxBank {
        source_len: data.len(),
        sample_rate: SAMPLE_RATE_PCM_8K,
        effects,
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

    use super::{load_intro_snd_path, load_sfx_snd_path};

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn shipped_sound_banks_match_known_lengths() {
        let intro = load_intro_snd_path(repo_root().join("INTRO.SND")).unwrap();
        assert_eq!(intro.sample_rate, 8000);
        assert_eq!(intro.sample_count(), 32100);

        let sfx = load_sfx_snd_path(repo_root().join("SFX.SND")).unwrap();
        assert_eq!(sfx.effect_count(), 6);
        let lengths = sfx
            .effects
            .iter()
            .map(|entry| entry.sample.sample_count())
            .collect::<Vec<_>>();
        assert_eq!(lengths, vec![3984, 5154, 8085, 801, 7771, 0]);
    }
}
