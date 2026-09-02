use std::path::Path;

use oplon::Opl2;
use skyroads_core::AudioCommand;
use skyroads_data::{
    load_intro_snd_path, load_muzax_lzs_path, load_sfx_snd_path, MuzaxArchive, Pcm8Sample, Result,
    SfxBank,
};

const OUTPUT_SAMPLE_RATE: u32 = 48_000;
const PIT_INPUT_HZ: u64 = 1_193_182;
const MUSIC_TIMER_DIVISOR: u64 = 0x19E4;
const INTRO_GAIN: f32 = 0.40;
const MUSIC_GAIN: f32 = 0.32;
const OPL_OUTPUT_SCALE: f32 = 32_768.0;
const OPL_TRACK_COUNT: usize = 11;
const MELODIC_TRACK_COUNT: usize = 6;
const BASS_DRUM_TRACK: usize = 6;

const OPERATOR_REGISTER_GROUPS: [u8; 5] = [0x20, 0x40, 0x60, 0x80, 0xE0];
const PRIMARY_OPERATOR_OFFSETS: [u8; OPL_TRACK_COUNT] = [
    0x00, 0x01, 0x02, 0x08, 0x09, 0x0A, 0x10, 0x14, 0x12, 0x15, 0x11,
];
const SECONDARY_OPERATOR_OFFSETS: [u8; OPL_TRACK_COUNT] = [
    0x03, 0x04, 0x05, 0x0B, 0x0C, 0x0D, 0x13, 0xFF, 0xFF, 0xFF, 0xFF,
];
const CHANNEL_OFFSETS: [u8; OPL_TRACK_COUNT] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0xFF, 0x08, 0xFF,
];
const NOTE_FNUM_LOW: [u8; 12] = [
    0xAC, 0xB6, 0xC1, 0xCD, 0xD9, 0xE6, 0xF3, 0x02, 0x11, 0x22, 0x33, 0x45,
];
const NOTE_FNUM_HIGH: [u8; 12] = [0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1];
const VOLUME_ATTENUATION: [u8; 31] = [
    0x3F, 0x14, 0x10, 0x0E, 0x0C, 0x0A, 0x09, 0x08, 0x07, 0x06, 0x06, 0x05, 0x05, 0x04, 0x04, 0x04,
    0x04, 0x04, 0x03, 0x03, 0x03, 0x03, 0x02, 0x02, 0x02, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00,
];

// The DOS executable installs these four single-operator rhythm patches once
// during AdLib initialization, before individual songs supply their own patches.
const DEFAULT_RHYTHM_PATCHES: [[u8; 11]; 4] = [
    [
        0x0C, 0x00, 0xF8, 0xB5, 0x00, 0x00, 0x00, 0xD6, 0x4F, 0x00, 0x01,
    ],
    [
        0x04, 0x00, 0xF7, 0xB5, 0x00, 0x00, 0x00, 0xD6, 0x4F, 0x00, 0x01,
    ],
    [
        0x01, 0x00, 0xF5, 0xB5, 0x00, 0x00, 0x00, 0xD6, 0x4F, 0x00, 0x01,
    ],
    [
        0x01, 0x00, 0xF7, 0xB5, 0x00, 0x4E, 0x00, 0x10, 0x00, 0x00, 0x01,
    ],
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioTimelineEvent {
    PlaySong(u8),
    StopSong,
    PlayIntroSample,
    PlaySfx(u8),
    StopAllSamples,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttractAudioAssets {
    pub intro: Pcm8Sample,
    pub sfx: SfxBank,
    pub muzax: MuzaxArchive,
}

impl AttractAudioAssets {
    pub fn load_from_root(source_root: impl AsRef<Path>) -> Result<Self> {
        let source_root = source_root.as_ref();
        Ok(Self {
            intro: load_intro_snd_path(source_root.join("INTRO.SND"))?,
            sfx: load_sfx_snd_path(source_root.join("SFX.SND"))?,
            muzax: load_muzax_lzs_path(source_root.join("MUZAX.LZS"))?,
        })
    }
}

pub struct AudioMixer {
    assets: AttractAudioAssets,
    timeline: Vec<AudioTimelineEvent>,
    active_samples: Vec<ActivePcm>,
    opl: OplRenderer,
    player: MuzaxPlayer,
}

impl AudioMixer {
    pub fn new(assets: AttractAudioAssets) -> Self {
        let opl = OplRenderer::new(OUTPUT_SAMPLE_RATE);
        let player = MuzaxPlayer::new(assets.muzax.clone());
        Self {
            assets,
            timeline: Vec::new(),
            active_samples: Vec::new(),
            opl,
            player,
        }
    }

    pub fn output_sample_rate(&self) -> u32 {
        OUTPUT_SAMPLE_RATE
    }

    pub fn timeline(&self) -> &[AudioTimelineEvent] {
        &self.timeline
    }

    pub fn apply_commands(&mut self, commands: &[AudioCommand]) {
        for command in commands {
            match *command {
                AudioCommand::PlaySong(song) => {
                    self.player.load_song(song as usize, &mut self.opl);
                    self.timeline.push(AudioTimelineEvent::PlaySong(song));
                }
                AudioCommand::StopSong => {
                    self.player.stop(&mut self.opl);
                    self.timeline.push(AudioTimelineEvent::StopSong);
                }
                AudioCommand::PlayIntroSample => {
                    self.active_samples
                        .push(ActivePcm::new(self.assets.intro.clone(), INTRO_GAIN));
                    self.timeline.push(AudioTimelineEvent::PlayIntroSample);
                }
                AudioCommand::PlaySfx(index) => {
                    if let Some(effect) = self.assets.sfx.effects.get(index as usize) {
                        self.active_samples
                            .push(ActivePcm::new(effect.sample.clone(), 0.55));
                        self.timeline.push(AudioTimelineEvent::PlaySfx(index));
                    }
                }
                AudioCommand::StopAllSamples => {
                    self.active_samples.clear();
                    self.timeline.push(AudioTimelineEvent::StopAllSamples);
                }
            }
        }
    }

    pub fn render_i16(&mut self, sample_count: usize) -> Vec<i16> {
        let mut out = vec![0i16; sample_count];
        self.render_into(&mut out);
        out
    }

    pub fn render_into(&mut self, out: &mut [i16]) {
        let mut music_accum = vec![0.0f32; out.len()];
        self.player.render(&mut self.opl, &mut music_accum);

        for (index, sample_out) in out.iter_mut().enumerate() {
            let mut mixed = music_accum[index] * MUSIC_GAIN;
            for playback in &mut self.active_samples {
                mixed += playback.next_sample(OUTPUT_SAMPLE_RATE);
            }
            *sample_out = (mixed.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        }

        self.active_samples.retain(|playback| !playback.finished);
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ActivePcm {
    sample: Pcm8Sample,
    position: f64,
    gain: f32,
    finished: bool,
}

impl ActivePcm {
    fn new(sample: Pcm8Sample, gain: f32) -> Self {
        Self {
            sample,
            position: 0.0,
            gain,
            finished: false,
        }
    }

    fn next_sample(&mut self, output_rate: u32) -> f32 {
        if self.finished {
            return 0.0;
        }
        let index = self.position.floor() as usize;
        let Some(byte) = self.sample.samples.get(index).copied() else {
            self.finished = true;
            return 0.0;
        };
        self.position += self.sample.sample_rate as f64 / output_rate as f64;
        ((byte as f32 / 255.0) * 2.0 - 1.0) * self.gain
    }
}

struct OplRenderer {
    chip: Opl2,
    rhythm_register: u8,
    #[cfg(test)]
    register_trace: Vec<(u8, u8)>,
}

impl OplRenderer {
    fn new(output_rate: u32) -> Self {
        let mut renderer = Self {
            chip: Opl2::new(output_rate),
            rhythm_register: 0xE0,
            #[cfg(test)]
            register_trace: Vec::new(),
        };
        renderer.initialize_adlib();
        renderer
    }

    fn initialize_adlib(&mut self) {
        self.reset_song();
        self.write_register(0x01, 0x20);
        self.write_register(0x08, 0x00);
        self.write_register(0xBD, 0xE0);

        for (patch_index, patch) in DEFAULT_RHYTHM_PATCHES.iter().enumerate() {
            self.write_instrument(7 + patch_index, patch);
        }

        self.write_register(0xA8, 0xAC);
        self.write_register(0xB8, 0x0C);
        self.write_register(0xA7, 0x02);
        self.write_register(0xB7, 0x0D);
    }

    fn reset_song(&mut self) {
        self.rhythm_register = 0xE0;
        for register in 0x40..=0x55 {
            self.write_register(register, 0x3F);
        }
        for track in (0..=7).rev() {
            self.stop_note(track);
        }
    }

    fn write_register(&mut self, register: u8, value: u8) {
        if register == 0xBD {
            self.rhythm_register = value;
        }
        self.chip.write_reg(register, value);
        #[cfg(test)]
        self.register_trace.push((register, value));
    }

    fn write_instrument(&mut self, track: usize, patch: &[u8]) {
        let Some(primary_offset) = PRIMARY_OPERATOR_OFFSETS.get(track).copied() else {
            return;
        };
        if patch.len() < 11 {
            return;
        }

        self.write_operator(primary_offset, &patch[..5]);

        let secondary_offset = SECONDARY_OPERATOR_OFFSETS[track];
        if secondary_offset != 0xFF {
            self.write_operator(secondary_offset, &patch[5..10]);
        }

        let channel_offset = CHANNEL_OFFSETS[track];
        if channel_offset != 0xFF {
            self.write_register(0xC0 + channel_offset, patch[10]);
        }
    }

    fn write_operator(&mut self, operator_offset: u8, parameters: &[u8]) {
        for (register_group, value) in OPERATOR_REGISTER_GROUPS.iter().zip(parameters) {
            self.write_register(register_group + operator_offset, *value);
        }
    }

    fn stop_note(&mut self, track: usize) {
        if track < MELODIC_TRACK_COUNT {
            self.write_register(0xB0 + CHANNEL_OFFSETS[track], 0x00);
            return;
        }
        if track >= OPL_TRACK_COUNT {
            return;
        }

        let rhythm_bit = 0x10 >> (track - BASS_DRUM_TRACK);
        self.write_register(0xBD, self.rhythm_register & !rhythm_bit);
    }

    fn play_note(&mut self, track: usize, note: u8) {
        if track >= OPL_TRACK_COUNT {
            return;
        }

        self.stop_note(track);
        if track <= BASS_DRUM_TRACK {
            let note_index = usize::from(note % 12);
            let octave = note / 12 + 2;
            let channel = CHANNEL_OFFSETS[track];
            self.write_register(0xA0 + channel, NOTE_FNUM_LOW[note_index]);

            let mut frequency_high = NOTE_FNUM_HIGH[note_index] | (octave << 2);
            if track < MELODIC_TRACK_COUNT {
                frequency_high |= 0x20;
            }
            self.write_register(0xB0 + channel, frequency_high);
        }

        if track >= BASS_DRUM_TRACK {
            let rhythm_bit = 0x10 >> (track - BASS_DRUM_TRACK);
            self.write_register(0xBD, self.rhythm_register | rhythm_bit);
        }
    }

    fn render_sample(&mut self) -> f32 {
        let (left, right) = self.chip.render_frame();
        let mono = (i64::from(left) + i64::from(right)) as f32 * 0.5;
        (mono / OPL_OUTPUT_SCALE).clamp(-1.0, 1.0)
    }

    #[cfg(test)]
    fn clear_trace(&mut self) {
        self.register_trace.clear();
    }
}

#[derive(Debug, Clone, PartialEq)]
struct MuzaxPlayer {
    muzax: MuzaxArchive,
    current_song: Option<usize>,
    commands: Vec<u8>,
    cursor: usize,
    paused: u8,
    loop_position: usize,
    timer_phase: u64,
    current_instruments: [u8; OPL_TRACK_COUNT],
    status: u8,
}

impl MuzaxPlayer {
    fn new(muzax: MuzaxArchive) -> Self {
        Self {
            muzax,
            current_song: None,
            commands: Vec::new(),
            cursor: 0,
            paused: 0,
            loop_position: 0,
            timer_phase: 0,
            current_instruments: [0; OPL_TRACK_COUNT],
            status: 0,
        }
    }

    fn load_song(&mut self, song_index: usize, opl: &mut OplRenderer) {
        if self.current_song == Some(song_index) {
            return;
        }

        self.current_song = Some(song_index);
        self.cursor = 0;
        self.paused = 0;
        self.loop_position = 0;
        self.timer_phase = 0;
        self.status = 0;
        self.commands = self
            .muzax
            .songs
            .get(song_index)
            .and_then(|song| song.commands.clone())
            .unwrap_or_default();
        opl.reset_song();
    }

    fn stop(&mut self, opl: &mut OplRenderer) {
        self.current_song = None;
        self.commands.clear();
        self.cursor = 0;
        self.paused = 0;
        self.loop_position = 0;
        self.timer_phase = 0;
        self.status = 0;
        opl.reset_song();
    }

    fn render(&mut self, opl: &mut OplRenderer, out: &mut [f32]) {
        if self.current_song.is_none() {
            out.fill(0.0);
            return;
        }

        let tick_threshold = u64::from(OUTPUT_SAMPLE_RATE) * MUSIC_TIMER_DIVISOR;
        for sample in out {
            self.timer_phase += PIT_INPUT_HZ;
            while self.timer_phase >= tick_threshold {
                self.process_tick(opl);
                self.timer_phase -= tick_threshold;
            }
            *sample = opl.render_sample();
        }
    }

    fn process_tick(&mut self, opl: &mut OplRenderer) {
        if self.current_song.is_none() || self.commands.is_empty() {
            return;
        }
        if self.paused > 0 {
            self.paused -= 1;
            return;
        }

        let command_count = self.commands.len() / 2;
        for _ in 0..=command_count {
            if self.paused > 0 {
                self.paused -= 1;
                return;
            }
            if self.cursor + 1 >= self.commands.len() {
                self.cursor = 0;
            }

            let command = self.commands[self.cursor];
            let value = self.commands[self.cursor + 1];
            self.cursor += 2;

            let function = command & 0x07;
            let track = usize::from(command >> 4);
            match function {
                0 => self.paused = value,
                1 => self.configure_instrument(track, usize::from(value), opl),
                2 => opl.play_note(track, value & 0x7F),
                3 => opl.stop_note(track),
                4 => self.set_volume(track, value, opl),
                5 => self.cursor = self.loop_position.min(self.commands.len()),
                6 => self.loop_position = self.cursor,
                7 => self.status = value,
                _ => unreachable!(),
            }
        }
    }

    fn configure_instrument(
        &mut self,
        track: usize,
        instrument_index: usize,
        opl: &mut OplRenderer,
    ) {
        if track >= OPL_TRACK_COUNT {
            return;
        }
        let Some(song_index) = self.current_song else {
            return;
        };
        let Some(instrument) = self
            .muzax
            .songs
            .get(song_index)
            .and_then(|song| song.instruments.get(instrument_index))
        else {
            return;
        };

        opl.stop_note(track);
        self.current_instruments[track] = instrument_index as u8;
        opl.write_instrument(track, &instrument.raw);
    }

    fn set_volume(&self, track: usize, volume: u8, opl: &mut OplRenderer) {
        if track >= OPL_TRACK_COUNT {
            return;
        }
        let Some(song_index) = self.current_song else {
            return;
        };
        let instrument_index = usize::from(self.current_instruments[track]);
        let Some(instrument) = self
            .muzax
            .songs
            .get(song_index)
            .and_then(|song| song.instruments.get(instrument_index))
        else {
            return;
        };
        let volume_index = usize::from(volume.min(30));
        let attenuation = VOLUME_ATTENUATION[volume_index];

        let secondary_offset = SECONDARY_OPERATOR_OFFSETS[track];
        if secondary_offset == 0xFF {
            write_operator_volume(
                opl,
                PRIMARY_OPERATOR_OFFSETS[track],
                instrument.raw[1],
                attenuation,
            );
            return;
        }

        write_operator_volume(opl, secondary_offset, instrument.raw[6], attenuation);
        let additive = instrument.channel_config & 0x01 != 0;
        if additive {
            write_operator_volume(
                opl,
                PRIMARY_OPERATOR_OFFSETS[track],
                instrument.raw[1],
                attenuation,
            );
        }
    }
}

fn write_operator_volume(
    opl: &mut OplRenderer,
    operator_offset: u8,
    instrument_level: u8,
    attenuation: u8,
) {
    let key_scale_level = instrument_level & 0xC0;
    let total_level = (instrument_level & 0x3F)
        .saturating_add(attenuation)
        .min(0x3F);
    opl.write_register(0x40 + operator_offset, key_scale_level | total_level);
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use skyroads_core::AudioCommand;

    use super::{
        AttractAudioAssets, AudioMixer, AudioTimelineEvent, MuzaxPlayer, OplRenderer,
        OUTPUT_SAMPLE_RATE,
    };

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn audio_assets_load() {
        let assets = AttractAudioAssets::load_from_root(repo_root()).unwrap();
        assert_eq!(assets.intro.sample_count(), 32100);
        assert_eq!(assets.sfx.effect_count(), 6);
        assert_eq!(assets.muzax.populated_song_count(), 14);
    }

    #[test]
    fn mixer_records_commands_and_renders_audio() {
        let assets = AttractAudioAssets::load_from_root(repo_root()).unwrap();
        let mut mixer = AudioMixer::new(assets);
        mixer.apply_commands(&[AudioCommand::PlaySong(1), AudioCommand::PlayIntroSample]);
        assert_eq!(
            mixer.timeline(),
            &[
                AudioTimelineEvent::PlaySong(1),
                AudioTimelineEvent::PlayIntroSample
            ]
        );
        let samples = mixer.render_i16(2048);
        assert!(samples.iter().any(|sample| *sample != 0));
    }

    #[test]
    fn every_populated_muzax_song_renders_music_without_clipping() {
        let assets = AttractAudioAssets::load_from_root(repo_root()).unwrap();

        for song_index in 0..assets.muzax.populated_song_count() {
            let mut mixer = AudioMixer::new(assets.clone());
            mixer.apply_commands(&[AudioCommand::PlaySong(song_index as u8)]);
            let samples = mixer.render_i16(OUTPUT_SAMPLE_RATE as usize * 3);
            let peak = samples
                .iter()
                .map(|sample| sample.unsigned_abs())
                .max()
                .unwrap_or_default();

            assert!(peak > 32, "MUZAX song {song_index} rendered as silence");
            assert!(peak < i16::MAX as u16, "MUZAX song {song_index} clipped");
        }
    }

    #[test]
    fn dos_timer_processes_the_first_music_tick_after_267_output_samples() {
        let assets = AttractAudioAssets::load_from_root(repo_root()).unwrap();
        let mut player = MuzaxPlayer::new(assets.muzax);
        let mut opl = OplRenderer::new(OUTPUT_SAMPLE_RATE);
        player.load_song(1, &mut opl);
        opl.clear_trace();

        player.render(&mut opl, &mut vec![0.0; 266]);
        assert!(opl.register_trace.is_empty());

        player.render(&mut opl, &mut [0.0]);
        assert!(!opl.register_trace.is_empty());
    }

    #[test]
    fn menu_song_first_tick_matches_the_dos_register_sequence() {
        let assets = AttractAudioAssets::load_from_root(repo_root()).unwrap();
        let mut player = MuzaxPlayer::new(assets.muzax);
        let mut opl = OplRenderer::new(OUTPUT_SAMPLE_RATE);
        player.load_song(1, &mut opl);
        opl.clear_trace();

        player.process_tick(&mut opl);

        assert_eq!(
            &opl.register_trace[..20],
            &[
                (0xBD, 0xE0),
                (0x30, 0x00),
                (0x50, 0x0B),
                (0x70, 0xA8),
                (0x90, 0x4C),
                (0xF0, 0x00),
                (0x33, 0x00),
                (0x53, 0x00),
                (0x73, 0xD6),
                (0x93, 0x4F),
                (0xF3, 0x00),
                (0xC6, 0x00),
                (0x53, 0x08),
                (0xBD, 0xE0),
                (0xA6, 0xD9),
                (0xB6, 0x14),
                (0xBD, 0xF0),
                (0xB0, 0x00),
                (0x20, 0x20),
                (0x40, 0x4B),
            ]
        );
        assert_eq!(player.paused, 83);
    }

    #[test]
    fn rhythm_notes_use_opl2_rhythm_bits_instead_of_fake_channels() {
        let mut opl = OplRenderer::new(OUTPUT_SAMPLE_RATE);
        opl.clear_trace();

        opl.play_note(7, 99);
        opl.play_note(10, 1);
        opl.stop_note(7);

        assert_eq!(
            opl.register_trace,
            vec![
                (0xBD, 0xE0),
                (0xBD, 0xE8),
                (0xBD, 0xE8),
                (0xBD, 0xE9),
                (0xBD, 0xE1)
            ]
        );
    }

    #[test]
    fn volume_uses_the_dos_attenuation_table_and_preserves_patch_level() {
        let assets = AttractAudioAssets::load_from_root(repo_root()).unwrap();
        let mut player = MuzaxPlayer::new(assets.muzax);
        let mut opl = OplRenderer::new(OUTPUT_SAMPLE_RATE);
        player.load_song(1, &mut opl);
        player.configure_instrument(6, 1, &mut opl);
        opl.clear_trace();

        player.set_volume(6, 7, &mut opl);

        assert_eq!(opl.register_trace, vec![(0x53, 0x08)]);
    }
}
