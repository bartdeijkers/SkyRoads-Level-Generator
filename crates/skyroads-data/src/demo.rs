use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::Result;

pub const DEMO_TILE_POSITION_STEP_FP16: u32 = 0x0666;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoInput {
    pub index: usize,
    pub byte: u8,
    pub accelerate_decelerate: i8,
    pub left_right: i8,
    pub jump: bool,
    pub tile_position_fp16: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JumpCounts {
    pub false_count: usize,
    pub true_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoRecording {
    pub source_len: usize,
    pub raw: Vec<u8>,
    pub entries: Vec<DemoInput>,
    pub accelerate_decelerate_counts: BTreeMap<i8, usize>,
    pub left_right_counts: BTreeMap<i8, usize>,
    pub jump_counts: JumpCounts,
}

impl DemoRecording {
    pub fn byte_count(&self) -> usize {
        self.raw.len()
    }

    pub fn approx_tile_length_fp16(&self) -> u32 {
        self.raw.len() as u32 * DEMO_TILE_POSITION_STEP_FP16
    }

    pub fn approx_tile_length(&self) -> f64 {
        self.approx_tile_length_fp16() as f64 / 65536.0
    }
}

pub fn load_demo_rec_path(path: impl AsRef<Path>) -> Result<DemoRecording> {
    let data = fs::read(path)?;
    load_demo_rec_bytes(&data)
}

pub fn load_demo_rec_bytes(data: &[u8]) -> Result<DemoRecording> {
    let mut entries = Vec::with_capacity(data.len());
    let mut accelerate_decelerate_counts = BTreeMap::new();
    let mut left_right_counts = BTreeMap::new();
    let mut jump_counts = JumpCounts {
        false_count: 0,
        true_count: 0,
    };

    for (index, &value) in data.iter().enumerate() {
        let accelerate_decelerate = ((value & 0x03) as i8) - 1;
        let left_right = (((value >> 2) & 0x03) as i8) - 1;
        let jump = ((value >> 4) & 0x01) != 0;

        *accelerate_decelerate_counts
            .entry(accelerate_decelerate)
            .or_insert(0) += 1;
        *left_right_counts.entry(left_right).or_insert(0) += 1;
        if jump {
            jump_counts.true_count += 1;
        } else {
            jump_counts.false_count += 1;
        }

        entries.push(DemoInput {
            index,
            byte: value,
            accelerate_decelerate,
            left_right,
            jump,
            tile_position_fp16: index as u32 * DEMO_TILE_POSITION_STEP_FP16,
        });
    }

    Ok(DemoRecording {
        source_len: data.len(),
        raw: data.to_vec(),
        entries,
        accelerate_decelerate_counts,
        left_right_counts,
        jump_counts,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::load_demo_rec_path;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn shipped_demo_matches_verified_counts() {
        let demo = load_demo_rec_path(repo_root().join("DEMO.REC")).unwrap();
        assert_eq!(demo.byte_count(), 6398);
        assert_eq!(demo.approx_tile_length_fp16(), 10_479_924);
        assert_eq!(demo.accelerate_decelerate_counts.get(&-1), Some(&232));
        assert_eq!(demo.accelerate_decelerate_counts.get(&0), Some(&5519));
        assert_eq!(demo.accelerate_decelerate_counts.get(&1), Some(&647));
        assert_eq!(demo.left_right_counts.get(&-1), Some(&183));
        assert_eq!(demo.left_right_counts.get(&0), Some(&5878));
        assert_eq!(demo.left_right_counts.get(&1), Some(&337));
        assert_eq!(demo.jump_counts.false_count, 5558);
        assert_eq!(demo.jump_counts.true_count, 840);
    }
}
