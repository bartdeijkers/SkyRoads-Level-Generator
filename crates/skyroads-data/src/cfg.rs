use std::fs;
use std::path::Path;

use crate::{Error, Result};

pub const SKYROADS_CFG_HEADER: [u8; 2] = [0x10, 0x02];
pub const SKYROADS_CFG_COMPLETION_COUNT: usize = 30;
const SKYROADS_CFG_SOUND_OFF: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ControlMode {
    #[default]
    Keyboard,
    Joystick,
    Mouse,
}

impl ControlMode {
    pub fn dos_value(self) -> u16 {
        match self {
            Self::Keyboard => 0,
            Self::Joystick => 1,
            Self::Mouse => 2,
        }
    }

    pub fn from_dos_value(value: u16) -> Self {
        match value {
            1 => Self::Joystick,
            2 => Self::Mouse,
            _ => Self::Keyboard,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyroadsCfg {
    pub control_mode: ControlMode,
    pub sound_enabled: bool,
    pub completion_counts: [u16; SKYROADS_CFG_COMPLETION_COUNT],
}

impl Default for SkyroadsCfg {
    fn default() -> Self {
        Self {
            control_mode: ControlMode::Keyboard,
            sound_enabled: true,
            completion_counts: [0; SKYROADS_CFG_COMPLETION_COUNT],
        }
    }
}

impl SkyroadsCfg {
    pub fn byte_count(&self) -> usize {
        6 + SKYROADS_CFG_COMPLETION_COUNT * 2
    }

    pub fn encoded_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.byte_count());
        bytes.extend_from_slice(&SKYROADS_CFG_HEADER);
        bytes.extend_from_slice(&self.control_mode.dos_value().to_le_bytes());
        let sound_raw = if self.sound_enabled {
            0u16
        } else {
            SKYROADS_CFG_SOUND_OFF
        };
        bytes.extend_from_slice(&sound_raw.to_le_bytes());
        for count in self.completion_counts {
            bytes.extend_from_slice(&count.to_le_bytes());
        }
        bytes
    }
}

pub fn load_cfg_or_default(path: impl AsRef<Path>) -> Result<SkyroadsCfg> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(SkyroadsCfg::default());
    }
    load_cfg_path(path)
}

pub fn save_cfg_path(path: impl AsRef<Path>, cfg: &SkyroadsCfg) -> Result<()> {
    fs::write(path, cfg.encoded_bytes())?;
    Ok(())
}

pub fn load_cfg_path(path: impl AsRef<Path>) -> Result<SkyroadsCfg> {
    let data = fs::read(path)?;
    load_cfg_bytes(&data)
}

pub fn load_cfg_bytes(data: &[u8]) -> Result<SkyroadsCfg> {
    if data.len() < 6 {
        return Err(Error::invalid_format(
            "SKYROADS.CFG is too short to contain the DOS header and settings",
        ));
    }
    if data[..2] != SKYROADS_CFG_HEADER {
        return Err(Error::invalid_format(format!(
            "unexpected SKYROADS.CFG header bytes {:02X} {:02X}",
            data[0], data[1]
        )));
    }

    let control_mode = ControlMode::from_dos_value(read_u16(data, 2)?);
    let sound_enabled = read_u16(data, 4)? != SKYROADS_CFG_SOUND_OFF;
    let completion_bytes = &data[6..];
    if completion_bytes.len() % 2 != 0 {
        return Err(Error::invalid_format(
            "SKYROADS.CFG completion table is not word-aligned",
        ));
    }

    let completion_word_count = completion_bytes.len() / 2;
    if completion_word_count > SKYROADS_CFG_COMPLETION_COUNT {
        return Err(Error::invalid_format(format!(
            "SKYROADS.CFG has {completion_word_count} completion counters, expected at most {SKYROADS_CFG_COMPLETION_COUNT}",
        )));
    }

    let mut completion_counts = [0u16; SKYROADS_CFG_COMPLETION_COUNT];
    for index in 0..completion_word_count {
        completion_counts[index] = read_u16(completion_bytes, index * 2)?;
    }

    Ok(SkyroadsCfg {
        control_mode,
        sound_enabled,
        completion_counts,
    })
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or(Error::UnexpectedEof("cfg word"))?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        load_cfg_bytes, load_cfg_or_default, load_cfg_path, save_cfg_path, ControlMode,
        SkyroadsCfg, SKYROADS_CFG_COMPLETION_COUNT, SKYROADS_CFG_HEADER,
    };

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn unique_temp_dir(test_name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "skyroads-data-{test_name}-{}-{timestamp}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn cfg_round_trips_thirty_completion_counters() {
        let mut completion_counts = [0u16; SKYROADS_CFG_COMPLETION_COUNT];
        completion_counts[0] = 3;
        completion_counts[12] = 7;
        completion_counts[29] = 11;
        let cfg = SkyroadsCfg {
            control_mode: ControlMode::Mouse,
            sound_enabled: false,
            completion_counts,
        };

        let encoded = cfg.encoded_bytes();
        let decoded = load_cfg_bytes(&encoded).unwrap();

        assert_eq!(encoded.len(), cfg.byte_count());
        assert_eq!(decoded, cfg);
    }

    #[test]
    fn cfg_accepts_shorter_completion_tables_and_zero_fills_remaining_roads() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&SKYROADS_CFG_HEADER);
        bytes.extend_from_slice(&ControlMode::Joystick.dos_value().to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        for value in 1u16..=24 {
            bytes.extend_from_slice(&value.to_le_bytes());
        }

        let cfg = load_cfg_bytes(&bytes).unwrap();
        assert_eq!(cfg.control_mode, ControlMode::Joystick);
        assert!(cfg.sound_enabled);
        assert_eq!(cfg.completion_counts[0], 1);
        assert_eq!(cfg.completion_counts[23], 24);
        assert_eq!(cfg.completion_counts[24], 0);
        assert_eq!(cfg.completion_counts[29], 0);
    }

    #[test]
    fn load_cfg_or_default_uses_dos_defaults_when_file_is_missing() {
        let temp_dir = unique_temp_dir("missing-cfg");
        let cfg = load_cfg_or_default(temp_dir.join("SKYROADS.CFG")).unwrap();

        assert_eq!(cfg, SkyroadsCfg::default());
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn save_cfg_path_writes_round_trippable_cfg_file() {
        let temp_dir = unique_temp_dir("save-cfg");
        let cfg_path = temp_dir.join("SKYROADS.CFG");
        let mut cfg = SkyroadsCfg::default();
        cfg.control_mode = ControlMode::Mouse;
        cfg.sound_enabled = false;
        cfg.completion_counts[5] = 9;

        save_cfg_path(&cfg_path, &cfg).unwrap();
        let loaded = load_cfg_path(&cfg_path).unwrap();

        assert_eq!(loaded, cfg);
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn shipped_cfg_loads_as_structured_data_when_present() {
        let cfg_path = repo_root().join("SKYROADS.CFG");
        if cfg_path.exists() {
            let cfg = load_cfg_path(cfg_path).unwrap();
            assert_eq!(cfg.byte_count(), 66);
        }
    }
}
