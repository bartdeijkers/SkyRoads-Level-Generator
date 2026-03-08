use std::fs;
use std::path::Path;

use crate::{decompress_stream, Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MuzaxSongHeader {
    pub index: usize,
    pub start_pos: u16,
    pub num_instruments: u16,
    pub uncompressed_length: u16,
}

impl MuzaxSongHeader {
    pub fn is_empty(&self) -> bool {
        self.start_pos == 0 && self.num_instruments == 0 && self.uncompressed_length == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MuzaxOscillator {
    pub tremolo: bool,
    pub vibrato: bool,
    pub sound_sustaining: bool,
    pub key_scaling: bool,
    pub multiplication: u8,
    pub key_scale_level: u8,
    pub output_level: u8,
    pub attack_rate: u8,
    pub decay_rate: u8,
    pub sustain_level: u8,
    pub release_rate: u8,
    pub wave_form: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuzaxInstrument {
    pub index: usize,
    pub raw: [u8; 16],
    pub operator_a: MuzaxOscillator,
    pub operator_b: MuzaxOscillator,
    pub channel_config: u8,
    pub tail: [u8; 5],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MuzaxCommandHead {
    pub index: usize,
    pub low: u8,
    pub high: u8,
    pub function_type: u8,
    pub channel: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuzaxCommandSummary {
    pub byte_length: usize,
    pub odd_trailing_byte: usize,
    pub command_count: usize,
    pub function_counts: [usize; 8],
    pub head: Vec<MuzaxCommandHead>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuzaxSong {
    pub header: MuzaxSongHeader,
    pub next_song_start: usize,
    pub widths: Option<[u8; 3]>,
    pub compressed_end: Option<usize>,
    pub compressed_size: Option<usize>,
    pub payload: Option<Vec<u8>>,
    pub instrument_bytes: usize,
    pub command_bytes: usize,
    pub instruments: Vec<MuzaxInstrument>,
    pub commands: Option<Vec<u8>>,
    pub command_summary: Option<MuzaxCommandSummary>,
}

impl MuzaxSong {
    pub fn is_empty(&self) -> bool {
        self.header.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuzaxArchive {
    pub source_len: usize,
    pub song_table_size: u16,
    pub songs: Vec<MuzaxSong>,
}

impl MuzaxArchive {
    pub fn song_count(&self) -> usize {
        self.songs.len()
    }

    pub fn populated_song_count(&self) -> usize {
        self.songs.iter().filter(|song| !song.is_empty()).count()
    }
}

pub fn load_muzax_lzs_path(path: impl AsRef<Path>) -> Result<MuzaxArchive> {
    let data = fs::read(path)?;
    load_muzax_lzs_bytes(&data)
}

pub fn load_muzax_lzs_bytes(data: &[u8]) -> Result<MuzaxArchive> {
    let song_table_size = read_u16(data, 0)?;
    if song_table_size % 6 != 0 {
        return Err(Error::invalid_format(format!(
            "MUZAX.LZS song table size is not a multiple of 6: {song_table_size}"
        )));
    }
    if usize::from(song_table_size) > data.len() {
        return Err(Error::invalid_format(format!(
            "MUZAX.LZS song table size is out of range: {song_table_size}"
        )));
    }

    let headers = parse_song_headers(data, song_table_size)?;
    let next_song_starts = build_next_song_starts(&headers, data.len());
    let mut songs = Vec::with_capacity(headers.len());

    for (header, next_song_start) in headers.into_iter().zip(next_song_starts) {
        if header.is_empty() {
            songs.push(MuzaxSong {
                header,
                next_song_start,
                widths: None,
                compressed_end: None,
                compressed_size: None,
                payload: None,
                instrument_bytes: 0,
                command_bytes: 0,
                instruments: Vec::new(),
                commands: None,
                command_summary: None,
            });
            continue;
        }

        let start_pos = usize::from(header.start_pos);
        if start_pos + 3 > data.len() {
            return Err(Error::invalid_format(format!(
                "MUZAX song {} starts out of range at 0x{start_pos:x}",
                header.index
            )));
        }
        let widths = [data[start_pos], data[start_pos + 1], data[start_pos + 2]];
        let (payload, consumed) = decompress_stream(
            data,
            start_pos + 3,
            Some(usize::from(header.uncompressed_length)),
            (widths[0], widths[1], widths[2]),
        )?;
        let instrument_bytes = usize::from(header.num_instruments) * 16;
        if instrument_bytes > payload.len() {
            return Err(Error::invalid_format(format!(
                "MUZAX song {} instrument region exceeds payload: {instrument_bytes} > {}",
                header.index,
                payload.len()
            )));
        }
        let instruments_blob = &payload[..instrument_bytes];
        let commands_blob = payload[instrument_bytes..].to_vec();
        let instruments = parse_instruments(instruments_blob, usize::from(header.num_instruments))?;
        let command_summary = summarize_commands(&commands_blob, 32);

        songs.push(MuzaxSong {
            header,
            next_song_start,
            widths: Some(widths),
            compressed_end: Some(start_pos + 3 + consumed),
            compressed_size: Some(3 + consumed),
            payload: Some(payload),
            instrument_bytes,
            command_bytes: commands_blob.len(),
            instruments,
            commands: Some(commands_blob),
            command_summary: Some(command_summary),
        });
    }

    Ok(MuzaxArchive {
        source_len: data.len(),
        song_table_size,
        songs,
    })
}

fn parse_song_headers(data: &[u8], song_table_size: u16) -> Result<Vec<MuzaxSongHeader>> {
    let count = usize::from(song_table_size / 6);
    let mut headers = Vec::with_capacity(count);
    for index in 0..count {
        let start_pos = read_u16(data, index * 6)?;
        let num_instruments = read_u16(data, index * 6 + 2)?;
        let uncompressed_length = read_u16(data, index * 6 + 4)?;
        headers.push(MuzaxSongHeader {
            index,
            start_pos,
            num_instruments,
            uncompressed_length,
        });
    }
    Ok(headers)
}

fn build_next_song_starts(headers: &[MuzaxSongHeader], file_len: usize) -> Vec<usize> {
    let mut next_song_starts = Vec::with_capacity(headers.len());
    for (index, _) in headers.iter().enumerate() {
        let mut next_start = file_len;
        for later in headers.iter().skip(index + 1) {
            if later.start_pos != 0 {
                next_start = usize::from(later.start_pos);
                break;
            }
        }
        next_song_starts.push(next_start);
    }
    next_song_starts
}

fn parse_instruments(data: &[u8], count: usize) -> Result<Vec<MuzaxInstrument>> {
    let mut instruments = Vec::with_capacity(count);
    for index in 0..count {
        let start = index * 16;
        let end = start + 16;
        if end > data.len() {
            return Err(Error::invalid_format(format!(
                "MUZAX instrument {index} is truncated"
            )));
        }
        let raw = data[start..end].try_into().unwrap();
        instruments.push(MuzaxInstrument {
            index,
            raw,
            operator_a: parse_oscillator(&data[start..start + 5]),
            operator_b: parse_oscillator(&data[start + 5..start + 10]),
            channel_config: data[start + 10],
            tail: data[start + 11..start + 16].try_into().unwrap(),
        });
    }
    Ok(instruments)
}

fn parse_oscillator(block: &[u8]) -> MuzaxOscillator {
    let tremolo = block[0];
    let key_scale_level = block[1];
    let attack_rate = block[2];
    let sustain_level = block[3];
    let wave_form = block[4];
    MuzaxOscillator {
        tremolo: (tremolo & 0x80) != 0,
        vibrato: (tremolo & 0x40) != 0,
        sound_sustaining: (tremolo & 0x20) != 0,
        key_scaling: (tremolo & 0x10) != 0,
        multiplication: tremolo & 0x0F,
        key_scale_level: key_scale_level >> 6,
        output_level: key_scale_level & 0x3F,
        attack_rate: attack_rate >> 4,
        decay_rate: attack_rate & 0x0F,
        sustain_level: sustain_level >> 4,
        release_rate: sustain_level & 0x0F,
        wave_form: wave_form & 0x07,
    }
}

fn summarize_commands(data: &[u8], head_count: usize) -> MuzaxCommandSummary {
    let command_count = data.len() / 2;
    let mut function_counts = [0usize; 8];
    let mut head = Vec::new();
    for index in 0..command_count {
        let low = data[index * 2];
        let high = data[index * 2 + 1];
        let function_type = low & 0x07;
        let channel = low >> 4;
        function_counts[usize::from(function_type)] += 1;
        if head.len() < head_count {
            head.push(MuzaxCommandHead {
                index,
                low,
                high,
                function_type,
                channel,
            });
        }
    }
    MuzaxCommandSummary {
        byte_length: data.len(),
        odd_trailing_byte: data.len() % 2,
        command_count,
        function_counts,
        head,
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

    use super::load_muzax_lzs_path;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn shipped_muzax_matches_verified_headers() {
        let muzax = load_muzax_lzs_path(repo_root().join("MUZAX.LZS")).unwrap();
        assert_eq!(muzax.song_table_size, 120);
        assert_eq!(muzax.song_count(), 20);
        assert_eq!(muzax.populated_song_count(), 14);

        let song0 = &muzax.songs[0];
        assert_eq!(song0.widths, Some([6, 10, 12]));
        assert_eq!(song0.compressed_size, Some(1302));
        assert_eq!(song0.instrument_bytes, 144);
        assert_eq!(song0.command_bytes, 12174);
        assert_eq!(
            song0.command_summary.as_ref().unwrap().function_counts,
            [1444, 9, 3718, 909, 4, 1, 1, 1]
        );
    }
}
