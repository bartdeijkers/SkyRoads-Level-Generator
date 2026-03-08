use std::path::Path;

use skyroads_core::AudioCommand;
use skyroads_data::{
    load_intro_snd_path, load_muzax_lzs_path, load_sfx_snd_path, MuzaxArchive, MuzaxInstrument,
    Pcm8Sample, Result, SfxBank,
};

const OUTPUT_SAMPLE_RATE: u32 = 48_000;
const MUSIC_TICK_SECONDS: f32 = 0.005;
const INTRO_GAIN: f32 = 0.40;
const MUSIC_GAIN: f32 = 0.32;
const MIN_DB: f32 = -96.0;
const MAX_DB: f32 = 0.0;
const SAMPLE_COUNT_WAVE: usize = 1024;

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

#[derive(Debug, Clone, PartialEq)]
pub struct AudioMixer {
    assets: AttractAudioAssets,
    timeline: Vec<AudioTimelineEvent>,
    active_samples: Vec<ActivePcm>,
    synth: OplSynth,
    player: MuzaxPlayer,
}

impl AudioMixer {
    pub fn new(assets: AttractAudioAssets) -> Self {
        let synth = OplSynth::new(OUTPUT_SAMPLE_RATE as f32);
        let player = MuzaxPlayer::new(assets.muzax.clone());
        Self {
            assets,
            timeline: Vec::new(),
            active_samples: Vec::new(),
            synth,
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
                    self.player.load_song(song as usize, &mut self.synth);
                    self.timeline.push(AudioTimelineEvent::PlaySong(song));
                }
                AudioCommand::StopSong => {
                    self.player.stop(&mut self.synth);
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
        self.player.render(&mut self.synth, &mut music_accum);

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

#[derive(Debug, Clone, PartialEq)]
struct MuzaxPlayer {
    muzax: MuzaxArchive,
    current_song: Option<usize>,
    commands: Vec<u8>,
    cursor: usize,
    paused: u8,
    jump_pos: usize,
    time_until_tick: f32,
}

impl MuzaxPlayer {
    fn new(muzax: MuzaxArchive) -> Self {
        Self {
            muzax,
            current_song: None,
            commands: Vec::new(),
            cursor: 0,
            paused: 0,
            jump_pos: 0,
            time_until_tick: 0.0,
        }
    }

    fn load_song(&mut self, song_index: usize, synth: &mut OplSynth) {
        if self.current_song == Some(song_index) {
            return;
        }
        self.current_song = Some(song_index);
        self.cursor = 0;
        self.paused = 0;
        self.jump_pos = 0;
        self.time_until_tick = 0.0;
        self.commands = self
            .muzax
            .songs
            .get(song_index)
            .and_then(|song| song.commands.clone())
            .unwrap_or_default();
        synth.stop_all();
    }

    fn stop(&mut self, synth: &mut OplSynth) {
        self.current_song = None;
        self.commands.clear();
        self.cursor = 0;
        self.paused = 0;
        self.jump_pos = 0;
        self.time_until_tick = 0.0;
        synth.stop_all();
    }

    fn render(&mut self, synth: &mut OplSynth, out: &mut [f32]) {
        if self.current_song.is_none() {
            for sample in out.iter_mut() {
                *sample = 0.0;
            }
            return;
        }

        let dt = 1.0 / OUTPUT_SAMPLE_RATE as f32;
        for sample in out.iter_mut() {
            self.time_until_tick += dt;
            while self.time_until_tick >= MUSIC_TICK_SECONDS {
                self.read_note(synth);
                self.time_until_tick -= MUSIC_TICK_SECONDS;
            }
            *sample = synth.next_sample();
        }
    }

    fn read_note(&mut self, synth: &mut OplSynth) {
        if self.current_song.is_none() || self.commands.is_empty() {
            return;
        }
        if self.paused > 0 {
            self.paused -= 1;
            return;
        }

        while self.paused == 0 {
            if self.cursor + 1 >= self.commands.len() {
                self.cursor = 0;
            }

            let mut cmd_low = self.commands[self.cursor];
            let cmd_high = self.commands[self.cursor + 1];
            self.cursor += 2;

            let function_type = cmd_low & 7;
            cmd_low >>= 4;

            match function_type {
                0 => {
                    self.paused = cmd_high;
                    return;
                }
                1 => {
                    self.stop_note(cmd_low as usize, synth);
                    self.configure_instrument(cmd_low as usize, cmd_high as usize, synth);
                }
                2 => self.play_note(cmd_low as usize, cmd_high, synth),
                3 => self.stop_note(cmd_low as usize, synth),
                4 => synth.set_channel_volume(
                    cmd_low as usize,
                    (cmd_high & 0x3F) as f32 / 0x3F as f32 * -47.25,
                ),
                5 => self.cursor = self.jump_pos.min(self.commands.len()),
                6 => self.jump_pos = self.cursor,
                7 => {}
                _ => {}
            }
        }
    }

    fn configure_instrument(&self, channel: usize, instrument_index: usize, synth: &mut OplSynth) {
        let Some(song_index) = self.current_song else {
            return;
        };
        let Some(song) = self.muzax.songs.get(song_index) else {
            return;
        };
        let Some(instrument) = song.instruments.get(instrument_index) else {
            return;
        };
        synth.set_channel_config(channel, instrument);
    }

    fn stop_note(&self, channel: usize, synth: &mut OplSynth) {
        if channel < 11 {
            synth.stop_note(channel);
        }
    }

    fn play_note(&self, channel: usize, note: u8, synth: &mut OplSynth) {
        let low_freqs = [
            0xAC, 0xB6, 0xC1, 0xCD, 0xD9, 0xE6, 0xF3, 0x02, 0x11, 0x22, 0x33, 0x45,
        ];
        let high_freqs = [0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1];
        let note_idx = usize::from(note % 12);
        let octave = usize::from(note / 12) + 2;
        let freq_num = ((high_freqs[note_idx] as u16) << 8) | low_freqs[note_idx] as u16;
        let target = if channel < 6 {
            channel
        } else {
            channel - 6 + 6
        };
        synth.start_note(target, freq_num, octave as u8);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WaveType {
    Sine = 0,
    HalfSine = 1,
    AbsSign = 2,
    PulseSign = 3,
    SineEven = 4,
    AbsSineEven = 5,
    Square = 6,
    DerivedSquare = 7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyState {
    Off,
    Attack,
    Sustain,
    Decay,
    Release,
}

#[derive(Debug, Clone, PartialEq)]
struct OscDesc {
    tremolo: bool,
    vibrato: bool,
    sound_sustaining: bool,
    key_scaling: bool,
    multiplication: f32,
    key_scale_level: usize,
    output_level: f32,
    attack_rate: usize,
    decay_rate: usize,
    sustain_level: f32,
    release_rate: usize,
    wave_form: WaveType,
}

#[derive(Debug, Clone, PartialEq)]
struct OscState {
    config: OscDesc,
    state: KeyState,
    volume: f32,
    envelope_step: usize,
    angle: f32,
}

impl Default for OscState {
    fn default() -> Self {
        Self {
            config: OscDesc::default(),
            state: KeyState::Off,
            volume: MIN_DB,
            envelope_step: 0,
            angle: 0.0,
        }
    }
}

impl Default for OscDesc {
    fn default() -> Self {
        Self {
            tremolo: false,
            vibrato: false,
            sound_sustaining: true,
            key_scaling: false,
            multiplication: 1.0,
            key_scale_level: 0,
            output_level: 0.0,
            attack_rate: 0,
            decay_rate: 0,
            sustain_level: 0.0,
            release_rate: 0,
            wave_form: WaveType::Sine,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Channel {
    a: OscState,
    b: OscState,
    additive: bool,
    feedback: usize,
    freq_num: u16,
    block_num: u8,
    output_0: f32,
    output_1: f32,
    feedback_factor: f32,
    m1: f32,
    m2: f32,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            a: OscState::default(),
            b: OscState::default(),
            additive: false,
            feedback: 0,
            freq_num: 0,
            block_num: 0,
            output_0: 0.0,
            output_1: 0.0,
            feedback_factor: 0.0,
            m1: 0.0,
            m2: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct OplSynth {
    sample_rate: f32,
    time: f32,
    waves: [[f32; SAMPLE_COUNT_WAVE]; 8],
    channels: Vec<Channel>,
}

impl OplSynth {
    fn new(sample_rate: f32) -> Self {
        let mut waves = [[0.0; SAMPLE_COUNT_WAVE]; 8];
        for index in 0..SAMPLE_COUNT_WAVE {
            let angle = 2.0 * std::f32::consts::PI * index as f32 / SAMPLE_COUNT_WAVE as f32;
            let sine = angle.sin();
            waves[WaveType::Sine as usize][index] = sine;
            waves[WaveType::HalfSine as usize][index] = sine.max(0.0);
            waves[WaveType::AbsSign as usize][index] = sine.abs();
            waves[WaveType::PulseSign as usize][index] =
                if angle % 6.28 < 1.57 { sine } else { 0.0 };
            waves[WaveType::SineEven as usize][index] =
                if angle % 12.56 < 6.28 { sine } else { 0.0 };
            waves[WaveType::AbsSineEven as usize][index] = if angle % 12.56 < 6.28 {
                sine.abs()
            } else {
                0.0
            };
            waves[WaveType::Square as usize][index] = if sine > 0.0 { 1.0 } else { 0.0 };
            waves[WaveType::DerivedSquare as usize][index] = if sine > 0.0 { 1.0 } else { 0.0 };
        }

        Self {
            sample_rate,
            time: 0.0,
            waves,
            channels: vec![Channel::default(); 15],
        }
    }

    fn stop_all(&mut self) {
        for channel in &mut self.channels {
            channel.a.state = KeyState::Off;
            channel.b.state = KeyState::Off;
        }
    }

    fn set_channel_config(&mut self, channel_index: usize, instrument: &MuzaxInstrument) {
        if let Some(channel) = self.channels.get_mut(channel_index) {
            channel.a.config = osc_desc_from_instrument(
                &instrument.operator_a,
                WaveType::from_u8(instrument.operator_a.wave_form),
            );
            channel.b.config = osc_desc_from_instrument(
                &instrument.operator_b,
                WaveType::from_u8(instrument.operator_b.wave_form),
            );
            channel.additive = (instrument.channel_config & 1) != 0;
            channel.feedback = usize::from((instrument.channel_config & 0x0E) >> 1);
            channel.feedback_factor = if channel.feedback > 0 {
                2.0f32.powi(channel.feedback as i32 + 8)
            } else {
                0.0
            };
            let radians_per_wave = 2.0 * std::f32::consts::PI;
            let dbu_per_wave = 1024.0 * 16.0;
            let vol_as_dbu = 1.0 * 0x4000 as f32 * 0x10000 as f32 / 0x4000 as f32;
            channel.m2 = radians_per_wave * vol_as_dbu / dbu_per_wave;
            channel.m1 = channel.m2 / 2.0 / 0x10000 as f32;
        }
    }

    fn set_channel_volume(&mut self, channel_index: usize, volume: f32) {
        if let Some(channel) = self.channels.get_mut(channel_index) {
            channel.b.config.output_level = volume;
        }
    }

    fn start_note(&mut self, channel_index: usize, freq_num: u16, block_num: u8) {
        if let Some(channel) = self.channels.get_mut(channel_index) {
            configure_osc_start(
                &mut channel.a,
                channel.freq_num,
                channel.block_num,
                freq_num,
                block_num,
            );
            configure_osc_start(
                &mut channel.b,
                channel.freq_num,
                channel.block_num,
                freq_num,
                block_num,
            );
            channel.freq_num = freq_num;
            channel.block_num = block_num;
        }
    }

    fn stop_note(&mut self, channel_index: usize) {
        if let Some(channel) = self.channels.get_mut(channel_index) {
            for osc in [&mut channel.a, &mut channel.b] {
                if osc.state != KeyState::Off {
                    osc.state = KeyState::Release;
                }
            }
        }
    }

    fn next_sample(&mut self) -> f32 {
        self.time += 1.0 / self.sample_rate;
        let mut out = 0.0;
        for channel_index in 0..self.channels.len() {
            out += self.process_channel(channel_index);
        }
        (out / 2.0).clamp(-1.0, 1.0)
    }

    fn process_channel(&mut self, channel_index: usize) -> f32 {
        let sample_rate = self.sample_rate;
        let time = self.time;
        let waves = &self.waves;
        let channel = &mut self.channels[channel_index];
        let feedback_mod =
            (channel.output_0 + channel.output_1) * channel.feedback_factor * channel.m1;
        let a = process_osc(
            sample_rate,
            time,
            waves,
            &mut channel.a,
            channel.freq_num,
            channel.block_num,
            feedback_mod,
        );
        let b = process_osc(
            sample_rate,
            time,
            waves,
            &mut channel.b,
            channel.freq_num,
            channel.block_num,
            if channel.additive {
                0.0
            } else {
                a * channel.m2
            },
        );
        channel.output_1 = channel.output_0;
        channel.output_0 = a;
        if channel.additive {
            a + b
        } else {
            b
        }
    }
}

fn process_osc(
    sample_rate: f32,
    time: f32,
    waves: &[[f32; SAMPLE_COUNT_WAVE]; 8],
    osc: &mut OscState,
    freq_num: u16,
    block_num: u8,
    modulator: f32,
) -> f32 {
    if osc.state == KeyState::Off {
        return 0.0;
    }

    let key_scale_num = usize::from(block_num) * 2 + usize::from(freq_num >> 7);
    let rof = if osc.config.key_scaling {
        key_scale_num
    } else {
        key_scale_num / 4
    };
    let get_rate = |rate: usize| -> usize {
        if rate > 0 {
            (rof + rate * 4).min(63)
        } else {
            0
        }
    };

    match osc.state {
        KeyState::Attack => {
            let rate = get_rate(osc.config.attack_rate);
            let time_to_attack = ATTACK_RATES[rate];
            if time_to_attack == 0.0 {
                osc.volume = MAX_DB;
                osc.envelope_step = 0;
                osc.state = KeyState::Decay;
            } else if time_to_attack.is_nan() {
                osc.state = KeyState::Off;
            } else {
                let steps = (time_to_attack / 1000.0 * sample_rate.recip())
                    .recip()
                    .floor()
                    .max(1.0) as usize;
                let p = 3.0;
                osc.volume = -96.0
                    * (((steps.saturating_sub(osc.envelope_step)) as f32 / steps as f32).powf(p));
                osc.envelope_step += 1;
                if osc.envelope_step >= steps {
                    osc.envelope_step = 0;
                    osc.volume = MAX_DB;
                    osc.state = KeyState::Decay;
                }
            }
        }
        KeyState::Decay => {
            let rate = get_rate(osc.config.decay_rate);
            let time_to_decay = DECAY_RATES[rate];
            if time_to_decay == 0.0 {
                osc.volume = osc.config.sustain_level;
                osc.envelope_step = 0;
                osc.state = KeyState::Sustain;
            } else if !time_to_decay.is_nan() {
                let steps = (time_to_decay / 1000.0 * sample_rate.recip())
                    .recip()
                    .floor()
                    .max(1.0) as usize;
                let decrease_amt = osc.config.sustain_level / steps as f32;
                osc.volume += decrease_amt;
                osc.envelope_step += 1;
                if osc.envelope_step >= steps {
                    osc.envelope_step = 0;
                    osc.state = KeyState::Sustain;
                }
            }
        }
        KeyState::Sustain => {
            if !osc.config.sound_sustaining {
                osc.state = KeyState::Release;
            }
        }
        KeyState::Release => {
            let rate = get_rate(osc.config.release_rate);
            let time_to_release = DECAY_RATES[rate];
            let steps = (time_to_release / 1000.0 * sample_rate.recip())
                .recip()
                .floor()
                .max(1.0) as usize;
            let decrease_amt = (MIN_DB - osc.config.sustain_level) / steps as f32;
            osc.volume += decrease_amt;
            osc.envelope_step += 1;
            if osc.envelope_step >= steps {
                osc.volume = MIN_DB;
                osc.state = KeyState::Off;
            }
        }
        KeyState::Off => {}
    }

    let mut ks_damping = 0.0;
    if osc.config.key_scale_level > 0 {
        let kslm = KEY_SCALE_MULTIPLIERS[osc.config.key_scale_level];
        ks_damping = -kslm * KEY_SCALE_LEVELS[usize::from(block_num)][usize::from(freq_num >> 6)];
    }

    let mut freq =
        FREQ_STARTS[usize::from(block_num)] + FREQ_STEPS[usize::from(block_num)] * freq_num as f32;
    freq *= if osc.config.multiplication == 0.0 {
        0.5
    } else {
        osc.config.multiplication
    };

    let vib = if osc.config.vibrato {
        (time * 2.0 * std::f32::consts::PI).cos() * 0.00004 + 1.0
    } else {
        1.0
    };
    osc.angle += (1.0 / sample_rate) * 2.0 * std::f32::consts::PI * freq * vib;

    let angle = osc.angle + modulator;
    let wrapped = angle.abs() % (2.0 * std::f32::consts::PI);
    let wave_index = ((wrapped * SAMPLE_COUNT_WAVE as f32) / (2.0 * std::f32::consts::PI))
        .floor()
        .min((SAMPLE_COUNT_WAVE - 1) as f32) as usize;
    let wave = waves[osc.config.wave_form as usize][wave_index];
    let tremolo = if osc.config.tremolo {
        (time * std::f32::consts::PI * 3.7).cos().abs()
    } else {
        0.0
    };
    wave * 10.0f32.powf((osc.volume + osc.config.output_level + tremolo + ks_damping) / 10.0)
}

fn configure_osc_start(
    osc: &mut OscState,
    current_freq_num: u16,
    current_block_num: u8,
    freq_num: u16,
    block_num: u8,
) {
    if current_freq_num == freq_num
        && current_block_num == block_num
        && osc.state == KeyState::Sustain
    {
        return;
    }
    osc.state = KeyState::Attack;
    osc.envelope_step = 0;
}

fn osc_desc_from_instrument(osc: &skyroads_data::MuzaxOscillator, wave_form: WaveType) -> OscDesc {
    OscDesc {
        tremolo: osc.tremolo,
        vibrato: osc.vibrato,
        sound_sustaining: osc.sound_sustaining,
        key_scaling: osc.key_scaling,
        multiplication: if osc.multiplication == 0 {
            0.0
        } else {
            osc.multiplication as f32
        },
        key_scale_level: usize::from(osc.key_scale_level),
        output_level: (osc.output_level as f32 / 0x3F as f32) * -47.25,
        attack_rate: usize::from(osc.attack_rate),
        decay_rate: usize::from(osc.decay_rate),
        sustain_level: -45.0 * osc.sustain_level as f32 / 0x0F as f32,
        release_rate: usize::from(osc.release_rate),
        wave_form,
    }
}

impl WaveType {
    fn from_u8(value: u8) -> Self {
        match value & 7 {
            0 => Self::Sine,
            1 => Self::HalfSine,
            2 => Self::AbsSign,
            3 => Self::PulseSign,
            4 => Self::SineEven,
            5 => Self::AbsSineEven,
            6 => Self::Square,
            _ => Self::DerivedSquare,
        }
    }
}

const ATTACK_RATES: [f32; 64] = [
    f32::NAN,
    f32::NAN,
    f32::NAN,
    f32::NAN,
    2826.24,
    2252.80,
    1884.16,
    1597.44,
    1413.12,
    1126.40,
    942.08,
    798.72,
    706.56,
    563.20,
    471.04,
    399.36,
    353.28,
    281.60,
    235.52,
    199.68,
    176.76,
    140.80,
    117.76,
    99.84,
    88.32,
    70.40,
    58.88,
    49.92,
    44.16,
    35.20,
    29.44,
    24.96,
    22.08,
    17.60,
    14.72,
    12.48,
    11.04,
    8.80,
    7.36,
    6.24,
    5.52,
    4.40,
    3.68,
    3.12,
    2.76,
    2.20,
    1.84,
    1.56,
    1.40,
    1.12,
    0.92,
    0.80,
    0.70,
    0.56,
    0.46,
    0.42,
    0.38,
    0.30,
    0.24,
    0.20,
    0.0,
    0.0,
    0.0,
    0.0,
];

const DECAY_RATES: [f32; 64] = [
    f32::NAN,
    f32::NAN,
    f32::NAN,
    f32::NAN,
    39280.64,
    31416.32,
    26173.44,
    22446.08,
    19640.32,
    15708.16,
    13086.72,
    11223.04,
    9820.16,
    7854.08,
    6543.36,
    5611.52,
    4910.08,
    3927.04,
    3271.68,
    2805.76,
    2455.04,
    1936.52,
    1635.84,
    1402.88,
    1227.52,
    981.76,
    817.92,
    701.44,
    613.76,
    490.88,
    488.96,
    350.72,
    306.88,
    245.44,
    204.48,
    175.36,
    153.44,
    122.72,
    102.24,
    87.68,
    76.72,
    61.36,
    51.12,
    43.84,
    38.36,
    30.68,
    25.56,
    21.92,
    19.20,
    15.36,
    12.80,
    10.96,
    9.60,
    7.68,
    6.40,
    5.48,
    4.80,
    3.84,
    3.20,
    2.74,
    2.40,
    2.40,
    2.40,
    2.40,
];

const KEY_SCALE_MULTIPLIERS: [f32; 4] = [0.0, 1.0, 0.5, 2.0];
const FREQ_STARTS: [f32; 8] = [0.047, 0.094, 0.189, 0.379, 0.758, 1.517, 3.034, 6.068];
const FREQ_STEPS: [f32; 8] = [0.048, 0.095, 0.190, 0.379, 0.759, 1.517, 3.034, 6.069];
const KEY_SCALE_LEVELS: [[f32; 16]; 8] = [
    [0.0; 16],
    [
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.75, 1.125, 1.5, 1.875, 2.25, 2.625, 3.0,
    ],
    [
        0.0, 0.0, 0.0, 0.0, 0.0, 1.875, 3.0, 4.125, 4.875, 5.625, 6.0, 6.75, 7.125, 7.5, 7.875,
        8.25,
    ],
    [
        0.0, 0.0, 0.0, 1.875, 3.0, 4.125, 4.875, 5.625, 6.0, 6.75, 7.125, 7.5, 7.875, 8.25, 8.625,
        9.0,
    ],
    [
        0.0, 0.0, 3.0, 4.875, 6.0, 7.125, 7.875, 8.625, 9.0, 9.75, 10.125, 10.5, 10.875, 11.25,
        11.625, 12.0,
    ],
    [
        0.0, 3.0, 6.0, 7.875, 9.0, 10.125, 10.875, 11.625, 12.0, 12.75, 13.125, 13.5, 13.875,
        14.25, 14.625, 15.0,
    ],
    [
        0.0, 6.0, 9.0, 10.875, 12.0, 13.125, 13.875, 14.625, 15.0, 15.75, 16.125, 16.5, 16.875,
        17.25, 17.625, 18.0,
    ],
    [
        0.0, 9.0, 12.0, 13.875, 15.0, 16.125, 16.875, 17.625, 18.0, 18.75, 19.125, 19.5, 19.875,
        20.25, 20.625, 21.0,
    ],
];

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use skyroads_core::AudioCommand;

    use super::{AttractAudioAssets, AudioMixer, AudioTimelineEvent};

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
}
